# SPDX-License-Identifier: AGPL-3.0-or-later
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
$instance = [guid]'11111111-1111-4111-8111-111111111111'
$plan = Get-NativeInstancePlan 'homelab' 'windows-v3.0.0-rc.5-build.1' 'codifix.exe' 'http://127.0.0.1:55440/healthz' $instance
if ($plan.service_name -ne 'Codifix-homelab' -or $plan.account -ne 'NT SERVICE\Codifix-homelab') { throw 'Service identity mismatch' }
foreach ($bad in @('Homelab', '../trust', 'space name', ('x' * 49))) {
    $rejected = $false
    try { $null = Get-NativeInstancePlan $bad 'windows-v3.0.0-rc.5-build.1' 'codifix.exe' 'http://127.0.0.1:55440/healthz' $instance } catch { $rejected = $true }
    if (-not $rejected) { throw 'Invalid instance name accepted' }
}
if ((ConvertTo-NativeCommandArgument 'C:\a b\') -ne '"C:\a b\\"') { throw 'Native trailing slash quoting failed' }
if ((ConvertTo-NativeCommandArgument 'a"b') -ne '"a\"b"') { throw 'Native quote escaping failed' }
$nestedBinary='"C:\Program Files\Codifix\wrapper.exe" "--name" "Codifix-synthetic"'
$nestedExpected='"\"C:\Program Files\Codifix\wrapper.exe\" \"--name\" \"Codifix-synthetic\""'
if((ConvertTo-NativeCommandArgument $nestedBinary) -cne $nestedExpected){throw 'Nested SCM binPath quoting changed'}
function Test-ScmArrayBinding([string[]]$Arguments){return $Arguments}
$bound=@(Test-ScmArrayBinding @('create','selected','binPath=','"C:\synthetic path\wrapper.exe" "--name" "synthetic"'))
if($bound.Count -ne 4 -or $bound[0] -cne 'create' -or $bound[3] -cne '"C:\synthetic path\wrapper.exe" "--name" "synthetic"'){
    throw 'SCM array expression binding dropped or changed arguments'
}
foreach($operation in @('create','config','sidtype')){
    $failure=Get-NativeScmFailure $operation 87
    if($failure.operation -cne $operation -or $failure.exit_code -ne 87 -or -not $failure.win32_message -or $failure.win32_message.Length -gt 512){
        throw 'Bounded SCM failure code diagnostic changed'
    }
    if((($failure.Keys | Sort-Object) -join ',') -cne 'exit_code,operation,win32_message'){throw 'SCM failure diagnostic included dynamic command data'}
}
foreach ($bad in @('../secret', '/absolute', 'bin\codifix.exe', 'x:y', 'AUX.json', 'bin/con.txt', 'bin/x.')) {
    $rejected = $false
    try { Assert-NativeArchiveName $bad } catch { $rejected = $true }
    if (-not $rejected) { throw "Unsafe archive name accepted: $bad" }
}
foreach ($bad in @('https://example.com/healthz', 'http://127.0.0.1:42/other', 'http://user:password@localhost/healthz', 'http://localhost/healthz?token=x')) {
    $rejected = $false
    try { $null = Get-NativeInstancePlan 'homelab' 'windows-v3.0.0-rc.5-build.1' 'codifix.exe' $bad $instance } catch { $rejected = $true }
    if (-not $rejected) { throw 'Unsafe readiness URL accepted' }
}
$digest = Get-NativeInputDigest ([ordered]@{ name = 'homelab'; source = 'synthetic' })
Assert-NativeRetry ([pscustomobject]@{ input_sha256 = $digest }) $digest
$rejected = $false
try { Assert-NativeRetry ([pscustomobject]@{ input_sha256 = $digest }) ('0' * 64) } catch { $rejected = $true }
if (-not $rejected) { throw 'Changed retry inputs accepted' }
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('codifix-native-journal-test-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path $temporary
try {
    $path = Join-Path $temporary 'journal.json'
    Write-NativeJournal $path ([ordered]@{ input_sha256 = $digest; phase = 'prepared' })
    $observed = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    Assert-NativeRetry $observed $digest
    if ($observed.phase -ne 'prepared') { throw 'Intent did not persist' }
    Write-NativeJournal $path ([ordered]@{ input_sha256 = $digest; phase = 'completed' })
    $observed = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if ($observed.phase -ne 'completed' -or $observed.input_sha256 -ne $digest) { throw 'Atomic outcome replacement failed' }
} finally { Remove-Item -LiteralPath $temporary -Recurse -Force }
Write-Output 'Native instance validation tests passed (no services changed)'
