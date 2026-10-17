# SPDX-License-Identifier: AGPL-3.0-or-later
# Registration/readback only: no login, database, synchronization or task start.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$SourceRoot,
      [Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$') { throw 'Disposable lab source and operation required' }
. (Join-Path $SourceRoot 'packaging\windows\native-instance-functions.ps1')
. (Join-Path $SourceRoot 'packaging\windows\onboarding-functions.ps1')
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\onboarding-task')
if (Test-Path -LiteralPath $root) { throw 'Retain existing task proof; use a new identity' }
New-Item -ItemType Directory -Path $root | Out-Null
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Task proof evidence protection failed' }
$owner=[guid]::NewGuid(); $project=[guid]::NewGuid()
$scope=[ordered]@{company_origin='https://synthetic.example.invalid';company=[guid]::NewGuid().ToString();device=[guid]::NewGuid().ToString()
    subscription=$OperationId.ToString();space=[guid]::NewGuid().ToString();area='homelab';owner=$owner.ToString();project=$project.ToString()
    local_database=[ordered]@{host='127.0.0.1';port=55471;database='synthetic_not_connected'}}
$plan=Get-OnboardingPlan -Stage EnableSync -Memory "$root\memory.exe" -MemorySha256 ('a'*64) -ReleaseRoot $root `
    -Config "$root\config.toml" -ConfigSha256 ('b'*64) -Company $scope.company_origin -Area homelab -Owner $owner -Project $project `
    -Profile "$root\profile.json" -ProfileSha256 ('c'*64) -ProfileScope $scope -CallerSid $sid
$digest=Get-NativeInputDigest $plan
$arguments=Get-OnboardingSyncCommand $plan
$powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$action=New-ScheduledTaskAction -Execute $powershell -Argument $arguments -WorkingDirectory $root
# The synthetic action cannot pass its non-existent pinned input check. It is
# never started by this proof; no application installation is created.
$principal=New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
$trigger=New-ScheduledTaskTrigger -AtLogOn -User $sid
$settings=New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval ([TimeSpan]::FromMinutes(1)) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$task=New-ScheduledTask -Action $action -Principal $principal -Trigger $trigger -Settings $settings -Description ('Memory Layer reviewed device sync '+$digest)
if (Get-ScheduledTask -TaskPath '\' -TaskName $plan.task.name -ErrorAction SilentlyContinue) { throw 'Foreign task exists; no adoption' }
$registered=$false
try {
    $null=Register-ScheduledTask -TaskPath '\' -TaskName $plan.task.name -InputObject $task
    $registered=$true
    $installed=Get-ScheduledTask -TaskPath '\' -TaskName $plan.task.name
    $resolveUser={ param($User) ([Security.Principal.NTAccount]::new($User)).Translate([Security.Principal.SecurityIdentifier]).Value }
    $expected=Get-OnboardingTaskPolicy $task $sid $resolveUser
    $actual=Get-OnboardingTaskPolicy $installed $sid $resolveUser
    @{format='memory-native-onboarding-task-proof-v1';task=$plan.task.name;expected=$expected;actual=$actual;state='observed_not_yet_verified'} |
        ConvertTo-Json -Depth 24 | Set-Content -LiteralPath (Join-Path $root 'policy.json') -Encoding UTF8
    Assert-OnboardingTask $installed $plan $powershell $arguments $digest $task
    @{format='memory-native-onboarding-task-proof-v1';state='succeeded';registration_roundtrip_verified=$true;task_started=$false;
      login_verified=$false;database_connected=$false;onboarding_verified=$false;observed_at=[DateTime]::UtcNow.ToString('o')} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'result.json') -Encoding UTF8
} finally {
    if ($registered) {
        $current=Get-ScheduledTask -TaskPath '\' -TaskName $plan.task.name
        if ($current.Description -cne ('Memory Layer reviewed device sync '+$digest) -or
            @($current.Actions).Count -ne 1 -or $current.Actions[0].Arguments -cne $arguments) { throw 'Task changed; retain for explicit inspection' }
        Unregister-ScheduledTask -TaskPath '\' -TaskName $plan.task.name -Confirm:$false
    }
}
