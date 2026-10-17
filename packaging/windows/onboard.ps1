# SPDX-License-Identifier: AGPL-3.0-or-later
# Explicit native Windows stages for an already-provisioned local installation.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Login','Catalog','Prepare','Apply','SubscriptionReview','SubscriptionConfirm','SyncOnce','SyncStatus','EnableSync')][string]$Stage,
    [Parameter(Mandatory=$true)][string]$Memory,
    [Parameter(Mandatory=$true)][string]$MemorySha256,
    [Parameter(Mandatory=$true)][string]$ReleaseRoot,
    [Parameter(Mandatory=$true)][string]$Config,
    [Parameter(Mandatory=$true)][string]$ConfigSha256,
    [Parameter(Mandatory=$true)][string]$Company,
    [string]$DeviceName, [string]$Area, [guid]$Owner, [guid]$Project, [long]$Revision,
    [string]$Workspace, [string]$Journal, [string]$WorkspacePlanSha256,
    [string]$SubscriptionOutput, [string]$BootstrapSha256,
    [string]$Profile, [string]$ProfileSha256,
    [switch]$Execute, [string]$ApprovalSha256
)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows interactive user required' }
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'onboarding-functions.ps1')
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if ($identity.IsSystem -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) { throw 'Run in the signed-in user session, never SYSTEM or a service' }
if (([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run onboarding unelevated; machine provisioning belongs to the separate installer' }
$sid=$identity.User.Value
$guards=[Collections.Generic.List[IDisposable]]::new()
try {
    foreach ($path in @($ReleaseRoot,$Config)) { Assert-OnboardingPath $path }
    Initialize-NativeReleaseGuard
    $guards.Add([MemoryLayer.ProtectedRelease]::Open($ReleaseRoot))
    $guards.Add([MemoryLayer.ProtectedRelease]::Open((Split-Path -Parent $Config)))
    foreach ($pair in @(@($Memory,$MemorySha256),@($Config,$ConfigSha256))) {
        Assert-OnboardingHash $pair[1]
        if ((Get-FileHash -LiteralPath $pair[0]).Hash.ToLowerInvariant() -cne $pair[1]) { throw 'Installed Memory/configuration differs from its reviewed pin' }
    }
    $scope=$null
    if ($Stage -in @('SyncOnce','SyncStatus','EnableSync')) {
        Assert-OnboardingPath $Profile; Assert-OnboardingHash $ProfileSha256
        # A dedicated profile directory must not contain mutable workspace or
        # application data: guards retain its reviewed file identities.
        $guards.Add([MemoryLayer.ProtectedRelease]::Open((Split-Path -Parent $Profile)))
        $acl=Get-Acl -LiteralPath $Profile
        $trusted=@($sid,'S-1-5-18','S-1-5-32-544')
        if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted) { throw 'Unexpected private profile owner' }
        foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin $trusted) { throw 'Profile permits another reader; no synchronization started' }
        }
        if ((Get-Item -LiteralPath $Profile).Length -gt 64KB -or
            (Get-FileHash -LiteralPath $Profile).Hash.ToLowerInvariant() -cne $ProfileSha256) { throw 'Bounded private profile differs from its reviewed pin' }
        $scope=Get-OnboardingProfileScope (Get-Content -LiteralPath $Profile -Raw | ConvertFrom-Json)
    }
    $inputs=@{Stage=$Stage;Memory=$Memory;MemorySha256=$MemorySha256;ReleaseRoot=$ReleaseRoot;Config=$Config;ConfigSha256=$ConfigSha256
        Company=$Company;DeviceName=$DeviceName;Area=$Area;Owner=$Owner;Project=$Project;Revision=$Revision;Workspace=$Workspace;Journal=$Journal
        WorkspacePlanSha256=$WorkspacePlanSha256;SubscriptionOutput=$SubscriptionOutput;BootstrapSha256=$BootstrapSha256
        Profile=$Profile;ProfileSha256=$ProfileSha256;ProfileScope=$scope;CallerSid=$sid}
    $plan=Get-OnboardingPlan @inputs
    $operatorPins=[ordered]@{}
    foreach ($file in @($PSCommandPath,(Join-Path $PSScriptRoot 'onboarding-functions.ps1'),(Join-Path $PSScriptRoot 'native-instance-functions.ps1'))) {
        $operatorPins[[IO.Path]::GetFileName($file)]=(Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant()
    }
    $plan | Add-Member -NotePropertyName operator_sha256 -NotePropertyValue $operatorPins
    $digest=Get-NativeInputDigest $plan
    if (-not $Execute) {
        [ordered]@{review_sha256=$digest;plan=$plan;effects_performed=$false} | ConvertTo-Json -Depth 15
        return
    }
    if ($ApprovalSha256 -cne $digest) { throw 'Exact displayed stage digest required; no stage executed' }
    if ($Stage -cne 'EnableSync') {
        & $Memory @($plan.arguments)
        if ($LASTEXITCODE -ne 0) { throw 'CLI stage unconfirmed; retain existing CLI journals and receipts, do not assume rollback' }
        return
    }
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $arguments=Get-OnboardingSyncCommand $plan
    $action=New-ScheduledTaskAction -Execute $powershell -Argument $arguments -WorkingDirectory $ReleaseRoot
    $principal=New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
    $trigger=New-ScheduledTaskTrigger -AtLogOn -User $sid
    $settings=New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval ([TimeSpan]::FromMinutes(1)) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    $task=New-ScheduledTask -Action $action -Principal $principal -Trigger $trigger -Settings $settings -Description ('Memory Layer reviewed device sync ' + $digest)
    $existing=Get-ScheduledTask -TaskPath '\' -TaskName $plan.task.name -ErrorAction SilentlyContinue
    if ($existing) { Assert-OnboardingTask $existing $plan $powershell $arguments $digest $task }
    else {
        # First validate the exact private profile and existing local database
        # through the shared Rust implementation. This is not remote login or
        # a claim that company authorization remains available.
        & $Memory --config $Config projects sync-status --profile $Profile
        if ($LASTEXITCODE -ne 0) { throw 'Profile/database prerequisite unconfirmed; task not created' }
        # No -Force: a competing/foreign registration is an explicit conflict.
        $null=Register-ScheduledTask -TaskPath '\' -TaskName $plan.task.name -InputObject $task
    }
    $installed=Get-ScheduledTask -TaskPath '\' -TaskName $plan.task.name
    Assert-OnboardingTask $installed $plan $powershell $arguments $digest $task
    Start-ScheduledTask -TaskPath '\' -TaskName $plan.task.name
    [ordered]@{state='sync_task_registered_and_start_requested';task=$plan.task.name;review_sha256=$digest
        sync_completion_verified=$false;onboarding_acceptance_verified=$false} | ConvertTo-Json
} catch {
    # Credential-bearing profile/config contents and provider/HTTP errors never
    # become coordinator diagnostics. Existing CLI journals retain uncertainty.
    throw 'Windows onboarding stage unconfirmed; inputs and existing journals retained; private details suppressed'
} finally {
    foreach ($guard in $guards) { $guard.Dispose() }
}
