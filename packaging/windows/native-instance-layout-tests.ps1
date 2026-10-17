# SPDX-License-Identifier: AGPL-3.0-or-later
# Native/elevated disposable test. Creates two temporary SCM virtual identities,
# never existing Codifix instances. Not evidence until run on Windows.
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$ServiceHostPath,
    [Parameter(Mandatory = $true)][string]$IsolationProbePath)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows required' }
if (-not ([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Elevated disposable VM test required' }
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
$suffix = [guid]::NewGuid().ToString('N')
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) "Codifix-layout-test-$suffix"
$names = @("Codifix-test-a-$suffix", "Codifix-test-b-$suffix")
$sc = Join-Path $env:SystemRoot 'System32\sc.exe'
$created = @()
$passed = $false
function Invoke-TestSc([string[]]$Values) { $null = & $sc @Values; if ($LASTEXITCODE -ne 0) { throw 'Disposable test SCM operation failed' } }
try {
    Set-NativePrivateDirectory $root -SharedAncestor
    foreach ($part in @('instances', 'configurations', 'bin')) { Set-NativePrivateDirectory (Join-Path $root $part) -SharedAncestor }
    $hostPath = Join-Path $root 'bin\codifix-windows-service.exe'
    $probePath = Join-Path $root 'bin\codifix-windows-isolation-probe.exe'
    Copy-Item -LiteralPath $ServiceHostPath -Destination $hostPath
    Copy-Item -LiteralPath $IsolationProbePath -Destination $probePath
    for ($index = 0; $index -lt 2; $index++) {
        $name = $names[$index]; $state = Join-Path $root "instances\$index"; $authority = Join-Path $root "configurations\$index"
        $own = Join-Path $authority 'synthetic.txt'
        Set-NativePrivateDirectory $state
        Set-NativePrivateDirectory $authority
        [IO.File]::WriteAllText($own, 'synthetic')
        $command = @($hostPath, '--name', $name, '--directory', $state, '--', $probePath, '--service-layout-child', $root, $index.ToString())
        $binary = ($command | ForEach-Object { ConvertTo-NativeCommandArgument $_ }) -join ' '
        Invoke-TestSc @('create', $name, 'binPath=', $binary, 'start=', 'demand', 'obj=', "NT SERVICE\$name")
        $created += $name
        Invoke-TestSc @('sidtype', $name, 'unrestricted')
        $sid = ([Security.Principal.NTAccount]::new("NT SERVICE\$name")).Translate([Security.Principal.SecurityIdentifier]).Value
        foreach ($part in @($root, (Join-Path $root 'instances'), (Join-Path $root 'configurations'), (Join-Path $root 'bin'))) { Set-NativePrivateDirectory $part $sid -SharedAncestor }
        Set-NativePrivateDirectory $state $sid
        Set-NativePrivateDirectory $authority $sid -ReadOnly
        foreach ($executable in @($hostPath, $probePath)) {
            $acl = Get-Acl -LiteralPath $executable
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'ReadAndExecute', 'Allow'))
            Set-Acl -LiteralPath $executable -AclObject $acl
        }
    }
    # Both accounts run only AFTER the second installation has reapplied shared
    # ancestors: losing the first service ACE makes this test fail visibly.
    for ($index = 0; $index -lt 2; $index++) {
        Start-Service $names[$index]
        $result = Join-Path $root "instances\$index\result.txt"
        $until = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path -LiteralPath $result) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 100 }
        if (-not (Test-Path -LiteralPath $result) -or [IO.File]::ReadAllText($result) -cne 'own=True;siblingDenied=True;immutable=True;protectedAncestors=True') { throw 'Exact service identity layout/read/write isolation failed' }
        [IO.File]::WriteAllText((Join-Path $root "instances\$index\finish.txt"), 'observed')
        (Get-Service $names[$index]).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30))
        $null = & $probePath --operator-service-read $root $index.ToString() $names[$index]
        if ($LASTEXITCODE -ne 0) { throw 'Elevated exact-service protected reader failed' }
        # PS5.1 turns redirected native stderr into a terminating error under
        # Stop, even for this deliberately rejected sibling-reader probe.
        $savedPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            $null = & $probePath --operator-service-read $root $index.ToString() $names[(1 - $index)] 2>$null
            $deniedExit = $LASTEXITCODE
        } finally { $ErrorActionPreference = $savedPreference }
        if ($deniedExit -eq 0) { throw 'Different service reader accepted sibling authority' }
    }
    Write-Output 'Observed two virtual service identities: own read/write, immutable authority, denied sibling, ancestor grants preserved'
    $passed = $true
} catch {
    # Capture synthetic SCM diagnostics before cleanup removes the registration.
    # Never retain real customer configuration or credentials in this fixture.
    $failure = [ordered]@{message=$_.Exception.Message;inner_message=$_.Exception.InnerException.Message;services=@()}
    foreach ($name in $created) {
        $failure.services += Get-CimInstance Win32_Service -Filter ("Name='"+$name+"'") |
            Select-Object Name,State,ExitCode,ServiceSpecificExitCode,ProcessId
    }
    $failure | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $root 'failure.json') -Encoding UTF8
    throw
} finally {
    foreach ($name in $created) { Stop-Service $name -ErrorAction SilentlyContinue; $null = & $sc delete $name }
    if ($passed) { Remove-Item -LiteralPath $root -Recurse -Force }
    elseif (Test-Path -LiteralPath $root) { Write-Output ("Retained synthetic failure evidence: " + $root) }
}
