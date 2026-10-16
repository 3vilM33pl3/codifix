# SPDX-License-Identifier: AGPL-3.0-or-later
# Fresh-cluster provisioning only. No adoption, upgrade, rollback or data removal.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Preview','Provision')][string]$Action,
    [Parameter(Mandatory=$true)][string]$Name,
    [Parameter(Mandatory=$true)][ValidateRange(1024,65535)][int]$Port,
    [Parameter(Mandatory=$true)][string]$ClusterPath,
    [Parameter(Mandatory=$true)][string]$PasswordFile,
    [Parameter(Mandatory=$true)][guid]$OperationId,
    [Parameter(Mandatory=$true)][string]$RuntimeManifestPath,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$RuntimeManifestSha256,
    [Parameter(Mandatory=$true)][guid]$RuntimeOperationId,
    [Parameter(Mandatory=$true)][ValidateLength(1,1024)][string]$RecoveryReference,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedInputSha256
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
. (Join-Path $PSScriptRoot 'native-postgres-functions.ps1')
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows required; no WSL/container fallback' }
$windowsRoot=[Environment]::GetFolderPath('Windows')
$system32=Join-Path $windowsRoot 'System32'

function Save-PgPhase([string]$Phase) {
    if (@($journal.history).Count -ge 1000) { throw 'Provisioning history limit reached; inspect retained state' }
    $journal.phase=$Phase
    $journal.history=@($journal.history)+@([ordered]@{phase=$Phase;observed_at=[DateTime]::UtcNow.ToString('o')})
    Write-NativeJournal $journalPath $journal
}
function Selected-Service { Get-CimInstance Win32_Service -Filter "Name='$($plan.service_name)'" }
function Grant-PgAccess([string]$Path,[string]$Sid,[string]$Rights,[switch]$Inherit) {
    $item=Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse provisioning path rejected' }
    $acl=Get-Acl -LiteralPath $Path
    if ($Inherit) { $rule=[Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($Sid),$Rights,'ContainerInherit,ObjectInherit','None','Allow') }
    else { $rule=[Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($Sid),$Rights,'Allow') }
    $acl.AddAccessRule($rule); Set-Acl -LiteralPath $Path -AclObject $acl
}
function Runtime-Paths {
    $runtimeRoot=Split-Path -Parent $plan.runtime.runtime_directory
    return @((Split-Path -Parent $runtimeRoot),$runtimeRoot,$plan.runtime.runtime_directory)+
        @(Get-ChildItem -LiteralPath $plan.runtime.runtime_directory -Recurse -Force | ForEach-Object FullName)
}
function Restore-PgRuntimeAccess {
    if (-not (Test-Path -LiteralPath $runtimeAccessPath)) { return }
    if ((Get-Item -LiteralPath $runtimeAccessPath).Length -gt 32MB) { throw 'Runtime ACL recovery receipt exceeds bound' }
    $record=Get-Content -LiteralPath $runtimeAccessPath -Raw | ConvertFrom-Json
    if ($record.kind -cne 'memory-native-pg-runtime-access-v1' -or $record.operation_id -cne $OperationId.ToString() -or
        $record.input_sha256 -cne $review.input_sha256 -or $record.phase -notin @('pending','restored')) { throw 'Foreign runtime ACL recovery receipt' }
    if ($record.phase -ceq 'restored') { return }
    $expected=@(Runtime-Paths | Sort-Object)
    $actual=@($record.entries | ForEach-Object path | Sort-Object)
    if ($actual.Count -ne $expected.Count -or @(Compare-Object $expected $actual -CaseSensitive).Count) { throw 'Runtime ACL recovery paths differ from pinned payload' }
    foreach ($entry in $record.entries) {
        $acl=Get-Acl -LiteralPath $entry.path
        $current=$acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
        if ((Get-NativePostgresAclRecoveryAction $current $entry.before $entry.granted) -ceq 'unchanged') { continue }
        $acl.SetSecurityDescriptorSddlForm($entry.before,[Security.AccessControl.AccessControlSections]::Access)
        Set-Acl -LiteralPath $entry.path -AclObject $acl
    }
    $record.phase='restored'; Write-NativeJournal $runtimeAccessPath $record
}
function Grant-PgRuntimeOperatorRead([string]$Sid) {
    Restore-PgRuntimeAccess
    $entries=@(foreach ($path in (Runtime-Paths)) {
        $acl=Get-Acl -LiteralPath $path
        $before=$acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($Sid),'ReadAndExecute','Allow'))
        [pscustomobject]@{path=$path;before=$before;granted=$acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access)}
    })
    $record=[ordered]@{kind='memory-native-pg-runtime-access-v1';operation_id=$OperationId.ToString();input_sha256=$review.input_sha256;phase='pending';entries=$entries}
    Write-NativeJournal $runtimeAccessPath $record
    foreach ($entry in $entries) {
        $acl=Get-Acl -LiteralPath $entry.path
        if ($acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Access) -cne $entry.before) { throw 'Runtime ACL changed before temporary grant; inspect retained intent' }
        $acl.SetSecurityDescriptorSddlForm($entry.granted,[Security.AccessControl.AccessControlSections]::Access)
        Set-Acl -LiteralPath $entry.path -AclObject $acl
    }
}
function Write-PgBytes([string]$Path,[byte[]]$Bytes) {
    $partial=Join-Path (Split-Path -Parent $Path) ([guid]::NewGuid().ToString()+'.partial')
    $file=[IO.File]::Open($partial,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $file.Write($Bytes,0,$Bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    if ([IO.File]::Exists($Path)) { [IO.File]::Replace($partial,$Path,[NullString]::Value) }
    else { [IO.File]::Move($partial,$Path) }
}
function Invoke-PgTool([string]$Executable,[string[]]$Arguments,[string]$Label,[string]$PassFile='') {
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=$Executable; $start.Arguments=($Arguments | ForEach-Object { ConvertTo-NativeCommandArgument $_ }) -join ' '
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
    $start.WorkingDirectory=$operationRoot
    $start.EnvironmentVariables.Clear()
    $start.EnvironmentVariables['SystemRoot']=$windowsRoot
    $start.EnvironmentVariables['PATH']=(Split-Path -Parent $Executable)+';'+$system32
    $start.EnvironmentVariables['TEMP']=$toolTemp; $start.EnvironmentVariables['TMP']=$toolTemp
    $start.EnvironmentVariables['LC_ALL']='C'; $start.EnvironmentVariables['PGCONNECT_TIMEOUT']='5'
    if ($PassFile) { $start.EnvironmentVariables['PGPASSFILE']=$PassFile }
    $process=[Diagnostics.Process]::new(); $process.StartInfo=$start
    try {
        if (-not $process.Start()) { throw 'Native database process failed to start' }
        $out=$process.StandardOutput.ReadToEndAsync(); $err=$process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(120000)) {
            # Exact owned process tree, not another service/database process.
            & (Join-Path $system32 'taskkill.exe') /PID $process.Id /T /F | Out-Null
            throw 'Database command timed out; effects may be incomplete, inspect the journal'
        }
        $process.WaitForExit(); $stdout=$out.Result; $stderr=$err.Result
        if ($stdout.Length+$stderr.Length -gt 4MB) { throw 'Native database output exceeds bound' }
        $log=Join-Path $operationRoot ($Label+'-'+[guid]::NewGuid().ToString()+'.log')
        Write-PgBytes $log ([Text.UTF8Encoding]::new($false).GetBytes($stdout+"`n"+$stderr))
        if ($process.ExitCode -ne 0) { throw "Native database command $Label failed; inspect its private operation log" }
        return $stdout
    } finally { $process.Dispose() }
}
function Protect-PgCluster {
    Set-NativePrivateDirectory $plan.cluster_directory $serviceSid
    foreach ($item in (Get-ChildItem -LiteralPath $plan.cluster_directory -Recurse -Force)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse point in fresh cluster; retained for inspection' }
        if ($item.PSIsContainer) { Set-NativePrivateDirectory $item.FullName $serviceSid }
        else {
            $acl=[Security.AccessControl.FileSecurity]::new(); $acl.SetAccessRuleProtection($true,$false)
            $acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
            foreach ($sid in @('S-1-5-18','S-1-5-32-544')) { $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid),'FullControl','Allow')) }
            $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($serviceSid),'Modify','Allow'))
            Set-Acl -LiteralPath $item.FullName -AclObject $acl
        }
    }
}

$review=$null; $lock=$null; $initPassword=$null; $passFile=$null; $journal=$null; $journalOwned=$false; $initPasswordOwned=$false; $runtimeAccessPath=$null
try {
    $review=Open-NativePostgresReview $Name $Port $ClusterPath $PasswordFile $OperationId $RuntimeManifestPath $RuntimeManifestSha256 $RuntimeOperationId $RecoveryReference
    $plan=$review.plan
    $root=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'MemoryLayer'
    $journals=Join-Path $root 'postgres-operations'; $operationRoot=Join-Path $journals $OperationId.ToString()
    $journalPath=Join-Path $operationRoot 'journal.json'
    $runtimeAccessPath=Join-Path $operationRoot 'runtime-access.json'
    $initPassword=Join-Path $operationRoot 'initdb.password.private'
    $binaryPath=(@($plan.service_executable)+@($plan.service_arguments) | ForEach-Object { ConvertTo-NativeCommandArgument $_ }) -join ' '
    if ($Action -eq 'Preview') {
        if (Test-Path -LiteralPath $plan.cluster_directory) { throw 'Fresh provisioning target already exists; resume using its original operation instead' }
        if (Selected-Service) { throw 'Selected service already exists; no adoption is implemented' }
        [pscustomobject]@{input_sha256=$review.input_sha256;plan=$plan;starts_services=$false;mutates_database=$false;execution_available=$true} | ConvertTo-Json -Depth 16
        return
    }
    if ($ExpectedInputSha256 -cne $review.input_sha256) { throw 'Provision inputs differ from the reviewed preview digest' }
    $principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Elevated provisioning operator required' }
    Set-NativePrivateDirectory $root -SharedAncestor
    Set-NativePrivateDirectory $journals
    # Share the named-instance operator lock: exact ACL restoration must not
    # erase a concurrent installer's legitimate service-reader grant.
    $lock=[IO.File]::Open((Join-Path $root 'operator.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    if (Test-Path -LiteralPath $journalPath) {
        $receiptGuard=[MemoryLayer.ProtectedRelease]::Open($operationRoot)
        try {
            if ((Get-Item -LiteralPath $journalPath).Length -gt 1MB) { throw 'Provisioning receipt exceeds bound' }
            $journal=Get-Content -LiteralPath $journalPath -Raw | ConvertFrom-Json
            Assert-NativeRetry $journal $review.input_sha256
            if ($journal.schema_version -ne 1 -or $journal.operation_id -cne $OperationId.ToString() -or $journal.service_name -cne $plan.service_name -or $journal.cluster_directory -cne $plan.cluster_directory) { throw 'Foreign provisioning receipt' }
            $journalOwned=$true
        } finally { $receiptGuard.Dispose() }
    } else {
        if ((Test-Path -LiteralPath $plan.cluster_directory) -or (Selected-Service)) { throw 'Existing service/cluster has no matching operation; retained unchanged' }
        Set-NativePrivateDirectory $operationRoot
        $journal=[ordered]@{schema_version=1;operation_id=$OperationId.ToString();input_sha256=$review.input_sha256
            service_name=$plan.service_name;cluster_directory=$plan.cluster_directory;phase='prepared';history=@()
            cluster_created=$false;cluster_identity='';configuration=$null;native_execution_verified=$false
            last_failure=$null;verified_at=$null;runtime=$plan.runtime}
        $journalOwned=$true
        Save-PgPhase 'prepared'
    }
    Restore-PgRuntimeAccess
    if (Test-Path -LiteralPath $initPassword) {
        if ((Get-Item -LiteralPath $initPassword).Length -gt 4096 -or
            (Get-NativeInputDigest ([Convert]::ToBase64String([IO.File]::ReadAllBytes($initPassword)))) -cne
            (Get-NativeInputDigest ([Convert]::ToBase64String($review.password_bytes)))) { throw 'Interrupted initialization password differs; retained for inspection' }
        $initPasswordOwned=$true
    }
    if ($journal.phase -ceq 'completed') {
        Assert-NativePostgresService (Selected-Service) $plan $binaryPath
        [pscustomobject]@{phase='completed';receipt=$journalPath;historical_verified_at=$journal.verified_at;fresh_health_verified=$false;service_restarted=$false} | ConvertTo-Json
        return
    }
    $toolTemp=Join-Path $operationRoot 'tool-temp'
    Set-NativePrivateDirectory $toolTemp
    Grant-PgAccess $toolTemp ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value) 'Modify' -Inherit
    $existing=Selected-Service
    if (-not $existing) {
        if ($journal.cluster_created) { throw 'Previously created service disappeared; explicit recovery required' }
        Save-PgPhase 'service_creating'
        $null=Invoke-PgTool (Join-Path $system32 'sc.exe') @('create',$plan.service_name,'binPath=',$binaryPath,'start=','demand','obj=',$plan.service_account) 'service-create'
        $existing=Selected-Service
    } elseif ($journal.phase -ceq 'prepared') { throw 'A service appeared outside the prepared operation; retained unchanged' }
    Assert-NativePostgresService $existing $plan $binaryPath
    if ($existing.State -notin @('Stopped','Running')) { throw 'Service is changing state; inspect before retry' }
    if (-not $journal.cluster_identity -and $existing.State -ne 'Stopped') { throw 'Uninitialized selected service is unexpectedly running' }
    $null=Invoke-PgTool (Join-Path $system32 'sc.exe') @('sidtype',$plan.service_name,'unrestricted') 'service-sid'
    $serviceSid=([Security.Principal.NTAccount]::new($plan.service_account)).Translate([Security.Principal.SecurityIdentifier]).Value
    if ($serviceSid -notmatch '^S-1-5-80-(\d+-){4}\d+$') { throw 'SCM did not resolve the exact virtual service identity' }
    foreach ($parent in @($root,(Join-Path $root 'databases'))) { Set-NativePrivateDirectory $parent $serviceSid -SharedAncestor }
    $runtimeRoot=Split-Path -Parent $plan.runtime.runtime_directory
    Set-NativePrivateDirectory (Split-Path -Parent $runtimeRoot) $serviceSid -SharedAncestor
    Set-NativePrivateDirectory $runtimeRoot $serviceSid -SharedAncestor
    foreach ($item in @((Get-Item -LiteralPath $plan.runtime.runtime_directory)) + @(Get-ChildItem -LiteralPath $plan.runtime.runtime_directory -Recurse -Force)) {
        Grant-PgAccess $item.FullName $serviceSid 'ReadAndExecute'
    }
    if (-not $journal.cluster_created) {
        if (Test-Path -LiteralPath $plan.cluster_directory) {
            if ($journal.phase -cne 'cluster_creating' -or @(Get-ChildItem -LiteralPath $plan.cluster_directory -Force).Count) { throw 'Unknown cluster contents retained; no initialization attempted' }
        }
        Save-PgPhase 'cluster_creating'
        Set-NativePrivateDirectory $plan.cluster_directory $serviceSid
        $journal.cluster_created=$true; Save-PgPhase 'cluster_created'
    }
    $bin=Split-Path -Parent $plan.initdb_executable
    if ($journal.cluster_identity) {
        $controlText=Invoke-PgTool (Join-Path $bin 'pg_controldata.exe') @('-D',$plan.cluster_directory) 'resume-control'
        $null=Test-NativePostgresControl (Get-Content -LiteralPath (Join-Path $plan.cluster_directory 'PG_VERSION') -Raw) $controlText $journal.cluster_identity -AllowRunning:((Selected-Service).State -eq 'Running')
    }
    if (-not $journal.cluster_identity) {
        $version=Join-Path $plan.cluster_directory 'PG_VERSION'; $control=Join-Path $plan.cluster_directory 'global\pg_control'
        $initialization=Get-NativePostgresInitAction (Test-Path -LiteralPath $version) (Test-Path -LiteralPath $control) @(Get-ChildItem -LiteralPath $plan.cluster_directory -Force).Count $journal.phase
        if ($initialization -ceq 'initialize') {
            $operatorSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
            Grant-PgAccess $plan.cluster_directory $operatorSid 'Modify' -Inherit
            Grant-PgAccess $operationRoot $operatorSid 'ReadAndExecute'
            if (-not $initPasswordOwned) { Write-PgBytes $initPassword $review.password_bytes; $initPasswordOwned=$true }
            Grant-PgAccess $initPassword $operatorSid 'Read'
            $arguments=@($plan.initdb_arguments)
            $arguments[$arguments.Length-1]=$initPassword
            Save-PgPhase 'initializing'
            # initdb deliberately restricts its Windows token. Give that exact
            # operator only RX, with durable exact prior-ACL restoration. Never
            # disable PostgreSQL's restricted-token safety mechanism.
            try {
                Grant-PgRuntimeOperatorRead $operatorSid
                $null=Invoke-PgTool $plan.initdb_executable $arguments 'initdb'
            } finally { Restore-PgRuntimeAccess }
        }
        $controlText=Invoke-PgTool (Join-Path $bin 'pg_controldata.exe') @('-D',$plan.cluster_directory) 'control'
        $journal.cluster_identity=Test-NativePostgresControl (Get-Content -LiteralPath $version -Raw) $controlText
        Save-PgPhase 'initialized'
    }
    if (-not $journal.configuration) {
        $journal.configuration=@(
            [pscustomobject]@{name='postgresql.auto.conf';before=(Get-FileHash -LiteralPath (Join-Path $plan.cluster_directory 'postgresql.auto.conf')).Hash.ToLowerInvariant();text=$plan.postgresql_conf},
            [pscustomobject]@{name='pg_hba.conf';before=(Get-FileHash -LiteralPath (Join-Path $plan.cluster_directory 'pg_hba.conf')).Hash.ToLowerInvariant();text=$plan.pg_hba_conf})
        Save-PgPhase 'configuring'
    }
    if ((@($journal.configuration | ForEach-Object name | Sort-Object) -join ',') -cne 'pg_hba.conf,postgresql.auto.conf') { throw 'Configuration receipt must contain the exact two managed policies' }
    foreach ($config in $journal.configuration) {
        if ($config.name -notin @('postgresql.auto.conf','pg_hba.conf')) { throw 'Unexpected configuration receipt' }
        $expectedText=if ($config.name -ceq 'postgresql.auto.conf') { $plan.postgresql_conf } else { $plan.pg_hba_conf }
        if ($config.text -cne $expectedText) { throw 'Configuration receipt differs from reviewed policy' }
        $path=Join-Path $plan.cluster_directory $config.name; $bytes=[Text.UTF8Encoding]::new($false).GetBytes($config.text)
        $sha=[Security.Cryptography.SHA256]::Create()
        try { $desired=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        $actual=(Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant()
        if ((Get-NativePostgresConfigAction $actual $config.before $desired) -ceq 'write') {
            Write-PgBytes $path $bytes
        }
    }
    if ((Selected-Service).State -eq 'Stopped') { Protect-PgCluster }
    Save-PgPhase 'starting'
    Assert-NativePostgresService (Selected-Service) $plan $binaryPath
    if ((Selected-Service).State -eq 'Stopped') { Start-Service -Name $plan.service_name }
    (Get-Service -Name $plan.service_name).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))
    Assert-NativePostgresService (Selected-Service) $plan $binaryPath
    $passFile=Join-Path $operationRoot ('pgpass-'+[guid]::NewGuid().ToString()+'.private')
    $secret=[Text.UTF8Encoding]::new($false,$true).GetString($review.password_bytes).Replace('\','\\').Replace(':','\:')
    $passBytes=[Text.UTF8Encoding]::new($false).GetBytes('127.0.0.1:'+$Port+':postgres:memory_admin:'+$secret+"`n")
    try { Write-PgBytes $passFile $passBytes } finally { [Array]::Clear($passBytes,0,$passBytes.Length); $secret=$null }
    $connection=@('--no-password','-h','127.0.0.1','-p',$Port.ToString(),'-U','memory_admin','-d','postgres','-v','ON_ERROR_STOP=1','-X','-A','-t')
    # A busy loopback port is not proof that this is our cluster. Verify the
    # authenticated server identity before executing even idempotent DDL.
    $identitySql="SELECT current_setting('server_version_num'),current_setting('listen_addresses'),current_setting('port'),system_identifier FROM pg_control_system();"
    $server=Invoke-PgTool (Join-Path $bin 'psql.exe') ($connection+@('-c',$identitySql)) 'server-identity' $passFile
    Assert-NativePostgresHealth $server $Port $journal.cluster_identity -WithoutExtensions
    $null=Invoke-PgTool (Join-Path $bin 'psql.exe') ($connection+@('-c','CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS pg_trgm;')) 'extensions' $passFile
    $sql="SELECT current_setting('server_version_num'),current_setting('listen_addresses'),current_setting('port'),system_identifier,(SELECT extversion FROM pg_extension WHERE extname='vector') FROM pg_control_system();"
    $health=(Invoke-PgTool (Join-Path $bin 'psql.exe') ($connection+@('-c',$sql)) 'health' $passFile).Trim()
    Assert-NativePostgresHealth $health $Port $journal.cluster_identity
    $journal.native_execution_verified=$true; $journal.verified_at=[DateTime]::UtcNow.ToString('o'); $journal.last_failure=$null
    Save-PgPhase 'completed'
    [pscustomobject]@{phase='completed';receipt=$journalPath;cluster_directory=$plan.cluster_directory;service_name=$plan.service_name;cluster_identity=$journal.cluster_identity;verified_at=$journal.verified_at;native_execution_verified=$true;memory_application_database_provisioned=$false} | ConvertTo-Json
} catch {
    if ($journalOwned -and $Action -eq 'Provision') {
        $journal.last_failure=[ordered]@{code='provisioning_incomplete';phase=$journal.phase;observed_at=[DateTime]::UtcNow.ToString('o');external_effects_retained=$true}
        try { Write-NativeJournal $journalPath $journal } catch { throw 'Journal update failed; external effects may exist. Inspect the selected operation; do not assume rollback' }
    }
    throw
} finally {
    try { if ($journalOwned -and $runtimeAccessPath) { Restore-PgRuntimeAccess } }
    finally {
        try {
            if ($journalOwned) {
                $cleanup=@($passFile); if ($initPasswordOwned) { $cleanup+=@($initPassword) }
                foreach ($temporary in $cleanup) { if ($temporary -and (Test-Path -LiteralPath $temporary)) { Remove-Item -LiteralPath $temporary -Force } }
            }
        } finally { if ($lock) { $lock.Dispose() }; Close-NativePostgresReview $review }
    }
}
