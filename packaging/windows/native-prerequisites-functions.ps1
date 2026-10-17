# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure descriptor validation; no network, service or database side effects.
function Get-NativePrerequisitePlan($Manifest) {
    if ($Manifest.kind -cne 'codifix-native-postgres-runtime') { return Get-NativeAdditionalRuntimePlan $Manifest }
    if ($Manifest.schema_version -ne 1 -or $Manifest.kind -cne 'codifix-native-postgres-runtime' -or
        $Manifest.platform -cne 'windows-x86_64' -or $Manifest.postgres_version -cne '17.11' -or
        $Manifest.pgvector_version -cne '0.8.7' -or $Manifest.crt_version -notmatch '^\d+(\.\d+){1,3}$' -or
        $Manifest.package_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Unsupported or unpinned native prerequisite descriptor' }
    $files = @($Manifest.payload_files)
    if ($files.Count -lt 1 -or $files.Count -gt 20000) { throw 'Complete bounded native payload inventory required' }
    $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $files) {
        Assert-NativeArchiveName $file.name
        if ($file.name.EndsWith('/') -or -not $names.Add($file.name) -or $file.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Duplicate or invalid native payload entry' }
        if ($file.name -match '(?i)(^|/)(data|pgdata|node_modules)(/|$)' -or $file.name -match '(?i)(^|/)(postgresql\.conf|pg_hba\.conf|pgpass\.conf|postmaster\.pid)$') { throw 'Runtime bundle must not contain database state or credentials' }
    }
    foreach ($required in @('pgsql/bin/postgres.exe','pgsql/bin/initdb.exe','pgsql/bin/pg_ctl.exe','pgsql/bin/psql.exe',
        'pgsql/bin/vcruntime140.dll','pgsql/bin/msvcp140.dll','pgsql/lib/vector.dll','pgsql/lib/pgcrypto.dll',
        'pgsql/lib/pg_trgm.dll','pgsql/share/extension/vector.control','pgsql/share/extension/vector--0.8.7.sql',
        'pgsql/share/extension/pgcrypto.control','pgsql/share/extension/pg_trgm.control')) {
        if (-not $names.Contains($required)) { throw "Incomplete native prerequisite: $required" }
    }
    $components = @($Manifest.components)
    if ($components.Count -ne 3) { throw 'PostgreSQL, pgvector and CRT provenance/notices required' }
    foreach ($name in @('postgresql','pgvector','msvc-crt')) {
        $entry = @($components | Where-Object { $_.name -ceq $name })
        if ($entry.Count -ne 1 -or $entry[0].source_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            -not $entry[0].version -or -not $entry[0].license -or -not $names.Contains($entry[0].notice_path)) { throw 'Missing native component source digest or bundled licence notice' }
        $version = switch ($name) { 'postgresql' { $Manifest.postgres_version }; 'pgvector' { $Manifest.pgvector_version }; 'msvc-crt' { $Manifest.crt_version } }
        if ($entry[0].version -cne $version) { throw 'Native component versions disagree' }
    }
    return [pscustomobject][ordered]@{ kind = $Manifest.kind; postgres_version = $Manifest.postgres_version
        pgvector_version = $Manifest.pgvector_version; crt_version = $Manifest.crt_version
        package_sha256 = $Manifest.package_sha256; payload_files = $files; components = $components
        native_stack_bundled = $false; database_provisioned = $false; service_created = $false }
}

function Get-NativePrerequisiteBinding($Plan, [string]$ManifestSha256, [guid]$OperationId, [string]$RuntimeRoot, $Receipt,
    [ValidateSet('codifix-native-postgres-runtime','codifix-native-pds-runtime','codifix-native-pi-runtime')][string]$ExpectedKind='codifix-native-postgres-runtime') {
    if ($ManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or $OperationId -eq [guid]::Empty) { throw 'Exact prerequisite manifest and operation identities required' }
    if ($Plan.kind -cne $ExpectedKind) { throw 'Runtime kind does not match this consumer' }
    $destination=Join-Path $RuntimeRoot (Get-NativeRuntimeIdentity $Plan $ManifestSha256)
    $input=Get-NativeInputDigest ([ordered]@{schema_version=1;manifest_sha256=$ManifestSha256;plan=$Plan;destination=$destination})
    if ($Receipt.schema_version -ne 1 -or $Receipt.phase -cne 'verified' -or
        $Receipt.operation_id -cne $OperationId.ToString() -or $Receipt.destination -cne $destination -or
        $Receipt.input_sha256 -cne $input -or $Receipt.database_provisioned -ne $false -or
        $Receipt.service_created -ne $false) { throw 'Native prerequisite receipt does not match the verified runtime' }
    $binding=[ordered]@{schema_version=1;manifest_sha256=$ManifestSha256;operation_id=$OperationId.ToString()
        input_sha256=$input;runtime_directory=$destination;postgres_bin=(Join-Path $destination 'pgsql/bin')
        verification_scope='protected_payload_only';database_readiness_verified=$false}
    if ($ExpectedKind -ceq 'codifix-native-postgres-runtime') { return [pscustomobject]$binding }
    $binding.Remove('postgres_bin'); $binding.Remove('database_readiness_verified')
    $binding.runtime_kind=$ExpectedKind; $binding.native_execution_verified=$false
    if ($ExpectedKind -ceq 'codifix-native-pds-runtime') {
        $binding.node_executable=Join-Path $destination 'node/node.exe';$binding.pds_entrypoint=Join-Path $destination 'pds/pds-server.mjs'
    } else { $binding.pi_executable=Join-Path $destination 'pi/pi.exe';$binding.artifact=$Plan.artifact }
    return [pscustomobject]$binding
}

function Close-NativePrerequisiteLease($Lease) {
    if ($null -ne $Lease) { foreach ($guard in $Lease.guards) { $guard.Dispose() } }
}

function Open-NativePrerequisiteLease([string]$ManifestPath, [string]$ManifestSha256, [guid]$OperationId,
    [ValidateSet('codifix-native-postgres-runtime','codifix-native-pds-runtime','codifix-native-pi-runtime')][string]$ExpectedKind='codifix-native-postgres-runtime') {
    Initialize-NativeReleaseGuard
    $guards=[Collections.Generic.List[IDisposable]]::new()
    try {
        $guards.Add([Codifix.ProtectedRelease]::OpenFile($ManifestPath))
        $manifestLength=(Get-Item -LiteralPath $ManifestPath).Length
        if ($manifestLength -gt 32MB -or
            (Get-FileHash -LiteralPath $ManifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Prerequisite manifest differs from pinned input' }
        $plan=Get-NativePrerequisitePlan (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json)
        Assert-NativeRuntimeManifestSize $manifestLength $plan.kind
        $root=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Codifix\runtimes'
        $journals=Join-Path $root 'operations'
        $guards.Add([Codifix.ProtectedRelease]::Open($journals))
        $receiptPath=Join-Path $journals ($OperationId.ToString() + '.json')
        if ((Get-Item -LiteralPath $receiptPath).Length -gt 1MB) { throw 'Prerequisite receipt too large' }
        $receipt=Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
        $binding=Get-NativePrerequisiteBinding $plan $ManifestSha256 $OperationId $root $receipt $ExpectedKind
        $treeEntryLimit=if($plan.kind -ceq 'codifix-native-pds-runtime'){100000}else{40000}
        $guards.Add([Codifix.ProtectedRelease]::Open($binding.runtime_directory,$treeEntryLimit))
        Assert-NativePayload $binding.runtime_directory $plan.payload_files -ExtendedPaths:($plan.kind -ceq 'codifix-native-pds-runtime')
        return [pscustomobject]@{binding=$binding;guards=$guards;plan=$plan}
    } catch { foreach ($guard in $guards) { $guard.Dispose() }; throw }
}

function Assert-NativeRuntimeFields($Value, [string[]]$Fields) {
    $names=if ($Value -is [Collections.IDictionary]) { @($Value.Keys) } else { @($Value.PSObject.Properties.Name) }
    if ((($names | Sort-Object) -join ',') -cne (($Fields | Sort-Object) -join ',')) { throw 'Unexpected or missing native runtime fields' }
}
function Assert-NativeRuntimeManifestSize([long]$Length,[string]$Kind) {
    $limit=if($Kind -ceq 'codifix-native-pds-runtime'){32MB}else{8MB}
    if($Length -lt 1 -or $Length -gt $limit){throw 'Native runtime manifest exceeds kind-specific bound'}
}
function Get-NativeRuntimeIdentity($Plan,[string]$ManifestSha256) {
    if ($ManifestSha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Exact runtime manifest digest required' }
    $prefix=switch -CaseSensitive ($Plan.kind) {
        'codifix-native-postgres-runtime' {'postgres-17.11-vector-0.8.7'}
        'codifix-native-pds-runtime' {'node-24.18.1-pds-0.5.27'}
        'codifix-native-pi-runtime' {'pi-0.5.1-no-tools-1-broker-1'}
        default {throw 'Unknown native runtime kind'}
    }
    return $prefix+'-'+$ManifestSha256
}
function Get-NativeAdditionalRuntimePlan($Manifest) {
    $common=@('schema_version','kind','platform','crt_version','package_sha256','payload_files','components')
    $pds=$Manifest.kind -ceq 'codifix-native-pds-runtime'
    if ($pds) { $extra=@('node_version','pds_version','pds_source_revision','pds_image_digest','pds_package_sha256','pds_lock_sha256','pnpm_version','better_sqlite3_version','sqlite_version','native_addons') }
    elseif ($Manifest.kind -ceq 'codifix-native-pi-runtime') { $extra=@('pi_version','artifact','source_archive_sha256','upstream_lock_sha256','rust_toolchain','rustc_commit','target') }
    else { throw 'Unknown native prerequisite kind' }
    $manifestFields=if($Manifest -is [Collections.IDictionary]){@($Manifest.Keys)}else{@($Manifest.PSObject.Properties.Name)}
    $patched=$pds -and $manifestFields -contains 'path_patch'
    if($patched){$extra+=@('original_prepared_sha256','path_patch')}
    Assert-NativeRuntimeFields $Manifest ($common+$extra)
    if ($Manifest.schema_version -ne 1 -or $Manifest.platform -cne 'windows-x86_64' -or
        $Manifest.crt_version -cnotmatch '^\d+(\.\d+){1,3}$' -or $Manifest.package_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Unsupported native runtime schema or pins' }
    $files=@($Manifest.payload_files)
    $fileLimit=if($pds){65536}else{20000}
    if ($files.Count -lt 1 -or $files.Count -gt $fileLimit) { throw 'Complete bounded payload inventory required' }
    $inventory=@{};$names=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $files) {
        Assert-NativeRuntimeFields $file @('name','sha256');Assert-NativeArchiveName $file.name
        if ($file.name.EndsWith('/') -or -not $names.Add($file.name) -or $file.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Malformed or colliding runtime payload' }
        # The exact reviewed dependency tree can contain public test certificates,
        # example settings or SQLite fixtures. Do not strip or interpret those
        # bytes. Operator-owned state is excluded by the top-level layout below;
        # dependencies themselves remain bound by the complete manifest digest.
        $dependencyMember=$pds -and $file.name.StartsWith('pds/node_modules/',[StringComparison]::Ordinal)
        if (-not $dependencyMember -and ($file.name -match '(?i)(^|/)(\.env(?:\.[^/]*)?|pgpass\.conf|auth\.json|credentials\.json|settings\.json|models\.json|postmaster\.pid)$' -or
            $file.name -match '(?i)\.(sqlite(?:3)?|db|jwt|key|pem)$')) { throw 'Runtime payload must not contain credentials or persisted state' }
        $inventory[$file.name]=$file.sha256
    }
    $required=@{};$versions=[ordered]@{}
    if ($pds) {
        if ($Manifest.node_version -cne '24.18.1' -or $Manifest.pds_version -cne '0.5.27' -or
            $Manifest.pds_source_revision -cne '374cf1d4ba782d4391bbb73e4e2d3f320d4846d6' -or
            $Manifest.pds_image_digest -cne 'sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12' -or
            $Manifest.pds_package_sha256 -cne 'c0e7740808e0d6bc26a9fd773162f780a45665b0cff2137a256157f3cff77fd5' -or
            $Manifest.pds_lock_sha256 -cne '23a8c97e01dff561266a6dcf67a4b74b25e98ebc695e6fa17c60ddf964160f73' -or
            $Manifest.pnpm_version -cne '10.34.1' -or $Manifest.better_sqlite3_version -cne '12.11.1' -or
            $Manifest.sqlite_version -cne '3.53.2') { throw 'Unreviewed Node/PDS baseline' }
        $required=@{'node/node.exe'=$null;'pds/package.json'=$Manifest.pds_package_sha256;'pds/pnpm-lock.yaml'=$Manifest.pds_lock_sha256
            'pds/pds-server.mjs'='cf223ee540400e244fa55f2e7d6e718195f0f8066aa3623c1c57df37acec3fb8'
            'pds/http-stdio.mjs'='9d7a04af99b878e4afdcce064e7c64b4a316bee345c96dfc14fce2ad14e481a2'
            'pds/node_modules/@atproto/pds/package.json'=$null;'pds/node_modules/@atproto/pds/dist/index.js'=$null
            'pds/node_modules/better-sqlite3/build/Release/better_sqlite3.node'=$null}
        if($patched){
            if($Manifest.original_prepared_sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact original PDS preparation digest required'}
            $patchPins=Assert-NativePdsPathPatch $Manifest.path_patch
            foreach($name in $patchPins.Keys){$required['pds/node_modules/@atproto/pds/dist/'+$name]=$patchPins[$name].patched_sha256}
            $required['pds/windows-paths.mjs']=$Manifest.path_patch.patch_sha256
        }
        foreach ($name in $inventory.Keys) {
            if ($name -cnotmatch '^(?:node/(?:node\.exe|vcruntime140(?:_1)?\.dll|msvcp140\.dll)|pds/(?:package\.json|pnpm-lock\.yaml|pds-server\.mjs|http-stdio\.mjs|node_modules/.+)|notices/[^/]+\.txt)$' -and
                -not($patched -and $name -ceq 'pds/windows-paths.mjs')) { throw 'Unsupported PDS runtime path; do not package a working state directory' }
        }
        $addons=@($Manifest.native_addons);$addonNames=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        if ($addons.Count -lt 1 -or $addons.Count -gt 32) { throw 'Native addon inventory required' }
        foreach($addon in $addons){
            Assert-NativeRuntimeFields $addon @('name','sha256')
            if ($addon.name -cnotmatch '^pds/node_modules/.+\.node$' -or -not $addonNames.Add($addon.name) -or
                -not $inventory.ContainsKey($addon.name) -or $addon.sha256 -cne $inventory[$addon.name]) { throw 'Native addon differs from complete prepared payload' }
        }
        if (@($inventory.Keys | Where-Object { $_ -match '\.node$' }).Count -ne $addons.Count) { throw 'Unrecorded native addon' }
        $versions=[ordered]@{node='24.18.1';pds='0.5.27';'locked-dependencies'=$Manifest.pds_lock_sha256;'codifix-pds-adapter'='stdio-v1';'msvc-crt'=$Manifest.crt_version}
        if($patched){$versions['codifix-pds-path-patch']='windows-sha256-did-paths-v1'}
        $binaryRoot='node'
    } else {
        if ($Manifest.pi_version -cne '0.5.1' -or
            $Manifest.source_archive_sha256 -cne '5408d25ab40e49ce42ea1622c3bdd0128f13dacc53e11e3548cacf48d8908bbc' -or
            $Manifest.upstream_lock_sha256 -cne '1aea4e3fe13a86760317d9c3da80d7461bd5cd5c013508ab5446a391c7fa14e5' -or
            $Manifest.rust_toolchain -cne 'nightly-2026-08-31' -or $Manifest.rustc_commit -cne '90850177249efe0321573c569aec5d12b257f8d6' -or
            $Manifest.target -cne 'x86_64-pc-windows-msvc') { throw 'Unreviewed Pi source/compiler baseline' }
        $pins=[ordered]@{source_revision='7c018475d54d4e09c907691e47a520f1f09e3288'
            no_tools_patch_sha256='bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc'
            broker_patch_sha256='61a620316d545b100cf17a83503aba4da46094346c7996c502508b9060aebdf4'
            portability_patch_sha256='241551af0ec017a63f31cf2aba93122f961998f7380a60abe162504d78cd70c6'
            helper_manifest_sha256='89c7d41c710c69122099b353177271a493ccfa5997918b5b8c62268714db0371'
            helper_source_sha256='3fd55c35ffb6418a8200ea221f96b9300f8bea90d9b9e1cab2f3ce68f9d859c3'
            patched_lock_sha256='362e708ba62267ca061a7e8172f663a4994b78990141d383879198132c74dfd6'}
        Assert-NativeRuntimeFields $Manifest.artifact (@($pins.Keys)+@('binary_sha256'))
        foreach($key in $pins.Keys){if($Manifest.artifact.$key -cne $pins[$key]){throw 'Pi provenance differs from pinned patches/source'}}
        if($Manifest.artifact.binary_sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Pinned Pi binary required'}
        $required=@{'pi/pi.exe'=$Manifest.artifact.binary_sha256;'source/pi-source.tar'=$Manifest.source_archive_sha256
            'source/no-tools-1.patch'=$pins.no_tools_patch_sha256;'source/windows-broker-1.patch'=$pins.broker_patch_sha256
            'source/windows-broker-safe-handle-1.patch'=$pins.portability_patch_sha256
            'source/windows-pipe-helper/Cargo.toml'=$pins.helper_manifest_sha256;'source/windows-pipe-helper/src/lib.rs'=$pins.helper_source_sha256
            'source/Cargo.lock'=$pins.patched_lock_sha256;'notices/pi.txt'='32a82e0a5754e72e51fae44b65a936c831c07376f21c90f5fb9e76897fcc3509'}
        foreach($name in $inventory.Keys){
            if($name -cnotmatch '^(?:pi/(?:pi\.exe|vcruntime140(?:_1)?\.dll|msvcp140\.dll)|source/(?:pi-source\.tar|no-tools-1\.patch|windows-broker-1\.patch|windows-broker-safe-handle-1\.patch|Cargo\.lock|windows-pipe-helper/(?:Cargo\.toml|src/lib\.rs))|notices/[^/]+\.txt)$'){throw 'Unsupported Pi runtime path; no sessions/settings/credentials may be bundled'}
        }
        $versions=[ordered]@{pi='0.5.1';'codifix-windows-pipe'='0.1.0';'msvc-crt'=$Manifest.crt_version}
        $binaryRoot='pi'
    }
    foreach($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')){$required[$binaryRoot+'/'+$name]=$null}
    foreach($name in $required.Keys){
        if(-not $inventory.ContainsKey($name) -or ($null -ne $required[$name] -and $inventory[$name] -cne $required[$name])){throw 'Missing or changed pinned native runtime member'}
    }
    $components=@($Manifest.components)
    if($components.Count -ne $versions.Count){throw 'Complete runtime component notices required'}
    foreach($name in $versions.Keys){
        $component=@($components | Where-Object {$_.name -ceq $name})
        if($component.Count -ne 1){throw 'Missing or duplicate native component provenance'}
        $entry=$component[0];Assert-NativeRuntimeFields $entry @('name','version','source_sha256','license','notice_path')
        if($entry.version -cne $versions[$name] -or $entry.source_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            [string]::IsNullOrWhiteSpace($entry.license) -or $entry.license.Length -gt 256 -or
            $entry.notice_path -cnotmatch '^notices/[^/]+\.txt$' -or -not $inventory.ContainsKey($entry.notice_path)){throw 'Component version, source hash or notice missing'}
        if($pds -and (($name -ceq 'pds' -and $entry.source_sha256 -cne $Manifest.pds_image_digest.Substring(7)) -or
            ($name -ceq 'locked-dependencies' -and $entry.source_sha256 -cne $Manifest.pds_lock_sha256) -or
            ($name -ceq 'codifix-pds-adapter' -and $entry.source_sha256 -cne $required['pds/pds-server.mjs']))){throw 'PDS component provenance contradicts fixed input pins'}
        if($pds -and $patched -and $name -ceq 'codifix-pds-path-patch' -and
            ($entry.source_sha256 -cne $Manifest.path_patch.patch_sha256 -or $entry.license -cne 'AGPL-3.0-or-later')){throw 'PDS path patch provenance or licence differs'}
        if(-not $pds -and (($name -ceq 'pi' -and ($entry.source_sha256 -cne $Manifest.source_archive_sha256 -or
            $entry.notice_path -cne 'notices/pi.txt' -or $entry.license -cne 'MIT-with-OpenAI-Anthropic-Rider')) -or
            ($name -ceq 'codifix-windows-pipe' -and ($entry.source_sha256 -cne $Manifest.artifact.helper_source_sha256 -or
            $entry.license -cne 'AGPL-3.0-or-later')))) {throw 'Pi component provenance or licence description contradicts pinned source'}
    }
    $plan=[ordered]@{}
    foreach($field in ($common+$extra)){$plan[$field]=$Manifest.$field}
    $plan.native_stack_bundled=$false;$plan.database_provisioned=$false;$plan.service_created=$false
    return [pscustomobject]$plan
}

function Assert-NativePdsPathPatch($Patch){
    Assert-NativeRuntimeFields $Patch @('layout','patch_sha256','files')
    if($Patch.layout -cne 'windows-sha256-did-paths-v1' -or
        $Patch.patch_sha256 -cne '79c8c8378fdab250457e663d2d48518cb28dbabf5a81b3ae832aa70a7b860441'){throw 'Unreviewed PDS filesystem patch'}
    $pins=[ordered]@{
        'actor-store/actor-store.js'=[ordered]@{original_sha256='33ba4bcf0b0afb06868b80a4cb0025388a2a7e8006338ce80a70785d2079af4f';patched_sha256='c192431cb2de642bbe5c8256fbe30f0de6fe5dc23f3b20b8baec71e969909965'}
        'disk-blobstore.js'=[ordered]@{original_sha256='cc18ec422b23ba3bc06baea321146e512ce9cb14608ec90ccfe5e9bc71359811';patched_sha256='9da2aa49a53b58826ae1b24fd7ba8d4fbbe7c9aaf9c7ee8a24c91c792b736f5a'}
    }
    if(@($Patch.files).Count -ne 2){throw 'Exactly two reviewed PDS path transformations required'}
    foreach($name in $pins.Keys){
        $entries=@($Patch.files | Where-Object {$_.name -ceq $name})
        if($entries.Count -ne 1){throw 'Missing or duplicate PDS path transformation'}
        Assert-NativeRuntimeFields $entries[0] @('name','original_sha256','patched_sha256')
        if($entries[0].original_sha256 -cne $pins[$name].original_sha256 -or
            $entries[0].patched_sha256 -cne $pins[$name].patched_sha256){throw 'PDS transformation differs from exact reviewed source/output'}
    }
    return $pins
}
