# SPDX-License-Identifier: AGPL-3.0-or-later
# Synthetic native API tests in the logged-in lab user's session, never S4U.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or $OperationId -eq [guid]::Empty) {
    throw 'Native API proof is restricted to the disposable Memory VM'
}
$inputs = 'C:\MemoryLab\inputs'
$root = Join-Path (Join-Path 'C:\MemoryLab\runs' $OperationId.ToString()) 'native-api'
New-Item -ItemType Directory -Path $root | Out-Null
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
if ($identity -ine 'MEMORY-WINLAB\memorylab' -or $sessionId -eq 0) {
    throw 'Native API tests require the interactive memorylab logon, not a service token'
}
& icacls.exe $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Native API results ACL failed' }
foreach ($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw 'Native API tests require a provider-credential-free environment' }
}
$manifestPath = Join-Path $inputs 'native-api.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$kinds = 'desktop,isolation-probe,onboarding,platform,protected-files,routing'
if ($manifest.format -ne 'memory-windows-native-api-v1' -or
    (@($manifest.artifacts | ForEach-Object { $_.kind } | Sort-Object) -join ',') -ne $kinds) {
    throw 'Native API manifest requires exactly the six reviewed executable kinds'
}
$names = @{}
foreach ($artifact in $manifest.artifacts) {
    if ($artifact.file -cnotmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,179}\.exe$' -or
        $artifact.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $names.ContainsKey($artifact.file)) {
        throw 'Unsafe or duplicate native API executable binding'
    }
    $names[$artifact.file] = $true
    if ((Get-FileHash -LiteralPath (Join-Path $inputs $artifact.file)).Hash.ToLowerInvariant() -ne $artifact.sha256) {
        throw 'Native API executable differs from its reviewed digest'
    }
}
$receipt = [ordered]@{format='memory-windows-native-api-result-v1';operation_id=$OperationId.ToString();manifest_sha256=(Get-FileHash $manifestPath).Hash.ToLowerInvariant();identity=$identity;sid=$sid;session_id=$sessionId;state='running';tests=@();failed_filters=@();execution_verified=$false}
function Save-Result {
    $partial = Join-Path $root 'result.partial.json'
    $target = Join-Path $root 'result.json'
    $receipt | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $partial -Encoding UTF8
    # Windows PowerShell's Move-Item -Force is not a dependable replacement
    # primitive. ReplaceFile preserves the old receipt until the new one wins.
    if ([IO.File]::Exists($target)) { [IO.File]::Replace($partial, $target, [NullString]::Value) }
    else { [IO.File]::Move($partial, $target) }
}
function Invoke-Bounded([string]$Exe, [string[]]$Arguments, [string]$Label, [int]$Seconds) {
    $stdout = Join-Path $root ($Label + '.stdout.txt')
    $stderr = Join-Path $root ($Label + '.stderr.txt')
    $start = @{FilePath=$Exe;RedirectStandardOutput=$stdout;RedirectStandardError=$stderr;PassThru=$true;NoNewWindow=$true}
    if ($Arguments.Count) { $start.ArgumentList = $Arguments }
    $process = Start-Process @start
    $null = $process.Handle
    $timedOut = -not $process.WaitForExit($Seconds * 1000)
    if ($timedOut) {
        # Only this test process tree; no name-based process termination.
        & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
        $process.WaitForExit(10000) | Out-Null
    } else { $process.WaitForExit() }
    $exitCode = if ($timedOut) { $null } else { $process.ExitCode }
    $result = [ordered]@{label=$Label;exit_code=$exitCode;timed_out=$timedOut;stdout=[IO.Path]::GetFileName($stdout);stderr=[IO.Path]::GetFileName($stderr)}
    $receipt.tests += $result
    Save-Result
    if ($timedOut -or $exitCode -ne 0) { throw 'Native API test failed; inspect retained per-filter results' }
    return Get-Content -LiteralPath $stdout -Raw
}
Save-Result
try {
    # Keep the mandatory adversarial probe last so a sandbox setup failure does
    # not hide independent filesystem/credential/routing diagnostics. It still
    # must pass before execution_verified can become true.
    foreach ($artifact in ($manifest.artifacts | Sort-Object { if ($_.kind -eq 'isolation-probe') { 'zz-isolation' } else { $_.kind } })) {
        $exe = Join-Path $inputs $artifact.file
        if ($artifact.kind -eq 'isolation-probe') {
            # Synthetic loopback denial probe only; no provider/service traffic.
            try { $null = Invoke-Bounded $exe @() 'isolation-probe' 120 }
            catch { $receipt.failed_filters += 'isolation-probe'; Save-Result }
            continue
        }
        $filters = switch ($artifact.kind) {
            'protected-files' { @('tests::') }
            'platform' { @('tests::') }
            'routing' { @('area::tests') }
            'desktop' { @('collector::codex::windows::native::tests') }
            'onboarding' { @('credentials::windows::tests','workspace::filesystem::windows::tests','agents::settings::filesystem::tests','runtime_env_windows::tests') }
        }
        $index = 0
        foreach ($filter in $filters) {
            $label = $artifact.kind + '-' + $index
            try {
                $listed = Invoke-Bounded $exe @($filter,'--list') ($label + '-list') 30
                if (-not ($listed -split '\r?\n' | Where-Object { $_ -match ': test$' })) {
                    throw 'Native API filter selected zero tests'
                }
                $output = Invoke-Bounded $exe @($filter,'--test-threads=1') $label 120
                if ($output -notmatch 'test result: ok\. ([1-9][0-9]*) passed; 0 failed;') {
                    throw 'Native API filter did not report positive passing test counts'
                }
            } catch {
                # Each filter uses independent synthetic fixtures. Retain its
                # failure and continue diagnostics, never turn it into a pass
                # or suppress the mandatory probe. No automatic retries.
                $receipt.failed_filters += $label
                Save-Result
            }
            $index++
        }
    }
    if ($receipt.failed_filters.Count) { throw 'Native API suite has failed filters; inspect retained results' }
    $receipt.state = 'succeeded'
    $receipt.execution_verified = $true
} catch {
    $receipt.state = 'failed'
    throw
} finally {
    $receipt.completed_at = [DateTime]::UtcNow.ToString('o')
    Save-Result
}
