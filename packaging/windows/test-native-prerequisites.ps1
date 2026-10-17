# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable contract tests, not native installation/runtime evidence.
$ErrorActionPreference='Stop'
$reader=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-prerequisites.ps1') -Raw
$compression=[regex]::Match($reader,'(?m)^\s*Add-Type -AssemblyName System.IO.Compression\s*$')
if(-not $compression.Success -or $compression.Index -gt $reader.IndexOf('[IO.Compression.ZipArchive]::new')){
    throw 'Windows PowerShell must load the ZipArchive assembly before extraction'
}
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
$hash='a'*64
$required=@('pgsql/bin/postgres.exe','pgsql/bin/initdb.exe','pgsql/bin/pg_ctl.exe','pgsql/bin/psql.exe',
    'pgsql/bin/vcruntime140.dll','pgsql/bin/msvcp140.dll','pgsql/lib/vector.dll','pgsql/lib/pgcrypto.dll',
    'pgsql/lib/pg_trgm.dll','pgsql/share/extension/vector.control','pgsql/share/extension/vector--0.8.7.sql',
    'pgsql/share/extension/pgcrypto.control','pgsql/share/extension/pg_trgm.control',
    'notices/postgresql.txt','notices/vector.txt','notices/crt.txt')
$manifest=[pscustomobject]@{schema_version=1;kind='codifix-native-postgres-runtime';platform='windows-x86_64';postgres_version='17.11';pgvector_version='0.8.7';crt_version='14.44.35211';package_sha256=$hash
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
$operation=[guid]'11111111-1111-4111-8111-111111111111'
$root=Join-Path ([IO.Path]::GetTempPath()) 'synthetic-runtime-root'
$destination=Join-Path $root ('postgres-17.11-vector-0.8.7-' + $hash)
$receipt=[pscustomobject]@{schema_version=1;phase='verified';operation_id=$operation.ToString();destination=$destination
    input_sha256=(Get-NativeInputDigest ([ordered]@{schema_version=1;manifest_sha256=$hash;plan=$plan;destination=$destination}))
    database_provisioned=$false;service_created=$false}
$binding=Get-NativePrerequisiteBinding $plan $hash $operation $root $receipt
if ($binding.database_readiness_verified -or $binding.verification_scope -cne 'protected_payload_only') { throw 'Runtime receipt falsely claims database readiness' }
foreach ($change in @(
    @{phase='staging'}, @{operation_id=[guid]::NewGuid().ToString()}, @{input_sha256=('b'*64)},
    @{destination=(Join-Path $root 'unrelated-runtime')}, @{database_provisioned=$true}, @{service_created=$true}
)) {
    $bad=$receipt | ConvertTo-Json | ConvertFrom-Json
    foreach ($key in $change.Keys) { $bad.$key=$change[$key] }
    $rejected=$false
    try { $null=Get-NativePrerequisiteBinding $plan $hash $operation $root $bad } catch { $rejected=$true }
    if (-not $rejected) { throw 'Changed or incomplete runtime receipt accepted' }
}
$instanceSource=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-instance.ps1') -Raw
if ($instanceSource.IndexOf('$runtimeLease=Open-NativePrerequisiteLease') -gt $instanceSource.IndexOf('Stop-Service -Name')) { throw 'Runtime must be revalidated before stopping service' }
$guardSource=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-instance-functions.ps1') -Raw
foreach($required in @('Open(string root) { return Open(root, 40000); }',
    'maxEntries == 40000 || maxEntries == 100000','Check(++count <= maxEntries',
    'public static ProtectedRelease OpenFile(string path)',
    'result.HoldAncestors(path)','Check(!result.Hold(path, false)',
    'directory || info.Links == 1','Release path is a reparse point','Release owner is not trusted')){
    if(-not $guardSource.Contains($required)){throw 'Protected input/tree guard policy changed'}
}
foreach($name in @('native-prerequisites.ps1','native-prerequisites-functions.ps1')){
    $source=Get-Content -LiteralPath (Join-Path $PSScriptRoot $name) -Raw
    foreach($required in @('::OpenFile($ManifestPath)',"`$treeEntryLimit=if(`$plan.kind -ceq 'codifix-native-pds-runtime'){100000}else{40000}")){
        if(-not $source.Contains($required)){throw 'Prerequisite guards must retain exact files and kind-scoped tree bounds'}
    }
}
$operator=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'native-prerequisites.ps1') -Raw
if(-not $operator.Contains('::OpenFile($PackagePath)')){throw 'Package guard must pin exact ZIP rather than unrelated sibling payloads'}
if((Get-NativeExtendedPath 'C:\reviewed\runtime\file.js') -cne '\\?\C:\reviewed\runtime\file.js' -or
    (Get-NativeExtendedPath '\\?\C:\reviewed\runtime\file.js') -cne '\\?\C:\reviewed\runtime\file.js'){throw 'Extended local path conversion differs'}
foreach($bad in @('relative','C:relative','\\server\share\file','\\.\pipe\file','C:\x\..\file','C:\x\file:stream','C:\x\NUL')){
    $rejected=$false;try{$null=Get-NativeExtendedPath $bad}catch{$rejected=$true}
    if(-not $rejected){throw 'Invalid local namespace became accepted through extended paths'}
}
foreach ($file in @('native-prerequisites.ps1','native-prerequisites-functions.ps1','native-instance.ps1')) {
    $tokens=$null; $errors=$null
    $null=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
}
Write-Output 'Native prerequisite descriptor, pinning, notices, state exclusion and preview-binding tests passed (portable only)'
