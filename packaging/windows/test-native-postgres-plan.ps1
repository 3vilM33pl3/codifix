# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable policy tests only, not native PostgreSQL/SCM acceptance.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-postgres-functions.ps1')
$runtime=[pscustomobject]@{schema_version=1;manifest_sha256=('a'*64);input_sha256=('b'*64)
    operation_id='11111111-1111-4111-8111-111111111111';runtime_directory='C:\Program Files\Memory Layer\runtimes\pinned'
    verification_scope='protected_payload_only';database_readiness_verified=$false}
$arguments=@{Name='workshop';Port=55471;ClusterPath='C:\ProgramData\MemoryLayer\databases\workshop'
    StateRoot='C:\ProgramData\MemoryLayer';PasswordFile='C:\Private\initdb.password'
    RuntimeBinding=$runtime;OperationId=[guid]'22222222-2222-4222-8222-222222222222';RecoveryReference='synthetic-backup-receipt'}
$plan=Get-NativePostgresPlan @arguments
if ($plan.host -cne '127.0.0.1' -or $plan.service_account -cne 'NT SERVICE\MemoryLayerPostgres-workshop' -or
    $plan.provisioning_implemented -or $plan.native_execution_verified -or $plan.mutates_existing_database) { throw 'Invalid provisioning scope' }
if ($plan.initdb_arguments -notcontains '--pwfile' -or $plan.initdb_arguments -contains '--auth=trust' -or
    $plan.pg_hba_conf.Contains(' trust') -or $plan.postgresql_conf.Contains("'*'")) { throw 'Unsafe initialization policy' }
$original=Get-NativeInputDigest $plan
foreach ($change in @(@{Name='../outside'},@{Port=543},@{Port=65536},@{ClusterPath='D:\Live'},
    @{ClusterPath='C:\ProgramData\MemoryLayer\databases\workshop\..\homelab'},
    @{PasswordFile='C:\ProgramData\MemoryLayer\databases\workshop\password'},
    @{PasswordFile='C:\Private\password:alternate'},@{OperationId=[guid]::Empty},@{RecoveryReference=''})) {
    $copy=$arguments.Clone(); foreach ($key in $change.Keys) { $copy[$key]=$change[$key] }
    $rejected=$false; try { $null=Get-NativePostgresPlan @copy } catch { $rejected=$true }
    if (-not $rejected) { throw 'Invalid provisioning input accepted' }
}
$changed=$arguments.Clone(); $changed.Port=55472
if ((Get-NativeInputDigest (Get-NativePostgresPlan @changed)) -ceq $original) { throw 'Port was not bound to review digest' }
foreach ($name in @('native-postgres-functions.ps1','native-postgres-preview.ps1')) {
    $tokens=$null; $errors=$null
    $null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
Write-Output 'Native PostgreSQL provisioning plan/loopback/identity/password-path tests passed (portable only; execution unavailable)'
