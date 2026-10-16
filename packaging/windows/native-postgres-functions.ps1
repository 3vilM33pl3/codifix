# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure provisioning contract. Execution/recovery is deliberately separate.
function Get-NativePostgresPlan {
    param([string]$Name, [int]$Port, [string]$ClusterPath, [string]$StateRoot,
        [string]$PasswordFile, $RuntimeBinding, [guid]$OperationId,
        [string]$RecoveryReference)
    if ($Name -cnotmatch '^[a-z][a-z0-9-]{0,47}$' -or $Port -lt 1024 -or $Port -gt 65535 -or
        $OperationId -eq [guid]::Empty -or [string]::IsNullOrWhiteSpace($RecoveryReference) -or
        $RecoveryReference.Length -gt 1024) { throw 'Exact cluster name, unprivileged port, operation and recovery reference required' }
    foreach ($path in @($StateRoot,$ClusterPath,$PasswordFile,$RuntimeBinding.runtime_directory)) {
        if ($path -cnotmatch '^[A-Za-z]:\\' -or $path.Contains('/') -or $path.EndsWith('\')) { throw 'Canonical absolute local Windows paths required' }
        Assert-NativeArchiveName $path.Substring(3).Replace('\','/')
    }
    $expected = $StateRoot + '\databases\' + $Name
    if (-not [string]::Equals($ClusterPath,$expected,[StringComparison]::OrdinalIgnoreCase)) { throw 'Cluster must be the selected Memory-owned named database directory' }
    if ($PasswordFile.StartsWith($ClusterPath + '\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Initialization password must remain outside database state' }
    if ($PasswordFile.StartsWith($StateRoot + '\postgres-operations\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Initialization password input must remain outside managed temporary operation files' }
    if ($RuntimeBinding.schema_version -ne 1 -or $RuntimeBinding.manifest_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $RuntimeBinding.input_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $RuntimeBinding.verification_scope -cne 'protected_payload_only' -or
        $RuntimeBinding.database_readiness_verified -ne $false -or
        [guid]$RuntimeBinding.operation_id -eq [guid]::Empty) { throw 'Verified exact native runtime binding required' }
    $service = 'MemoryLayerPostgres-' + $Name
    $bin = $RuntimeBinding.runtime_directory + '\pgsql\bin'
    return [pscustomobject][ordered]@{
        schema_version=1; kind='memory-native-postgres-provisioning'; name=$Name
        operation_id=$OperationId.ToString(); recovery_reference=$RecoveryReference
        cluster_directory=$expected; service_name=$service; service_account=('NT SERVICE\' + $service)
        runtime=$RuntimeBinding; host='127.0.0.1'; port=$Port; superuser='memory_admin'
        initdb_executable=($bin + '\initdb.exe')
        initdb_arguments=@('--pgdata',$expected,'--username=memory_admin','--encoding=UTF8',
            '--locale=C','--no-clean','--auth-host=scram-sha-256','--auth-local=scram-sha-256','--pwfile',$PasswordFile)
        service_executable=($bin + '\pg_ctl.exe')
        service_arguments=@('runservice','-N',$service,'-D',$expected,'-w')
        postgresql_conf="listen_addresses = '127.0.0.1'`nport = $Port`npassword_encryption = 'scram-sha-256'`n"
        pg_hba_conf="host all all 127.0.0.1/32 scram-sha-256`nhost all all ::1/128 reject`n"
        required_extensions=@('vector','pgcrypto','pg_trgm')
        mutates_existing_database=$false; automatic_database_upgrade=$false
        provisioning_implemented=$true; native_execution_verified=$false
    }
}

function Close-NativePostgresReview($Review) {
    if ($Review) {
        if ($Review.password_bytes) { [Array]::Clear($Review.password_bytes,0,$Review.password_bytes.Length) }
        if ($Review.file) { $Review.file.Dispose() }
        if ($Review.guard) { $Review.guard.Dispose() }
        Close-NativePrerequisiteLease $Review.runtime
    }
}

function Open-NativePostgresReview {
    param([string]$Name,[int]$Port,[string]$ClusterPath,[string]$PasswordFile,[guid]$OperationId,
        [string]$RuntimeManifestPath,[string]$RuntimeManifestSha256,[guid]$RuntimeOperationId,[string]$RecoveryReference)
    $review=[pscustomobject]@{runtime=$null;guard=$null;file=$null;password_bytes=$null;plan=$null;input_sha256=$null}
    try {
        $review.runtime=Open-NativePrerequisiteLease $RuntimeManifestPath $RuntimeManifestSha256 $RuntimeOperationId
        $root=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'MemoryLayer'
        $review.plan=Get-NativePostgresPlan $Name $Port $ClusterPath $root $PasswordFile $review.runtime.binding $OperationId $RecoveryReference
        $review.guard=[MemoryLayer.ProtectedRelease]::Open([IO.Path]::GetDirectoryName($PasswordFile))
        $review.file=[IO.File]::Open($PasswordFile,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        if ($review.file.Length -lt 32 -or $review.file.Length -gt 4096) { throw 'Initialization password must be 32-4096 UTF-8 bytes' }
        $allowed=@('S-1-5-18','S-1-5-32-544',[Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
        $acl=Get-Acl -LiteralPath $PasswordFile
        foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -eq 'Allow' -and $rule.FileSystemRights -ne 0 -and $rule.IdentityReference.Value -notin $allowed) { throw 'Initialization password file is accessible to another principal' }
        }
        $review.password_bytes=[byte[]]::new([int]$review.file.Length); $offset=0
        while ($offset -lt $review.password_bytes.Length) {
            $count=$review.file.Read($review.password_bytes,$offset,$review.password_bytes.Length-$offset)
            if ($count -eq 0) { throw 'Initialization password changed during review' }; $offset+=$count
        }
        $text=[Text.UTF8Encoding]::new($false,$true).GetString($review.password_bytes)
        if ($text.Length -lt 32 -or $text[0] -eq [char]0xfeff -or $text.Contains("`r") -or $text.Contains("`n") -or $text.Contains([char]0)) { throw 'Initialization password must be one UTF-8 line without BOM/terminator' }
        $text=$null
        $sha=[Security.Cryptography.SHA256]::Create()
        try { $fingerprint=([BitConverter]::ToString($sha.ComputeHash($review.password_bytes))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        $review.input_sha256=Get-NativeInputDigest ([ordered]@{plan=$review.plan;password_fingerprint=$fingerprint})
        return $review
    } catch { Close-NativePostgresReview $review; throw }
}

function Test-NativePostgresControl([string]$Version,[string]$ControlOutput,[string]$ExpectedIdentity='',[switch]$AllowRunning) {
    if ($Version.Trim() -cne '17') { throw 'Cluster is not PostgreSQL 17; retained without upgrade' }
    $match=[regex]::Match($ControlOutput,'(?m)^Database system identifier:\s+(\d+)\s*$')
    $state=if ($AllowRunning) { '(?:shut down|in production)' } else { 'shut down' }
    if (-not [regex]::IsMatch($ControlOutput,'(?m)^Database cluster state:\s+'+$state+'\s*$')) { throw 'Initialized cluster is not in the expected state; retain it for inspection' }
    if (-not $match.Success -or ($ExpectedIdentity -and $ExpectedIdentity -cne $match.Groups[1].Value)) { throw 'Cluster control identity is missing or changed; retained for inspection' }
    return $match.Groups[1].Value
}

function Assert-NativePostgresService($Service,$Plan,[string]$BinaryPath) {
    if (-not $Service -or $Service.Name -cne $Plan.service_name -or -not [string]::Equals($Service.StartName,$Plan.service_account,[StringComparison]::OrdinalIgnoreCase) -or
        $Service.PathName -cne $BinaryPath -or $Service.StartMode -cne 'Manual') { throw 'Selected PostgreSQL service differs from exact operation identity; no service modified' }
}

function Assert-NativePostgresHealth([string]$Output,[int]$Port,[string]$ClusterIdentity,[switch]$WithoutExtensions) {
    $expected='170011|127.0.0.1|'+$Port+'|'+$ClusterIdentity
    if (-not $WithoutExtensions) { $expected+='|0.8.7' }
    if ($ClusterIdentity -cnotmatch '^\d+$' -or $Output.Trim() -cne $expected) {
        throw 'Authenticated database readiness differs from reviewed cluster/version/listener/extensions'
    }
}

function Get-NativePostgresAclRecoveryAction([string]$Current,[string]$Before,[string]$Granted) {
    if (-not $Current -or -not $Before -or -not $Granted) { throw 'Incomplete runtime ACL recovery state' }
    if ($Current -ceq $Before) { return 'unchanged' }
    if ($Current -ceq $Granted) { return 'restore' }
    throw 'Runtime ACL changed outside initialization; unknown access preserved for inspection'
}

function Get-NativePostgresConfigAction([string]$Current,[string]$Before,[string]$Desired) {
    foreach ($hash in @($Current,$Before,$Desired)) { if ($hash -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid configuration receipt digest' } }
    if ($Current -ceq $Desired) { return 'unchanged' }
    if ($Current -ceq $Before) { return 'write' }
    throw 'Cluster configuration changed outside this operation; no overwrite'
}

function Get-NativePostgresInitAction([bool]$HasVersion,[bool]$HasControl,[int]$Entries,[string]$Phase) {
    if ($HasVersion -and $HasControl -and $Phase -ceq 'initializing') { return 'recover' }
    if (-not $HasVersion -and -not $HasControl -and $Entries -eq 0 -and $Phase -in @('cluster_created','initializing')) { return 'initialize' }
    throw 'Partial or unknown initdb state retained; never initialize over existing contents'
}
