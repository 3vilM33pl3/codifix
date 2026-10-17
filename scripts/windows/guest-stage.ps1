# SPDX-License-Identifier: AGPL-3.0-or-later
# Long lab stages survive SSH disconnects. No passwords or user-supplied commands.
[CmdletBinding()]
param(
    [ValidateSet('Start','Status','Execute')][string]$Action = 'Status',
    [ValidateSet('BuildTools','PostgresPrepare','PostgresStart','PdsPrepare','PdsVerify','PiBuild','ProductCheck','ProductBuild','NativeApi','NativeEval')][string]$Stage = 'BuildTools',
    [Parameter(Mandatory=$true)][guid]$OperationId,
    [string]$SourceRoot
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'Stage execution is restricted to the disposable Memory VM'
}
if ($OperationId -eq [guid]::Empty) { throw 'A stable operation UUID is required' }
$root = Join-Path 'C:\MemoryLab\runs' $OperationId.ToString()
$descriptor = Join-Path $root 'input.json'
$statusPath = Join-Path $root 'status.json'
$taskName = 'MemoryLab-' + $OperationId.ToString()
function Get-BoundFiles([string]$SelectedStage) {
    switch ($SelectedStage) {
        'BuildTools' { @('guest-build-tools.ps1','build-tools.json') }
        { $_ -in @('PostgresPrepare','PostgresStart') } { @('guest-native-postgres.ps1','native-postgres.json') }
        'PdsPrepare' { @('guest-native-pds.ps1','native-pds.json') }
        'PdsVerify' { @('guest-pds-stdio.ps1') }
        'PiBuild' { @('guest-native-pi.ps1','native-pi.json') }
        { $_ -in @('ProductCheck','ProductBuild') } { @('guest-native-product.ps1') }
        'NativeEval' { @('guest-native-eval.ps1','guest-evaluation-proof.mjs','evaluation-helper.exe','r3_score.exe') }
        'NativeApi' {
            @('guest-native-api.ps1','native-api.json')
            $api = Get-Content -LiteralPath 'C:\MemoryLab\inputs\native-api.json' -Raw | ConvertFrom-Json
            if ($api.format -ne 'memory-windows-native-api-v1' -or
                (@($api.artifacts | ForEach-Object { $_.kind } | Sort-Object) -join ',') -ne 'desktop,federation-pipe,isolation-probe,onboarding,platform,protected-files,routing') {
                throw 'Native API manifest requires exactly seven executable kinds'
            }
            $seen = @{}
            foreach ($artifact in ($api.artifacts | Sort-Object kind)) {
                if ($artifact.file -cnotmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,179}\.exe$' -or
                    $artifact.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $seen.ContainsKey($artifact.file) -or
                    (Get-FileHash -LiteralPath (Join-Path 'C:\MemoryLab\inputs' $artifact.file)).Hash.ToLowerInvariant() -ne $artifact.sha256) {
                    throw 'Invalid native API executable binding'
                }
                $seen[$artifact.file] = $true
                $artifact.file
            }
        }
        default { throw 'Unknown native proof stage' }
    }
}
function Save-Status([string]$State, [string]$Reason) {
    $partial = Join-Path $root 'status.partial.json'
    @{format='memory-windows-lab-stage-status-v1';operation_id=$OperationId.ToString();stage=$Stage;state=$State;reason=$Reason;observed_at=[DateTime]::UtcNow.ToString('o')} |
        ConvertTo-Json | Set-Content $partial -Encoding UTF8
    if ([IO.File]::Exists($statusPath)) { [IO.File]::Replace($partial, $statusPath, [NullString]::Value) }
    else { [IO.File]::Move($partial, $statusPath) }
}
if ($Action -eq 'Status') {
    if (Test-Path -LiteralPath $statusPath) {
        $observed = Get-Content -LiteralPath $statusPath -Raw | ConvertFrom-Json
        if ($observed.state -eq 'running') {
            $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
            if (-not $task -or $task.State -ne 'Running') {
                $observed.state = 'uncertain'
                $observed.reason = 'task_ended_without_completion_receipt'
            }
        }
        $observed | ConvertTo-Json
    }
    else { @{operation_id=$OperationId.ToString();state='unknown'} | ConvertTo-Json }
    return
}
if ($Action -eq 'Start') {
    if ($SourceRoot -and ($SourceRoot -notlike 'C:\MemoryLab\*' -or $SourceRoot.Contains('"') -or $SourceRoot.Contains('..'))) {
        throw 'SourceRoot must be a copied lab source directory'
    }
    $boundFiles = @(Get-BoundFiles $Stage)
    $fileHashes = @($boundFiles | ForEach-Object { [ordered]@{name=$_;sha256=(Get-FileHash (Join-Path 'C:\MemoryLab\inputs' $_)).Hash.ToLowerInvariant()} })
    $inputRecord = [ordered]@{format='memory-windows-lab-stage-v1';operation_id=$OperationId.ToString();stage=$Stage;source_root=$SourceRoot;runner_sha256=(Get-FileHash $PSCommandPath).Hash.ToLowerInvariant();files=$fileHashes}
    if (Test-Path -LiteralPath $descriptor) {
        $old = Get-Content -LiteralPath $descriptor -Raw | ConvertFrom-Json
        if ($old.stage -ne $Stage -or $old.source_root -ne $SourceRoot -or $old.runner_sha256 -ne $inputRecord.runner_sha256 -or
            (ConvertTo-Json -InputObject $old.files -Compress) -ne (ConvertTo-Json -InputObject $fileHashes -Compress)) {
            throw 'Existing stage operation has different inputs; no new execution'
        }
        Get-Content -LiteralPath $statusPath -Raw
        return
    }
    New-Item -ItemType Directory -Force $root | Out-Null
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    & icacls.exe $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Stage directory protection failed' }
    $inputRecord | ConvertTo-Json | Set-Content -LiteralPath $descriptor -Encoding UTF8
    Save-Status 'queued' 'local_scheduled_task'
    $arguments = '-NoProfile -ExecutionPolicy Bypass -File "' + $PSCommandPath + '" -Action Execute -OperationId ' + $OperationId.ToString()
    $actionSpec = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument $arguments
    # Product/API checks model an ordinary interactive user, not an elevated
    # installer. Only explicitly selected installer/dependency stages elevate.
    $runLevel = if ($Stage -in @('NativeApi','PiBuild','ProductCheck','ProductBuild','PdsVerify','NativeEval')) { 'Limited' } else { 'Highest' }
    $principal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel $runLevel
    $limit = if ($Stage -eq 'NativeApi') { New-TimeSpan -Minutes 20 } else { New-TimeSpan -Hours 3 }
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit $limit -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $taskName -Action $actionSpec -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName $taskName
    Get-Content -LiteralPath $statusPath -Raw
    return
}
$inputRecord = Get-Content -LiteralPath $descriptor -Raw | ConvertFrom-Json
if ($inputRecord.format -ne 'memory-windows-lab-stage-v1' -or
    $inputRecord.operation_id -ne $OperationId.ToString() -or
    $inputRecord.runner_sha256 -ne (Get-FileHash $PSCommandPath).Hash.ToLowerInvariant()) { throw 'Stage descriptor changed' }
$Stage = $inputRecord.stage
$SourceRoot = $inputRecord.source_root
$inputs = 'C:\MemoryLab\inputs'
if ((@($inputRecord.files | ForEach-Object { $_.name }) -join ',') -ne (@(Get-BoundFiles $Stage) -join ',')) {
    throw 'Queued native proof file inventory changed; no execution'
}
foreach ($file in $inputRecord.files) {
    if ($file.sha256 -ne (Get-FileHash (Join-Path $inputs $file.name)).Hash.ToLowerInvariant()) {
        throw 'Queued native proof input changed; no execution'
    }
}
# Pi and the product use disjoint source/output/cache trees. Permit these two
# bounded two-job compilers alongside dependency/API observation; serialize
# repeated attempts of each family and all shared dependency mutations.
$mutexName = switch ($Stage) {
    'PiBuild' { 'Local\MemoryLab-NativePiBuild' }
    { $_ -in @('ProductCheck','ProductBuild') } { 'Local\MemoryLab-NativeProductBuild' }
    default { 'Local\MemoryLab-NativeBuildStage' }
}
$mutex = [Threading.Mutex]::new($false, $mutexName)
$held = $false
try {
    try { $held = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $held = $true }
    if (-not $held) { Save-Status 'blocked' 'another_lab_stage_is_running'; return }
    if (Test-Path $statusPath) {
        $old = Get-Content $statusPath -Raw | ConvertFrom-Json
        if ($old.state -eq 'succeeded') { return }
        if ($old.state -eq 'running') { Save-Status 'uncertain' 'previous_execution_interrupted'; return }
    }
    Save-Status 'running' 'native_execution'
    Start-Transcript -Path (Join-Path $root 'execution.log') -Append | Out-Null
    try {
        switch ($Stage) {
            'BuildTools' { & (Join-Path $inputs 'guest-build-tools.ps1') -ManifestPath (Join-Path $inputs 'build-tools.json') }
            'PostgresPrepare' { & (Join-Path $inputs 'guest-native-postgres.ps1') -ManifestPath (Join-Path $inputs 'native-postgres.json') -Action Prepare }
            'PostgresStart' { & (Join-Path $inputs 'guest-native-postgres.ps1') -ManifestPath (Join-Path $inputs 'native-postgres.json') -Action Start }
            'PdsPrepare' {
                if (-not $SourceRoot) { throw 'Copied lab source is required' }
                & (Join-Path $inputs 'guest-native-pds.ps1') -ManifestPath (Join-Path $inputs 'native-pds.json') -SourceRoot $SourceRoot
            }
            'NativeApi' { & (Join-Path $inputs 'guest-native-api.ps1') -OperationId $OperationId }
            'PiBuild' {
                if (-not $SourceRoot) { throw 'Copied lab source is required' }
                $piArguments = @{ManifestPath=(Join-Path $inputs 'native-pi.json');SourceRoot=$SourceRoot;OperationId=$OperationId}
                & (Join-Path $inputs 'guest-native-pi.ps1') -Action Prepare @piArguments
                & (Join-Path $inputs 'guest-native-pi.ps1') -Action Verify @piArguments
            }
            { $_ -in @('ProductCheck','ProductBuild') } {
                if (-not $SourceRoot) { throw 'Copied lab source is required' }
                $productAction=if ($Stage -eq 'ProductBuild') { 'Build' } else { 'Check' }
                & (Join-Path $inputs 'guest-native-product.ps1') -SourceRoot $SourceRoot -OperationId $OperationId -Action $productAction
            }
            'NativeEval' {
                if (-not $SourceRoot) { throw 'Copied lab source is required' }
                & (Join-Path $inputs 'guest-native-eval.ps1') -SourceRoot $SourceRoot -OperationId $OperationId
            }
            'PdsVerify' { & (Join-Path $inputs 'guest-pds-stdio.ps1') -OperationId $OperationId }
            default { throw 'Unknown native proof stage' }
        }
        Save-Status 'succeeded' 'stage_completed'
    } catch {
        Save-Status 'failed' 'inspect_private_execution_log'
        throw
    } finally { Stop-Transcript | Out-Null }
} finally {
    if ($held) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
