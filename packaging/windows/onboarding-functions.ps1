# SPDX-License-Identifier: AGPL-3.0-or-later
# Native onboarding orchestration only; existing CLI owns credentials and journals.
function Assert-OnboardingPath([string]$Path) {
    if ($Path.Length -gt 2048 -or $Path -cnotmatch '^[A-Za-z]:\\' -or $Path -match '[\x00-\x1f/]' -or $Path.Substring(2).Contains(':')) { throw 'Exact ordinary absolute Windows path required' }
    foreach ($part in $Path.Substring(3).Split('\')) {
        if (-not $part -or $part -in @('.', '..') -or $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Unsafe Windows onboarding path' }
    }
}
function Assert-OnboardingHash([string]$Hash) {
    if ($Hash -cnotmatch '^[0-9a-f]{64}$') { throw 'Exact lowercase SHA-256 required' }
}
function Get-OnboardingPlan {
    param([string]$Stage, [string]$Memory, [string]$MemorySha256, [string]$ReleaseRoot,
        [string]$Config, [string]$ConfigSha256, [string]$Company, [string]$DeviceName,
        [string]$Area, [guid]$Owner, [guid]$Project, [long]$Revision,
        [string]$Workspace, [string]$Journal, [string]$WorkspacePlanSha256,
        [string]$SubscriptionOutput, [string]$BootstrapSha256,
        [string]$Profile, [string]$ProfileSha256, $ProfileScope, [string]$CallerSid)
    if ($Stage -cnotin @('Login','Catalog','Prepare','Apply','SubscriptionReview','SubscriptionConfirm','SyncOnce','SyncStatus','EnableSync')) { throw 'Unsupported fixed onboarding stage' }
    foreach ($path in @($Memory, $ReleaseRoot, $Config)) { Assert-OnboardingPath $path }
    foreach ($hash in @($MemorySha256, $ConfigSha256)) { Assert-OnboardingHash $hash }
    if (-not $Memory.StartsWith($ReleaseRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
        [IO.Path]::GetExtension($Config) -ine '.toml' -or $Memory -notmatch '(?i)\\memory\.exe$') { throw 'Pinned immutable Memory release and explicit TOML configuration required' }
    $uri = $null
    if (-not [uri]::TryCreate($Company, [UriKind]::Absolute, [ref]$uri) -or $uri.Scheme -cne 'https' -or
        -not $uri.Host -or $uri.UserInfo -or $uri.Query -or $uri.Fragment -or $uri.AbsolutePath -cne '/' -or
        $Company -cne $uri.GetLeftPart([UriPartial]::Authority)) { throw 'Canonical credential-free HTTPS company origin required' }
    if ($CallerSid -notmatch '^S-1-5-21-(\d+-){3}\d+$') { throw 'Exact interactive user SID required' }
    $arguments = @('--config', $Config)
    switch ($Stage) {
        Login {
            if (-not $DeviceName -or [Text.Encoding]::UTF8.GetByteCount($DeviceName) -gt 128 -or $DeviceName -match '[\x00-\x1f\x7f]') { throw 'Bounded public device name required' }
            $arguments += @('auth','login','--company',$Company,'--name',$DeviceName)
        }
        Catalog { $arguments += @('projects','available','--company',$Company) }
        Prepare {
            if ($Area -cnotin @('workshop','homelab','general') -or $Owner -eq [guid]::Empty -or $Project -eq [guid]::Empty -or $Revision -le 0) { throw 'Exact selected offline project identity and revision required; Trust stays remote-only' }
            Assert-OnboardingPath $Workspace; Assert-OnboardingPath $Journal
            $arguments += @('projects','prepare','--company',$Company,'--area',$Area,'--owner-instance',$Owner.ToString(),'--project',$Project.ToString(),'--revision',$Revision.ToString([Globalization.CultureInfo]::InvariantCulture),'--workspace',$Workspace,'--journal',$Journal)
        }
        Apply {
            Assert-OnboardingPath $Journal; Assert-OnboardingHash $WorkspacePlanSha256
            $arguments += @('projects','apply','--company',$Company,'--journal',$Journal,'--plan-sha256',$WorkspacePlanSha256,'--confirm')
        }
        { $_ -in @('SubscriptionReview','SubscriptionConfirm') } {
            if ($Area -cnotin @('workshop','homelab','general') -or $Owner -eq [guid]::Empty -or $Project -eq [guid]::Empty -or $Revision -le 0) { throw 'Exact selected offline project identity and revision required; Trust stays remote-only' }
            Assert-OnboardingPath $SubscriptionOutput
            $arguments += @('projects','subscription','--company',$Company,'--area',$Area,'--owner-instance',$Owner.ToString(),'--project',$Project.ToString(),'--revision',$Revision.ToString([Globalization.CultureInfo]::InvariantCulture),'--output',$SubscriptionOutput,'--replication-mode','curated_project_v1')
            if ($Stage -ceq 'SubscriptionConfirm') { Assert-OnboardingHash $BootstrapSha256; $arguments += @('--bootstrap-sha256',$BootstrapSha256,'--confirm') }
        }
        default {
            Assert-OnboardingPath $Profile; Assert-OnboardingHash $ProfileSha256
            if ($Area -cnotin @('workshop','homelab','general') -or $Owner -eq [guid]::Empty -or $Project -eq [guid]::Empty) { throw 'Exact sync area/owner/project required' }
            if ($null -eq $ProfileScope -or $ProfileScope.company_origin -cne $Company -or $ProfileScope.area -cne $Area -or
                $ProfileScope.owner -cne $Owner.ToString() -or $ProfileScope.project -cne $Project.ToString()) { throw 'Pinned profile is authoritative for sync and must match the selected project' }
            foreach ($key in @('company','device','subscription','space')) {
                $id=[guid]::Empty
                if (-not [guid]::TryParse([string]$ProfileScope.$key,[ref]$id) -or $id -eq [guid]::Empty) { throw 'Complete subscription identity required' }
            }
            $verb = if ($Stage -ceq 'SyncStatus') { 'sync-status' } else { 'sync' }
            $arguments += @('projects',$verb,'--profile',$Profile)
            if ($Stage -ceq 'EnableSync') { $arguments += '--watch' }
        }
    }
    $task = $null
    if ($Stage -ceq 'EnableSync') {
        $identity = Get-NativeInputDigest ([ordered]@{sid=$CallerSid;scope=$ProfileScope;config=$Config.ToLowerInvariant()})
        $task = [ordered]@{name=('MemoryLayer-DeviceSync-' + $identity.Substring(0,24));user_sid=$CallerSid;logon='Interactive';run_level='Limited';start='logon_and_now';interval='existing_cli_watch_300_seconds'}
    }
    return [pscustomobject][ordered]@{schema_version=1;kind='memory-windows-onboarding-stage';stage=$Stage
        memory=$Memory;memory_sha256=$MemorySha256;release_root=$ReleaseRoot;config=$Config;config_sha256=$ConfigSha256
        company=$Company;area=$Area;owner=$Owner.ToString();project=$Project.ToString();profile=$Profile;profile_sha256=$ProfileSha256;profile_scope=$ProfileScope
        arguments=$arguments;task=$task;prerequisites='already_provisioned_user_config_and_local_services'
        provisions_database=$false;creates_memory_service=$false;enables_paid_models=$false;onboarding_acceptance_verified=$false}
}

function Get-OnboardingProfileScope($Profile) {
    # Do not return the database URI or source contents. --config is not a sync
    # destination override: the exact approved profile supplies its database.
    $uri=$null
    if ($Profile.schema_version -ne 1 -or $Profile.replication_mode -cne 'curated_project_v1' -or
        -not [uri]::TryCreate([string]$Profile.local_database_url,[UriKind]::Absolute,[ref]$uri) -or
        $uri.Scheme -notin @('postgres','postgresql') -or $uri.Host -notin @('127.0.0.1','[::1]','::1','localhost') -or
        $uri.Query -or $uri.Fragment -or $uri.Port -lt 1024 -or $uri.AbsolutePath -notmatch '^/[A-Za-z0-9_]{1,63}$') { throw 'Curated sync requires an already-provisioned exact loopback database profile' }
    $scope=$Profile.bootstrap.scope
    return [ordered]@{company_origin=[string]$Profile.company;company=[string]$scope.company;device=[string]$Profile.bootstrap.device
        subscription=[string]$Profile.subscription;space=[string]$scope.space;area=[string]$scope.area;owner=[string]$scope.owner;project=[string]$scope.project
        local_database_host=$uri.Host;local_database_port=$uri.Port;local_database_name=$uri.AbsolutePath.Substring(1)}
}

function ConvertTo-OnboardingLiteral([string]$Value) { return "'" + $Value.Replace("'", "''") + "'" }
function Get-OnboardingSyncCommand($Plan) {
    if ($Plan.stage -cne 'EnableSync' -or $null -eq $Plan.task) { throw 'Only a reviewed sync stage can create a task command' }
    # Fixed PowerShell program with literal values, no caller expressions. Hash
    # checks rely on the protected immutable release/user-profile boundary;
    # they are not a claim of same-handle CreateProcess execution.
    $script = '$ErrorActionPreference=''Stop'';'
    foreach ($pair in @(@($Plan.memory,$Plan.memory_sha256),@($Plan.config,$Plan.config_sha256),@($Plan.profile,$Plan.profile_sha256))) {
        $script += 'if((Get-FileHash -LiteralPath ' + (ConvertTo-OnboardingLiteral $pair[0]) + ').Hash.ToLowerInvariant() -cne ' + (ConvertTo-OnboardingLiteral $pair[1]) + '){throw ''Reviewed onboarding input changed''};'
    }
    $script += '& ' + (ConvertTo-OnboardingLiteral $Plan.memory) + ' ' + (($Plan.arguments | ForEach-Object { ConvertTo-OnboardingLiteral $_ }) -join ' ') + ';exit $LASTEXITCODE'
    return '-NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
}

function ConvertTo-OnboardingPolicyValue($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [string] -or $Value -is [ValueType]) { return [string]$Value }
    if ($Value -is [Collections.IDictionary]) {
        $result=[ordered]@{}
        foreach ($key in ($Value.Keys | Sort-Object)) { $result[$key]=ConvertTo-OnboardingPolicyValue $Value[$key] }
        return $result
    }
    if ($Value -is [Collections.IEnumerable] -and $Value -isnot [Collections.IDictionary]) {
        return ,@($Value | ForEach-Object { ConvertTo-OnboardingPolicyValue $_ })
    }
    $fields=[ordered]@{}
    $properties=if ($null -ne $Value.CimInstanceProperties) { $Value.CimInstanceProperties } else { $Value.PSObject.Properties }
    foreach ($property in ($properties | Sort-Object Name)) { $fields[$property.Name]=ConvertTo-OnboardingPolicyValue $property.Value }
    return $fields
}
function Resolve-OnboardingTriggerSid([string]$User, [string]$ExpectedSid, [scriptblock]$Resolver) {
    if ($User -ceq $ExpectedSid) { return $ExpectedSid }
    # Scheduler may serialize the SID as domain\user or a bare LOCAL SAM name.
    # Qualify bare names explicitly: never let lookup choose an ambient domain.
    if ($User.Length -gt 256 -or $User -match '^S-\d-') { throw 'Foreign sync identity' }
    if ($User -match '^[^\\/:@\x00-\x1f]{1,128}$') { $User=[Environment]::MachineName + '\' + $User }
    if ($User -notmatch '^[^\\/:@\x00-\x1f]{1,128}\\[^\\/:@\x00-\x1f]{1,128}$') { throw 'Foreign sync identity' }
    $resolved=& $Resolver $User
    if ($resolved -isnot [string] -or $resolved -cne $ExpectedSid) { throw 'Foreign sync identity' }
    return $ExpectedSid
}
function Resolve-OnboardingEnum([string]$Value, [string]$Symbolic, [string]$Numeric) {
    if ($Value -cne $Symbolic -and $Value -cne $Numeric) { throw 'Unsupported task policy enumeration' }
    return $Symbolic
}
function Get-OnboardingTaskPolicy($Task, [string]$ExpectedSid, [scriptblock]$Resolver) {
    $policy=[ordered]@{actions=(ConvertTo-OnboardingPolicyValue @($Task.Actions));principal=(ConvertTo-OnboardingPolicyValue $Task.Principal)
        triggers=(ConvertTo-OnboardingPolicyValue @($Task.Triggers));settings=(ConvertTo-OnboardingPolicyValue $Task.Settings)}
    # Only registration defaults observed on Windows are normalized. All other
    # principal, action, trigger and settings fields remain in the exact digest.
    if ($null -eq $policy.principal['Id'] -or $policy.principal['Id'] -ceq 'Author') { $policy.principal['Id']='Author' }
    $policy.principal['UserId']=Resolve-OnboardingTriggerSid $policy.principal['UserId'] $ExpectedSid $Resolver
    $policy.principal['LogonType']=Resolve-OnboardingEnum $policy.principal['LogonType'] 'Interactive' '3'
    $policy.principal['RunLevel']=Resolve-OnboardingEnum $policy.principal['RunLevel'] 'Limited' '0'
    $policy.settings['MultipleInstances']=Resolve-OnboardingEnum $policy.settings['MultipleInstances'] 'IgnoreNew' '2'
    foreach ($trigger in $policy.triggers) {
        $trigger['UserId']=Resolve-OnboardingTriggerSid $trigger['UserId'] $ExpectedSid $Resolver
        $repetition=$trigger['Repetition']
        if ($null -ne $repetition -and (($repetition.Keys | Sort-Object) -join ',') -ceq 'Duration,Interval,StopAtDurationEnd' -and
            $null -eq $repetition['Duration'] -and $null -eq $repetition['Interval'] -and $repetition['StopAtDurationEnd'] -ceq 'False') {
            $trigger['Repetition']=$null
        } elseif ($null -eq $repetition) { $trigger['Repetition']=$null }
    }
    return ConvertTo-OnboardingPolicyValue $policy
}
function Assert-OnboardingTask($Task, $Plan, [string]$PowerShell, [string]$Arguments, [string]$Digest, $ExpectedTask,
    [scriptblock]$ResolveUser = { param($User) ([Security.Principal.NTAccount]::new($User)).Translate([Security.Principal.SecurityIdentifier]).Value }) {
    if ($Task.Description -cne ('Memory Layer reviewed device sync ' + $Digest) -or $Task.TaskName -cne $Plan.task.name -or
        (Resolve-OnboardingTriggerSid $Task.Principal.UserId $Plan.task.user_sid $ResolveUser) -cne $Plan.task.user_sid -or [string]$Task.Principal.LogonType -cnotin @('Interactive','3') -or
        [string]$Task.Principal.RunLevel -cnotin @('Limited','0') -or @($Task.Actions).Count -ne 1 -or
        $Task.Actions[0].Execute -ine $PowerShell -or $Task.Actions[0].Arguments -cne $Arguments -or
        $Task.Actions[0].WorkingDirectory -ine $Plan.release_root -or @($Task.Triggers).Count -ne 1 -or
        $Task.Triggers[0].CimClass.CimClassName -cne 'MSFT_TaskLogonTrigger' -or
        (Resolve-OnboardingTriggerSid $Task.Triggers[0].UserId $Plan.task.user_sid $ResolveUser) -cne $Plan.task.user_sid -or -not $Task.Triggers[0].Enabled -or
        [string]$Task.Settings.MultipleInstances -cnotin @('IgnoreNew','2') -or -not $Task.Settings.Enabled -or
        [string]$Task.Settings.ExecutionTimeLimit -cne 'PT0S' -or $null -eq $ExpectedTask -or
        (Get-NativeInputDigest (Get-OnboardingTaskPolicy $Task $Plan.task.user_sid $ResolveUser)) -cne
        (Get-NativeInputDigest (Get-OnboardingTaskPolicy $ExpectedTask $Plan.task.user_sid $ResolveUser))) { throw 'Existing sync task differs; do not adopt or replace it' }
}
