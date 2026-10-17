# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure descriptors/bindings only; no install, provider, SCM or VM execution.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
function Reject([scriptblock]$Action) {$failed=$false;try{$null=& $Action}catch{$failed=$true};if(-not $failed){throw 'Invalid additional runtime accepted'}}
function Copy-Value($Value){return $Value | ConvertTo-Json -Depth 20 | ConvertFrom-Json}
function Component($Name,$Version,$Source,$License='MIT'){return [pscustomobject]@{name=$Name;version=$Version;source_sha256=$Source;license=$License;notice_path=('notices/'+$Name+'.txt')}}
$hash='a'*64
Assert-NativeRuntimeManifestSize 32MB 'memory-native-pds-runtime'
Reject {Assert-NativeRuntimeManifestSize (32MB+1) 'memory-native-pds-runtime'}
foreach($kind in @('memory-native-postgres-runtime','memory-native-pi-runtime')){
    Assert-NativeRuntimeManifestSize 8MB $kind
    Reject {Assert-NativeRuntimeManifestSize (8MB+1) $kind}
}
$pdsPins=@{'pds/package.json'='c0e7740808e0d6bc26a9fd773162f780a45665b0cff2137a256157f3cff77fd5'
    'pds/pnpm-lock.yaml'='23a8c97e01dff561266a6dcf67a4b74b25e98ebc695e6fa17c60ddf964160f73'
    'pds/pds-server.mjs'='cf223ee540400e244fa55f2e7d6e718195f0f8066aa3623c1c57df37acec3fb8'
    'pds/http-stdio.mjs'='0dac0e7fe040be14398c2304ce4b70e8d65b74023d52d80b79e88113a5e4c467'}
$addon='pds/node_modules/better-sqlite3/build/Release/better_sqlite3.node'
$pds=[pscustomobject]@{schema_version=1;kind='memory-native-pds-runtime';platform='windows-x86_64';node_version='24.18.1';pds_version='0.5.27'
    pds_source_revision='374cf1d4ba782d4391bbb73e4e2d3f320d4846d6';pds_image_digest='sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12'
    pds_package_sha256=$pdsPins['pds/package.json'];pds_lock_sha256=$pdsPins['pds/pnpm-lock.yaml'];pnpm_version='10.34.1';better_sqlite3_version='12.11.1';sqlite_version='3.53.2'
    crt_version='14.44.35211';package_sha256=$hash;payload_files=@();native_addons=@([pscustomobject]@{name=$addon;sha256=$hash})
    components=@((Component 'node' '24.18.1' $hash),(Component 'pds' '0.5.27' 'd95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12'),
        (Component 'locked-dependencies' $pdsPins['pds/pnpm-lock.yaml'] $pdsPins['pds/pnpm-lock.yaml'] 'multiple-see-notices'),
        (Component 'memory-pds-adapter' 'stdio-v1' $pdsPins['pds/pds-server.mjs'] 'AGPL-3.0-or-later'),(Component 'msvc-crt' '14.44.35211' $hash 'Microsoft redistributable terms'))}
$pdsNames=@('node/node.exe','node/vcruntime140.dll','node/vcruntime140_1.dll','node/msvcp140.dll',
    'pds/node_modules/@atproto/pds/package.json','pds/node_modules/@atproto/pds/dist/index.js',$addon)+@($pdsPins.Keys)+@($pds.components | ForEach-Object notice_path)
$pds.payload_files=@($pdsNames | ForEach-Object {[pscustomobject]@{name=$_;sha256=$(if($pdsPins.ContainsKey($_)){$pdsPins[$_]}else{$hash})}})
$artifact=[pscustomobject]@{source_revision='7c018475d54d4e09c907691e47a520f1f09e3288'
    no_tools_patch_sha256='bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc'
    broker_patch_sha256='61a620316d545b100cf17a83503aba4da46094346c7996c502508b9060aebdf4'
    portability_patch_sha256='241551af0ec017a63f31cf2aba93122f961998f7380a60abe162504d78cd70c6'
    helper_manifest_sha256='89c7d41c710c69122099b353177271a493ccfa5997918b5b8c62268714db0371'
    helper_source_sha256='3fd55c35ffb6418a8200ea221f96b9300f8bea90d9b9e1cab2f3ce68f9d859c3'
    patched_lock_sha256='362e708ba62267ca061a7e8172f663a4994b78990141d383879198132c74dfd6';binary_sha256=$hash}
$pi=[pscustomobject]@{schema_version=1;kind='memory-native-pi-runtime';platform='windows-x86_64';pi_version='0.5.1';artifact=$artifact
    source_archive_sha256='5408d25ab40e49ce42ea1622c3bdd0128f13dacc53e11e3548cacf48d8908bbc';upstream_lock_sha256='1aea4e3fe13a86760317d9c3da80d7461bd5cd5c013508ab5446a391c7fa14e5'
    rust_toolchain='nightly-2026-08-31';rustc_commit='90850177249efe0321573c569aec5d12b257f8d6';target='x86_64-pc-windows-msvc'
    crt_version='14.44.35211';package_sha256=$hash;payload_files=@();components=@(
        (Component 'pi' '0.5.1' '5408d25ab40e49ce42ea1622c3bdd0128f13dacc53e11e3548cacf48d8908bbc' 'MIT-with-OpenAI-Anthropic-Rider'),
        (Component 'memory-windows-pipe' '0.1.0' $artifact.helper_source_sha256 'AGPL-3.0-or-later'),(Component 'msvc-crt' '14.44.35211' $hash 'Microsoft redistributable terms'))}
$piPins=@{'pi/pi.exe'=$hash;'source/pi-source.tar'=$pi.source_archive_sha256;'source/no-tools-1.patch'=$artifact.no_tools_patch_sha256
    'source/windows-broker-1.patch'=$artifact.broker_patch_sha256;'source/windows-broker-safe-handle-1.patch'=$artifact.portability_patch_sha256
    'source/windows-pipe-helper/Cargo.toml'=$artifact.helper_manifest_sha256;'source/windows-pipe-helper/src/lib.rs'=$artifact.helper_source_sha256
    'source/Cargo.lock'=$artifact.patched_lock_sha256;'notices/pi.txt'='32a82e0a5754e72e51fae44b65a936c831c07376f21c90f5fb9e76897fcc3509'}
$piNames=@($piPins.Keys)+@('pi/vcruntime140.dll','pi/vcruntime140_1.dll','pi/msvcp140.dll','notices/memory-windows-pipe.txt','notices/msvc-crt.txt')
$pi.payload_files=@($piNames | ForEach-Object {[pscustomobject]@{name=$_;sha256=$(if($piPins.ContainsKey($_)){$piPins[$_]}else{$hash})}})
foreach($manifest in @($pds,$pi)){
    $plan=Get-NativePrerequisitePlan $manifest
    if($plan.native_stack_bundled -or $plan.database_provisioned -or $plan.service_created){throw 'Byte deployment claimed activation'}
    $operation=[guid]'11111111-1111-4111-8111-111111111111';$root=Join-Path ([IO.Path]::GetTempPath()) 'synthetic-runtimes'
    $destination=Join-Path $root (Get-NativeRuntimeIdentity $plan $hash)
    $receipt=[pscustomobject]@{schema_version=1;phase='verified';operation_id=$operation.ToString();destination=$destination;database_provisioned=$false;service_created=$false
        input_sha256=(Get-NativeInputDigest ([ordered]@{schema_version=1;manifest_sha256=$hash;plan=$plan;destination=$destination}))}
    Reject {Get-NativePrerequisiteBinding $plan $hash $operation $root $receipt}
    $binding=Get-NativePrerequisiteBinding $plan $hash $operation $root $receipt $manifest.kind
    if($binding.runtime_kind -cne $manifest.kind -or $binding.native_execution_verified -or $binding.PSObject.Properties.Name -contains 'postgres_bin'){throw 'Runtime consumer kind confused'}
    $changed=Copy-Value $manifest;$changed.payload_files[0].sha256='b'*64
    # A pinned member may reject outright; an unpinned built byte must alter the
    # exact review digest, never reuse the old operation silently.
    try{$changedPlan=Get-NativePrerequisitePlan $changed;if((Get-NativeInputDigest $changedPlan) -ceq (Get-NativeInputDigest $plan)){throw 'unchanged'}}catch{if($_.Exception.Message -ceq 'unchanged'){throw}}
    foreach($change in @({param($m)$m.platform='windows-arm64'},{param($m)$m.kind='unknown'},
        {param($m)$m | Add-Member -NotePropertyName credentials -NotePropertyValue 'private'},
        {param($m)$m.payload_files+=[pscustomobject]@{name='../escape';sha256=('a'*64)}},
        {param($m)$m.payload_files+=[pscustomobject]@{name='state/account.sqlite';sha256=('a'*64)}},
        {param($m)$m.payload_files+=[pscustomobject]@{name='notices/.env';sha256=('a'*64)}},
        {param($m)$m.payload_files+=[pscustomobject]@{name=$m.payload_files[0].name.ToUpperInvariant();sha256=('a'*64)}},
        {param($m)$m.components[0].source_sha256='invalid'},{param($m)$m.components[0].notice_path='missing.txt'})){
        $copy=Copy-Value $manifest;& $change $copy;Reject {Get-NativePrerequisitePlan $copy}
    }
}
foreach($change in @({param($m)$m.node_version='24.19.0'},{param($m)$m.pds_source_revision=('0'*40)},
    {param($m)$m.better_sqlite3_version='12.11.2'},{param($m)$m.sqlite_version='12.11.1'},
    {param($m)$m.payload_files=@($m.payload_files | Where-Object name -ne 'node/vcruntime140_1.dll')},
    {param($m)$m.native_addons[0].sha256=('b'*64)},
    {param($m)$m.payload_files+=[pscustomobject]@{name='pds/node_modules/other/extra.node';sha256=('a'*64)}})){
    $copy=Copy-Value $pds;& $change $copy;Reject {Get-NativePrerequisitePlan $copy}
}
# Preserve the complete reviewed dependency tree, including public fixtures.
# The observed prepared tree exceeds the historical PostgreSQL 20k limit.
$complete=Copy-Value $pds
$complete.payload_files+=@('test/server.key','test/cert.pem','test/settings.json','test/example.db' | ForEach-Object {
    [pscustomobject]@{name=('pds/node_modules/fixture-package/'+$_);sha256=$hash}
})
$complete.payload_files+=@(1..40509 | ForEach-Object {[pscustomobject]@{name=('pds/node_modules/fixture-package/source/'+$_+'.js');sha256=$hash}})
$null=Get-NativePrerequisitePlan $complete
$complete.payload_files+=@(40510..65536 | ForEach-Object {[pscustomobject]@{name=('pds/node_modules/fixture-package/source/'+$_+'.js');sha256=$hash}})
Reject {Get-NativePrerequisitePlan $complete}
foreach($change in @({param($m)$m.artifact.broker_patch_sha256=('0'*64)},
    {param($m)$m.components[0].license='MIT'},
    {param($m)$m.payload_files=@($m.payload_files | Where-Object name -ne 'source/pi-source.tar')},
    {param($m)$m.rustc_commit=('0'*40)},
    {param($m)$m.payload_files+=[pscustomobject]@{name='pi/sessions/latest.json';sha256=('a'*64)}})){
    $copy=Copy-Value $pi;& $change $copy;Reject {Get-NativePrerequisitePlan $copy}
}
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
foreach($pair in @(@('packaging/windows-pds/pds-server.mjs',$pdsPins['pds/pds-server.mjs']),@('packaging/windows-pds/http-stdio.mjs',$pdsPins['pds/http-stdio.mjs']))){
    if((Get-FileHash -LiteralPath (Join-Path $repo $pair[0])).Hash.ToLowerInvariant() -cne $pair[1]){throw 'Reviewed adapter source changed without prerequisite pin review'}
}
foreach($pair in @(
    @('crates/mem-supervisor-pi/patches/no-tools-1.patch',$artifact.no_tools_patch_sha256),
    @('crates/mem-supervisor-pi/patches/windows-broker-1.patch',$artifact.broker_patch_sha256),
    @('crates/mem-supervisor-pi/patches/windows-broker-safe-handle-1.patch',$artifact.portability_patch_sha256),
    @('crates/mem-supervisor-pi/windows-pipe-helper/Cargo.toml',$artifact.helper_manifest_sha256),
    @('crates/mem-supervisor-pi/windows-pipe-helper/src/lib.rs',$artifact.helper_source_sha256),
    @('crates/mem-supervisor-pi/patches/LICENSE.pi',$piPins['notices/pi.txt']))){
    if((Get-FileHash -LiteralPath (Join-Path $repo $pair[0])).Hash.ToLowerInvariant() -cne $pair[1]){throw 'Reviewed Pi source changed without prerequisite pin review'}
}
Write-Output 'Portable Node/PDS and Pi runtime schema, source, notices, exclusions and consumer binding checks passed; native deployment remains unverified.'
