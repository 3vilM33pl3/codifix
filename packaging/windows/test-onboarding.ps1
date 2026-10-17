# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable command/policy tests, not native logon, SCM or onboarding acceptance.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'onboarding-functions.ps1')
function Reject([scriptblock]$Action) {
    $rejected=$false; try { $null=& $Action } catch { $rejected=$true }
    if (-not $rejected) { throw 'Unsafe onboarding input accepted' }
}
$base=@{Stage='Prepare';Memory='C:\Program Files\Memory Layer\releases\pinned\bin\memory.exe';MemorySha256=('a'*64)
    ReleaseRoot='C:\Program Files\Memory Layer\releases\pinned';Config='C:\Users\Example\Memory\config\homelab.toml';ConfigSha256=('b'*64)
    Company='https://company.example.invalid';DeviceName='Example laptop';Area='homelab'
    Owner=[guid]'11111111-1111-4111-8111-111111111111';Project=[guid]'22222222-2222-4222-8222-222222222222';Revision=3
    Workspace="C:\Users\Example\Repositories\Équipe O'Brien";Journal='C:\Users\Example\Memory\journals\project.json'
    CallerSid='S-1-5-21-1-2-3-1001'}
$plan=Get-OnboardingPlan @base
if ($plan.arguments[0] -cne '--config' -or $plan.arguments -notcontains $base.Workspace -or $plan.arguments -contains '--confirm' -or
    $plan.provisions_database -or $plan.creates_memory_service -or $plan.enables_paid_models -or $plan.onboarding_acceptance_verified) { throw 'Prepare changed consent or prerequisite boundary' }
foreach ($change in @(@{Area='trust'},@{Company='https://user:secret@company.example.invalid'},@{Company='http://company.example.invalid'},
    @{Config='C:\Private\config.toml:secret'},@{Workspace='C:\Private\..\other'},@{Journal='C:\Private\NUL.json'},
    @{Memory='C:\Other\memory.exe'},@{Revision=0},@{CallerSid='S-1-5-18'},@{MemorySha256=('A'*64)})) {
    $copy=$base.Clone(); foreach ($key in $change.Keys) { $copy[$key]=$change[$key] }
    Reject { Get-OnboardingPlan @copy }
}
$apply=$base.Clone();$apply.Stage='Apply';$apply.WorkspacePlanSha256=('c'*64)
if ((Get-OnboardingPlan @apply).arguments -notcontains '--confirm') { throw 'Exact apply confirmation omitted' }
$apply.WorkspacePlanSha256='';Reject { Get-OnboardingPlan @apply }
$subscription=$base.Clone();$subscription.Stage='SubscriptionReview';$subscription.SubscriptionOutput='C:\Users\Example\Memory\subscriptions\metadata.json'
if ((Get-OnboardingPlan @subscription).arguments -contains '--confirm') { throw 'Subscription review granted disclosure' }
$subscription.Stage='SubscriptionConfirm';Reject { Get-OnboardingPlan @subscription }
$subscription.BootstrapSha256=('d'*64)
if ((Get-OnboardingPlan @subscription).arguments -notcontains '--confirm') { throw 'Explicit subscription confirmation omitted' }
$profile=[pscustomobject]@{schema_version=1;replication_mode='curated_project_v1';company=$base.Company
    subscription='33333333-3333-4333-8333-333333333333';local_database_url='postgresql://local:PRIVATE_SENTINEL@127.0.0.1:55473/memory_homelab'
    bootstrap=[pscustomobject]@{device='44444444-4444-4444-8444-444444444444';scope=[pscustomobject]@{
        company='55555555-5555-4555-8555-555555555555';space='66666666-6666-4666-8666-666666666666';area=$base.Area;owner=$base.Owner.ToString();project=$base.Project.ToString()}}}
$scope=Get-OnboardingProfileScope $profile
if (($scope | ConvertTo-Json) -match 'PRIVATE_SENTINEL|local_database_url') { throw 'Profile scope disclosed credentials' }
$sync=$base.Clone();$sync.Stage='EnableSync';$sync.Profile='C:\Users\Example\Memory\profiles\homelab.json';$sync.ProfileSha256=('e'*64);$sync.ProfileScope=$scope
$syncPlan=Get-OnboardingPlan @sync
$digest=Get-NativeInputDigest $syncPlan
$encoded=Get-OnboardingSyncCommand $syncPlan
$script=[Text.Encoding]::Unicode.GetString([Convert]::FromBase64String(($encoded -split ' ')[-1]))
if ($script -notmatch 'Get-FileHash' -or $script -notmatch "'projects' 'sync' '--profile'" -or $script -notmatch "'--watch'" -or
    $script -match 'PRIVATE_SENTINEL|Invoke-Expression|Set-ExecutionPolicy' -or $encoded -notmatch '-ExecutionPolicy Bypass') { throw 'Sync task program changed fixed execution boundary' }
# Literal generation must not turn apostrophes or code-looking paths into code.
if ((ConvertTo-OnboardingLiteral "C:\O'Brien\x") -cne "'C:\O''Brien\x'") { throw 'PowerShell literal escaping failed' }
foreach ($key in @('device','subscription','company','space')) {
    $altered=[ordered]@{};foreach($name in $scope.Keys){$altered[$name]=$scope[$name]};$altered[$key]=[guid]::NewGuid().ToString();$copy=$sync.Clone();$copy.ProfileScope=$altered
    if ((Get-OnboardingPlan @copy).task.name -ceq $syncPlan.task.name) { throw 'Task identity lost a subscription scope component' }
}
$wrong=[ordered]@{};foreach($name in $scope.Keys){$wrong[$name]=$scope[$name]};$wrong.owner=[guid]::NewGuid().ToString();$copy=$sync.Clone();$copy.ProfileScope=$wrong;Reject { Get-OnboardingPlan @copy }
$badProfile=$profile | ConvertTo-Json -Depth 8 | ConvertFrom-Json;$badProfile.local_database_url='postgresql://local@remote.invalid:5432/db';Reject { Get-OnboardingProfileScope $badProfile }
$powerShell='C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
$task=[pscustomobject]@{Description=('Memory Layer reviewed device sync '+$digest);TaskName=$syncPlan.task.name
    Principal=[pscustomobject]@{UserId=$base.CallerSid;LogonType='Interactive';RunLevel='Limited'}
    Actions=@([pscustomobject]@{Execute=$powerShell;Arguments=$encoded;WorkingDirectory=$base.ReleaseRoot})
    Triggers=@([pscustomobject]@{CimClass=[pscustomobject]@{CimClassName='MSFT_TaskLogonTrigger'};UserId=$base.CallerSid;Enabled=$true})
    Settings=[pscustomobject]@{MultipleInstances='IgnoreNew';Enabled=$true;ExecutionTimeLimit='PT0S';RestartCount=3;AllowDemandStart=$true}}
$expected=$task | ConvertTo-Json -Depth 12 | ConvertFrom-Json
Assert-OnboardingTask $task $syncPlan $powerShell $encoded $digest $expected
$task.Settings.AllowDemandStart=$false;Reject { Assert-OnboardingTask $task $syncPlan $powerShell $encoded $digest $expected }
$task.Settings.AllowDemandStart=$true;$task.Principal.RunLevel='Highest';Reject { Assert-OnboardingTask $task $syncPlan $powerShell $encoded $digest $expected }
$task.Principal.RunLevel='Limited';$task.Actions[0].Arguments+=' changed';Reject { Assert-OnboardingTask $task $syncPlan $powerShell $encoded $digest $expected }
# Exact registration normalization observed on the native VM, not a general
# omission of principal IDs, trigger identity, repetition or future properties.
$expected.Principal | Add-Member -NotePropertyName Id -NotePropertyValue $null
$expected.Triggers[0] | Add-Member -NotePropertyName Repetition -NotePropertyValue $null
$registered=$expected | ConvertTo-Json -Depth 12 | ConvertFrom-Json
$registered.Principal.Id='Author'
$registered.Triggers[0].UserId='MEMORY-WINLAB\memorylab'
$registered.Triggers[0].Repetition=[pscustomobject]@{Duration=$null;Interval=$null;StopAtDurationEnd=$false}
$resolve={param($User) if($User -cne 'MEMORY-WINLAB\memorylab'){throw 'unknown synthetic account'};return 'S-1-5-21-1-2-3-1001'}
Assert-OnboardingTask $registered $syncPlan $powerShell $encoded $digest $expected $resolve
$wrong={param($User) 'S-1-5-21-1-2-3-9999'}
Reject { Assert-OnboardingTask $registered $syncPlan $powerShell $encoded $digest $expected $wrong }
foreach($change in @('foreign-id','interval','duration','stop','extra-repetition','foreign-sid','unknown-user','highest','settings','action')) {
    $copy=$registered | ConvertTo-Json -Depth 12 | ConvertFrom-Json
    switch($change){
        'foreign-id' {$copy.Principal.Id='OtherPrincipal'}
        'interval' {$copy.Triggers[0].Repetition.Interval='PT5M'}
        'duration' {$copy.Triggers[0].Repetition.Duration='PT1H'}
        'stop' {$copy.Triggers[0].Repetition.StopAtDurationEnd=$true}
        'extra-repetition' {$copy.Triggers[0].Repetition | Add-Member -NotePropertyName Extra -NotePropertyValue 'foreign'}
        'foreign-sid' {$copy.Triggers[0].UserId='S-1-5-21-1-2-3-9999'}
        'unknown-user' {$copy.Triggers[0].UserId='OTHER\someone'}
        'highest' {$copy.Principal.RunLevel='Highest'}
        'settings' {$copy.Settings.AllowDemandStart=$false}
        'action' {$copy.Actions[0].Arguments+=' changed'}
    }
    Reject { Assert-OnboardingTask $copy $syncPlan $powerShell $encoded $digest $expected $resolve }
}
# Native CIM exposes enum numbers and a bare local SAM principal. Only these
# exact enum values have the same semantics as the reviewed symbolic defaults.
$numeric=$registered | ConvertTo-Json -Depth 12 | ConvertFrom-Json
$numeric.Principal.UserId='memorylab';$numeric.Principal.LogonType=3;$numeric.Principal.RunLevel=0;$numeric.Settings.MultipleInstances=2
$resolvedNames=[Collections.Generic.List[string]]::new()
$resolveLocal={param($User)
    $resolvedNames.Add($User)
    if($User -cnotin @('MEMORY-WINLAB\memorylab',([Environment]::MachineName+'\memorylab'))){throw 'unknown local identity'}
    'S-1-5-21-1-2-3-1001'
}
Assert-OnboardingTask $numeric $syncPlan $powerShell $encoded $digest $expected $resolveLocal
if(-not $resolvedNames.Contains([Environment]::MachineName+'\memorylab') -or $resolvedNames.Contains('memorylab')) { throw 'Bare SAM lookup used an ambient domain' }
foreach($change in @('logon0','logon2','logon4','level1','instances0','instances1','instances3','foreign-principal','foreign-principal-sid','upn-principal','new-principal-policy')) {
    $copy=$numeric | ConvertTo-Json -Depth 12 | ConvertFrom-Json
    switch($change){
        'logon0' {$copy.Principal.LogonType=0}
        'logon2' {$copy.Principal.LogonType=2}
        'logon4' {$copy.Principal.LogonType=4}
        'level1' {$copy.Principal.RunLevel=1}
        'instances0' {$copy.Settings.MultipleInstances=0}
        'instances1' {$copy.Settings.MultipleInstances=1}
        'instances3' {$copy.Settings.MultipleInstances=3}
        'foreign-principal' {$copy.Principal.UserId='someone-else'}
        'foreign-principal-sid' {$copy.Principal.UserId='S-1-5-21-1-2-3-9999'}
        'upn-principal' {$copy.Principal.UserId='memorylab@other.example'}
        'new-principal-policy' {$copy.Principal | Add-Member -NotePropertyName ProcessTokenSidType -NotePropertyValue 2}
    }
    Reject { Assert-OnboardingTask $copy $syncPlan $powerShell $encoded $digest $expected $resolveLocal }
}
Write-Output 'Portable Windows onboarding policy checks passed; native execution remains unverified.'
