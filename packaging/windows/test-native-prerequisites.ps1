# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable contract tests, not native installation/runtime evidence.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
$hash='a'*64
$required=@('pgsql/bin/postgres.exe','pgsql/bin/initdb.exe','pgsql/bin/pg_ctl.exe','pgsql/bin/psql.exe',
    'pgsql/bin/vcruntime140.dll','pgsql/bin/msvcp140.dll','pgsql/lib/vector.dll','pgsql/lib/pgcrypto.dll',
    'pgsql/lib/pg_trgm.dll','pgsql/share/extension/vector.control','pgsql/share/extension/vector--0.8.7.sql',
    'pgsql/share/extension/pgcrypto.control','pgsql/share/extension/pg_trgm.control',
    'notices/postgresql.txt','notices/vector.txt','notices/crt.txt')
$manifest=[pscustomobject]@{schema_version=1;kind='memory-native-postgres-runtime';platform='windows-x86_64';postgres_version='17.11';pgvector_version='0.8.7';crt_version='14.44.35211';package_sha256=$hash
    payload_files=@($required | ForEach-Object { [pscustomobject]@{name=$_;sha256=$hash} })
    components=@(
        [pscustomobject]@{name='postgresql';version='17.11';source_sha256=$hash;license='PostgreSQL';notice_path='notices/postgresql.txt'},
        [pscustomobject]@{name='pgvector';version='0.8.7';source_sha256=$hash;license='PostgreSQL';notice_path='notices/vector.txt'},
        [pscustomobject]@{name='msvc-crt';version='14.44.35211';source_sha256=$hash;license='Microsoft redistributable terms';notice_path='notices/crt.txt'})}
$plan=Get-NativePrerequisitePlan $manifest
if ($plan.native_stack_bundled -or $plan.database_provisioned -or $plan.service_created) { throw 'Runtime falsely claims complete stack or database installation' }
$original=Get-NativeInputDigest $plan
foreach ($mutate in @(
    { param($m) $m.postgres_version='17.12' },
    { param($m) $m.components[0].version='17.10' },
    { param($m) $m.components[0].source_sha256='missing' },
    { param($m) $m.components[2].notice_path='missing.txt' },
    { param($m) $m.payload_files=@($m.payload_files | Where-Object name -ne 'pgsql/bin/msvcp140.dll') },
    { param($m) $m.payload_files+= [pscustomobject]@{name='PGSQL/bin/POSTGRES.exe';sha256=('a'*64)} },
    { param($m) $m.payload_files+= [pscustomobject]@{name='../outside';sha256=('a'*64)} },
    { param($m) $m.payload_files+= [pscustomobject]@{name='pgsql/data/PG_VERSION';sha256=('a'*64)} }
)) {
    $copy=$manifest | ConvertTo-Json -Depth 16 | ConvertFrom-Json
    & $mutate $copy
    $rejected=$false
    try { $null=Get-NativePrerequisitePlan $copy } catch { $rejected=$true }
    if (-not $rejected) { throw 'Invalid runtime manifest accepted' }
}
$changed=$manifest | ConvertTo-Json -Depth 16 | ConvertFrom-Json
$changed.payload_files[0].sha256='b'*64
if ((Get-NativeInputDigest (Get-NativePrerequisitePlan $changed)) -eq $original) { throw 'Changed runtime input did not change preview digest' }
foreach ($file in @('native-prerequisites.ps1','native-prerequisites-functions.ps1')) {
    $tokens=$null; $errors=$null
    $null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
Write-Output 'Native prerequisite descriptor, pinning, notices, state exclusion and preview-binding tests passed (portable only)'
