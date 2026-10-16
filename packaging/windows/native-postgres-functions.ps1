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
            '--auth-host=scram-sha-256','--auth-local=scram-sha-256','--pwfile',$PasswordFile)
        service_executable=($bin + '\pg_ctl.exe')
        service_arguments=@('runservice','-N',$service,'-D',$expected,'-w')
        postgresql_conf="listen_addresses = '127.0.0.1'`nport = $Port`npassword_encryption = 'scram-sha-256'`n"
        pg_hba_conf="host all all 127.0.0.1/32 scram-sha-256`nhost all all ::1/128 reject`n"
        required_extensions=@('vector','pgcrypto','pg_trgm')
        mutates_existing_database=$false; automatic_database_upgrade=$false
        provisioning_implemented=$false; native_execution_verified=$false
    }
}
