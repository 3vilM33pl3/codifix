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
    -not $plan.provisioning_implemented -or $plan.native_execution_verified -or $plan.mutates_existing_database) { throw 'Invalid provisioning scope' }
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
$identity=Test-NativePostgresControl "17`n" "Database system identifier: 123456789`nDatabase cluster state: shut down`n" '123456789'
if ($identity -cne '123456789') { throw 'Cluster control identity not preserved' }
foreach ($input in @(@('16','Database system identifier: 123',''),@('17','malformed',''),@('17',"Database system identifier: 123`nDatabase cluster state: shut down",'456'),
    @('17',"Database system identifier: 123`nDatabase cluster state: in production",''))) {
    $rejected=$false; try { $null=Test-NativePostgresControl $input[0] $input[1] $input[2] } catch { $rejected=$true }
    if (-not $rejected) { throw 'Invalid control metadata accepted' }
}
$service=[pscustomobject]@{Name=$plan.service_name;StartName=$plan.service_account;PathName='exact command';StartMode='Manual'}
Assert-NativePostgresService $service $plan 'exact command'
$sameAccount=$service | ConvertTo-Json | ConvertFrom-Json
$sameAccount.StartName=$sameAccount.StartName.ToLowerInvariant()
Assert-NativePostgresService $sameAccount $plan 'exact command'
foreach ($change in @(@{Name='OtherService'},@{StartName='LocalSystem'},@{PathName='other.exe'},@{StartMode='Auto'})) {
    $copy=$service | ConvertTo-Json | ConvertFrom-Json
    foreach ($key in $change.Keys) { $copy.$key=$change[$key] }
    $rejected=$false; try { Assert-NativePostgresService $copy $plan 'exact command' } catch { $rejected=$true }
    if (-not $rejected) { throw 'Changed service identity accepted' }
}
Assert-NativePostgresHealth '170011|127.0.0.1|55471|123|0.8.7' 55471 '123'
Assert-NativePostgresHealth '170011|127.0.0.1|55471|123' 55471 '123' -WithoutExtensions
$null=Test-NativePostgresControl '17' "Database system identifier: 123`nDatabase cluster state: in production" '123' -AllowRunning
foreach ($bad in @('170010|127.0.0.1|55471|123|0.8.7','170011|*|55471|123|0.8.7',
    '170011|127.0.0.1|55472|123|0.8.7','170011|127.0.0.1|55471|456|0.8.7','170011|127.0.0.1|55471|123|0.8.6')) {
    $rejected=$false; try { Assert-NativePostgresHealth $bad 55471 '123' } catch { $rejected=$true }
    if (-not $rejected) { throw 'Wrong server/listener/cluster/extension accepted' }
}
if ((Get-NativePostgresAclRecoveryAction 'before' 'before' 'granted') -cne 'unchanged' -or
    (Get-NativePostgresAclRecoveryAction 'granted' 'before' 'granted') -cne 'restore') { throw 'Interrupted ACL grant/restore is not replayable' }
$rejected=$false; try { $null=Get-NativePostgresAclRecoveryAction 'foreign' 'before' 'granted' } catch { $rejected=$true }
if (-not $rejected) { throw 'Unknown runtime ACL would be overwritten' }
if ((Get-NativePostgresConfigAction ('b'*64) ('a'*64) ('b'*64)) -cne 'unchanged' -or
    (Get-NativePostgresConfigAction ('a'*64) ('a'*64) ('b'*64)) -cne 'write') { throw 'Configuration interrupted replay is not idempotent' }
$rejected=$false; try { $null=Get-NativePostgresConfigAction ('c'*64) ('a'*64) ('b'*64) } catch { $rejected=$true }
if (-not $rejected) { throw 'Unknown configuration would be overwritten' }
if ((Get-NativePostgresInitAction $false $false @().Count 'cluster_created') -cne 'initialize' -or
    (Get-NativePostgresInitAction $true $true 5 'initializing') -cne 'recover') { throw 'Initialization recovery action differs' }
foreach ($state in @(@($true,$false,3,'initializing'),@($false,$false,3,'initializing'),@($true,$true,5,'prepared'))) {
    $rejected=$false; try { $null=Get-NativePostgresInitAction $state[0] $state[1] $state[2] $state[3] } catch { $rejected=$true }
    if (-not $rejected) { throw 'Unknown/partial initialization would be replayed' }
}
$source=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-postgres.ps1') -Raw
foreach ($required in @("Save-PgPhase 'initializing'",'Get-NativePostgresInitAction','external_effects_retained=$true',
    'if ($journalOwned -and $Action',"Save-PgPhase 'completed'",'Assert-NativePostgresHealth',
    "'--no-password'",'PGPASSFILE','service_restarted=$false')) { if (-not $source.Contains($required)) { throw 'Missing provisioning/recovery safeguard' } }
if ($source.Contains('PGPASSWORD') -or $source.Contains("@('delete',") -or $source.Contains('Remove-Item -LiteralPath $plan.cluster_directory')) { throw 'Credential argv/data-removal policy regression' }
if ($source.IndexOf("Save-PgPhase 'initializing'") -gt $source.IndexOf('Invoke-PgTool $plan.initdb_executable')) { throw 'Initialization intent must precede external work' }
if ($source.IndexOf('Assert-NativePostgresHealth $server') -gt $source.IndexOf('CREATE EXTENSION IF NOT EXISTS vector')) { throw 'Wrong-server identity must be rejected before database DDL' }
if ($source.IndexOf("'resume-control'") -gt $source.IndexOf("Save-PgPhase 'configuring'")) { throw 'Resumed control identity must be checked before cluster mutation' }
foreach ($required in @('Grant-PgRuntimeOperatorRead','finally { Restore-PgRuntimeAccess }',
    'Get-NativePostgresAclRecoveryAction','runtime-access.json',"(Join-Path `$root 'operator.lock')")) { if (-not $source.Contains($required)) { throw 'Missing restricted-token runtime access/recovery safeguard' } }
if ($source.Contains("['PG_RESTRICT_EXEC']")) { throw 'PostgreSQL token restriction must not be disabled' }
foreach ($name in @('native-postgres-functions.ps1','native-postgres-preview.ps1','native-postgres.ps1')) {
    $tokens=$null; $errors=$null
    $null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
Write-Output 'Native PostgreSQL provisioning plan, identity, readiness and recovery-policy tests passed (portable only; native execution pending)'
