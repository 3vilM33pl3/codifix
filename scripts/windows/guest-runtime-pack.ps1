# SPDX-License-Identifier: AGPL-3.0-or-later
# Offline packaging only: no download, database, service or provider invocation.
[CmdletBinding()]
param(
    [ValidateSet('Postgres','Pds','Pi')][string]$Kind,
    [guid]$OperationId,
    [string]$SourceRoot,
    [guid]$PiBuildOperationId,
    [string]$PdsProfilePath,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$PdsProfileSha256,
    [string]$CrtNoticePath,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$CrtNoticeSha256,
    [string]$PostgresNoticePath,
    [switch]$FunctionsOnly
)
$ErrorActionPreference='Stop'
function Assert-PackPath([string]$Path) {
    $item=Get-Item -LiteralPath $Path -Force
    while($null -ne $item){
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Package input/output must not traverse a reparse point'}
        if($item -is [IO.DirectoryInfo]){$item=$item.Parent}else{$item=$item.Directory}
    }
}
function Get-PackHash([string]$Path){Assert-PackPath $Path;return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()}
function Assert-PackHash([string]$Path,[string]$Expected){if($Expected -cnotmatch '^[0-9a-f]{64}$' -or (Get-PackHash $Path) -cne $Expected){throw 'Recorded native packaging input changed'}}
function Read-PackJson([string]$Path,[long]$Limit=32MB){
    Assert-PackPath $Path
    if((Get-Item -LiteralPath $Path).Length -gt $Limit){throw 'Packaging receipt exceeds its bound'}
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}
function Copy-PackFile([string]$Source,[string]$Relative,[string]$Expected) {
    Assert-NativeArchiveName $Relative
    Assert-PackHash $Source $Expected
    $target=Join-Path $script:payload $Relative.Replace('/',[IO.Path]::DirectorySeparatorChar)
    $null=New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force
    # Never replace an earlier selected member, including case aliases.
    $inputStream=[IO.File]::Open($Source,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try{
        $outputStream=[IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try{$inputStream.CopyTo($outputStream);$outputStream.Flush($true)}finally{$outputStream.Dispose()}
    }finally{$inputStream.Dispose()}
    Assert-PackHash $target $Expected
}
function New-PackComponent([string]$Name,[string]$Version,[string]$SourceHash,[string]$License,[string]$Notice) {
    return [ordered]@{name=$Name;version=$Version;source_sha256=$SourceHash;license=$License;notice_path=$Notice}
}
function Assert-PackPostgresRuntime([string]$Bin,[string]$Lib,$Prepared){
    $records=@($Prepared.runtime_support)
    $names=@($records | ForEach-Object name | Sort-Object)
    $actual=@(Get-ChildItem -LiteralPath $Bin -Filter '*.dll' -File | ForEach-Object Name | Sort-Object)
    if($names.Count -lt 3 -or @($names | Select-Object -Unique).Count -ne $names.Count -or
        @($names | Where-Object {$_ -cnotmatch '^[A-Za-z0-9_.-]+\.dll$'}).Count -or
        @(Compare-Object $names $actual -CaseSensitive).Count){throw 'Prepared PostgreSQL runtime support inventory differs'}
    foreach($record in $records){Assert-PackHash (Join-Path $Bin $record.name) $record.sha256}
    $extensions=@($Prepared.extensions)
    $extensionNames=@($extensions | ForEach-Object name | Sort-Object)
    if($extensionNames.Count -ne 3 -or @(Compare-Object @('pg_trgm.dll','pgcrypto.dll','vector.dll') $extensionNames -CaseSensitive).Count){throw 'Exact prepared PostgreSQL extension inventory required'}
    foreach($record in $extensions){Assert-PackHash (Join-Path $Lib $record.name) $record.sha256}
    Assert-PackHash (Join-Path $Bin 'postgres.exe') $Prepared.postgres_sha256
}
function Assert-PackPdsPatchDelta($Original,$Patched,[string]$OriginalSha256){
    if($Patched.format -cne 'memory-windows-pds-patched-proof-v1' -or
        $OriginalSha256 -cnotmatch '^[0-9a-f]{64}$' -or $Patched.original_prepared_sha256 -cne $OriginalSha256 -or
        $Patched.manifest_sha256 -cne $Original.manifest_sha256 -or $Patched.node_sha256 -cne $Original.node_sha256 -or
        $Patched.source_revision -cne $Original.source_revision -or $Patched.native_sqlite_verified -ne $Original.native_sqlite_verified){throw 'Patched PDS preparation is not bound to original proof'}
    $pins=Assert-NativePdsPathPatch $Patched.path_patch
    $expected=[Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach($member in $Original.files){
        Assert-NativeArchiveName $member.path
        if(-not $seen.Add($member.path) -or $member.sha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Invalid original PDS member inventory'}
        $expected[$member.path]=$member.sha256
    }
    foreach($name in $pins.Keys){
        $relative='node_modules/@atproto/pds/dist/'+$name
        if($expected[$relative] -cne $pins[$name].original_sha256){throw 'Original PDS path source differs from reviewed upstream'}
        $expected[$relative]=$pins[$name].patched_sha256
    }
    if(@($Patched.files).Count -ne $expected.Count){throw 'Patched PDS inventory count differs'}
    foreach($member in $Patched.files){
        Assert-NativeArchiveName $member.path
        if(-not $expected.ContainsKey($member.path) -or $member.sha256 -cne $expected[$member.path]){throw 'Unreviewed PDS runtime delta'}
        $null=$expected.Remove($member.path)
    }
    if($expected.Count){throw 'Incomplete patched PDS runtime inventory'}
}
function Get-PackInventory([string]$Root){
    Assert-PackPath $Root
    $entries=@(Get-ChildItem -LiteralPath $Root -Recurse -Force)
    if($entries | Where-Object {$_.Attributes -band [IO.FileAttributes]::ReparsePoint}){throw 'Package inventory contains a reparse point'}
    return @($entries | Where-Object {-not $_.PSIsContainer} | Sort-Object FullName | ForEach-Object {
        [ordered]@{name=$_.FullName.Substring($Root.Length+1).Replace('\','/');sha256=(Get-PackHash $_.FullName)}
    })
}
function Get-PackPostgresInventory([string]$Runtime){
    # Keep the complete server/runtime trees, not optional archive applications
    # such as pgAdmin. Legal notices are selected separately from actual files.
    Assert-PackPath $Runtime
    $selected=[Collections.Generic.List[object]]::new()
    foreach($name in @('bin','lib','share')){
        $directory=Join-Path $Runtime ('pgsql/'+$name)
        Assert-PackPath $directory
        if(-not (Get-Item -LiteralPath $directory).PSIsContainer){throw 'Required PostgreSQL runtime root is not a directory'}
        $members=@(Get-PackInventory $directory)
        if($members.Count -eq 0){throw 'Required PostgreSQL runtime root is empty'}
        foreach($member in $members){
            $selected.Add([ordered]@{name=('pgsql/'+$name+'/'+$member.name);sha256=$member.sha256})
        }
    }
    return $selected.ToArray()
}
function Write-PackJson([string]$Path,$Value){[IO.File]::WriteAllText($Path,($Value | ConvertTo-Json -Depth 32),[Text.UTF8Encoding]::new($false))}
if($FunctionsOnly){return}
if($env:OS -cne 'Windows_NT' -or $env:COMPUTERNAME -cne 'MEMORY-WINLAB'){throw 'Runtime packaging is restricted to the disposable native Windows lab'}
if(-not $Kind -or $OperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)*$' -or $SourceRoot.Split('\') -contains '..'){
    throw 'Exact runtime kind, operation and copied lab source required'
}
if(($PdsProfilePath -and -not $PdsProfileSha256) -or ($PdsProfileSha256 -and -not $PdsProfilePath) -or
    ($Kind -cne 'Pds' -and ($PdsProfilePath -or $PdsProfileSha256))){throw 'Exact PDS profile path/digest pair applies only to PDS packaging'}
Assert-PackPath $SourceRoot
. (Join-Path $SourceRoot 'packaging/windows/native-instance-functions.ps1')
. (Join-Path $SourceRoot 'packaging/windows/native-prerequisites-functions.ps1')
Assert-PackHash $CrtNoticePath $CrtNoticeSha256
if((Get-Item -LiteralPath $CrtNoticePath).Length -lt 64){throw 'Retain the actual full CRT redistribution terms, not a placeholder'}
$pgRoot='C:\MemoryLab\NativePostgres'
$pgManifestPath=Join-Path $SourceRoot 'scripts/windows/native-postgres.json'
$pgManifest=Read-PackJson $pgManifestPath
$pgPreparedPath=Join-Path $pgRoot 'prepared.json';$pgPrepared=Read-PackJson $pgPreparedPath
if($pgManifest.format -cne 'memory-windows-postgres-proof-v1' -or $pgManifest.postgres_version -cne '17.11' -or
    $pgManifest.pgvector_version -cne '0.8.7' -or $pgPrepared.format -cne 'memory-windows-postgres-proof-result-v1' -or
    $pgManifest.postgres.sha256 -cne '6eabdf00d2893713b75db4336a23c3fdf505f056e217ec6e2e95d901750cfea3' -or
    $pgManifest.pgvector.sha256 -cne 'cac0b10c360f05b2d521200105ba3697e773d4cd3731f5a915a7e37ebe0bea85' -or
    $pgPrepared.manifest_sha256 -cne (Get-PackHash $pgManifestPath)){throw 'Pinned prepared PostgreSQL/CRT receipt required'}
$pgBin=Join-Path $pgRoot 'runtime/pgsql/bin'
$crtEntry=@($pgPrepared.runtime_support | Where-Object name -ceq 'vcruntime140.dll')
if($crtEntry.Count -ne 1){throw 'Recorded CRT version required'}
$crtVersion=$crtEntry[0].version
if($crtVersion -cnotmatch '^\d+(\.\d+){1,3}$'){throw 'Actual CRT file version is not a supported component version'}
foreach($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')){
    $record=@($pgPrepared.runtime_support | Where-Object name -ceq $name)
    if($record.Count -ne 1){throw 'Complete recorded app-local CRT required'}
    Assert-PackHash (Join-Path $pgBin $name) $record[0].sha256
}
$root='C:\MemoryLab\RuntimePackages'
Assert-PackPath (Split-Path -Parent $root)
if(-not(Test-Path -LiteralPath $root)){$null=New-Item -ItemType Directory -Path $root}
Assert-PackPath $root
$output=Join-Path $root ($OperationId.ToString()+'-'+$Kind.ToLowerInvariant())
if(Test-Path -LiteralPath $output){throw 'Use a fresh packaging operation; previous evidence/payloads are never replaced'}
$null=New-Item -ItemType Directory -Path $output
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $output /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if($LASTEXITCODE -ne 0){throw 'Unable to protect fresh runtime package output'}
$script:payload=Join-Path $output 'payload';$null=New-Item -ItemType Directory -Path $script:payload
$manifest=[ordered]@{schema_version=1;kind=('memory-native-'+$Kind.ToLowerInvariant()+'-runtime');platform='windows-x86_64';crt_version=$crtVersion
    package_sha256=('0'*64);payload_files=@();components=@()}
$proofInputs=[ordered]@{postgres_manifest_sha256=(Get-PackHash $pgManifestPath);postgres_prepared_sha256=(Get-PackHash $pgPreparedPath)
    crt_notice_path=$CrtNoticePath;crt_notice_sha256=$CrtNoticeSha256}
Copy-PackFile $CrtNoticePath 'notices/msvc-crt.txt' $CrtNoticeSha256
$crtComponent=New-PackComponent 'msvc-crt' $crtVersion (Get-PackHash (Join-Path $pgBin 'vcruntime140.dll')) 'Microsoft redistributable terms; see bundled notice' 'notices/msvc-crt.txt'
switch($Kind){
    'Postgres' {
        $manifest.postgres_version='17.11';$manifest.pgvector_version='0.8.7'
        $pgArchive=Join-Path $pgRoot ('downloads/'+$pgManifest.postgres.file)
        $vectorArchive=Join-Path $pgRoot ('downloads/'+$pgManifest.pgvector.file)
        Assert-PackHash $pgArchive $pgManifest.postgres.sha256;Assert-PackHash $vectorArchive $pgManifest.pgvector.sha256
        $proofInputs.postgres_archive=[ordered]@{path=$pgArchive;sha256=$pgManifest.postgres.sha256}
        $proofInputs.pgvector_archive=[ordered]@{path=$vectorArchive;sha256=$pgManifest.pgvector.sha256}
        Assert-PackPostgresRuntime $pgBin (Join-Path $pgRoot 'runtime/pgsql/lib') $pgPrepared
        $tree=Join-Path $pgRoot 'runtime';foreach($member in (Get-PackPostgresInventory $tree)){Copy-PackFile (Join-Path $tree $member.name) $member.name $member.sha256}
        if(-not $PostgresNoticePath){throw 'Supply the actual PostgreSQL bundled licence/legal notice path; no fabricated notice fallback'}
        Copy-PackFile $PostgresNoticePath 'notices/postgresql.txt' (Get-PackHash $PostgresNoticePath)
        $vectorNotice=Join-Path $pgRoot 'source/pgvector-0.8.7/LICENSE'
        Copy-PackFile $vectorNotice 'notices/pgvector.txt' (Get-PackHash $vectorNotice)
        $manifest.components=@((New-PackComponent 'postgresql' '17.11' $pgManifest.postgres.sha256 'PostgreSQL' 'notices/postgresql.txt'),
            (New-PackComponent 'pgvector' '0.8.7' $pgManifest.pgvector.sha256 'PostgreSQL' 'notices/pgvector.txt'),$crtComponent)
    }
    'Pds' {
        $pdsRoot='C:\MemoryLab\NativePds';$runtime=Join-Path $pdsRoot 'runtime';$proofPath=Join-Path $pdsRoot 'prepared.json'
        $proof=Read-PackJson $proofPath;$baselinePath=Join-Path $SourceRoot 'scripts/windows/native-pds.json';$baseline=Read-PackJson $baselinePath
        if($proof.format -cne 'memory-windows-pds-proof-result-v1' -or $proof.native_sqlite_verified -ne $true -or
            $baseline.format -cne 'memory-windows-pds-proof-v1' -or $baseline.node_version -cne '24.18.1' -or $baseline.pds_version -cne '0.5.27' -or
            $proof.manifest_sha256 -cne (Get-PackHash $baselinePath) -or $proof.source_revision -cne $baseline.source_revision){throw 'Actual pinned PDS preparation receipt required'}
        $originalProofSha=Get-PackHash $proofPath
        if($PdsProfilePath){
            Assert-PackHash $PdsProfilePath $PdsProfileSha256
            $selection=Read-PackJson $PdsProfilePath 1MB
            Assert-NativeRuntimeFields $selection @('format','prepared','sha256')
            if($selection.format -cne 'memory-native-pds-profile-v1' -or
                $selection.prepared -cnotmatch '^C:\\MemoryLab\\NativePds\\patched-[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\\prepared\.json$'){throw 'Exact fresh patched PDS profile required'}
            Assert-PackHash $selection.prepared $selection.sha256
            $patchedProof=Read-PackJson $selection.prepared
            Assert-PackPdsPatchDelta $proof $patchedProof $originalProofSha
            $proof=$patchedProof;$proofPath=$selection.prepared;$runtime=Join-Path (Split-Path -Parent $proofPath) 'runtime'
            $proofInputs.pds_profile_sha256=$PdsProfileSha256
            $manifest.original_prepared_sha256=$originalProofSha;$manifest.path_patch=$proof.path_patch
            $patchFile=Join-Path $SourceRoot 'packaging/windows-pds/windows-paths.mjs'
            Copy-PackFile $patchFile 'pds/windows-paths.mjs' $proof.path_patch.patch_sha256
            Copy-PackFile (Join-Path $SourceRoot 'LICENSE') 'notices/memory-pds-path-patch.txt' (Get-PackHash (Join-Path $SourceRoot 'LICENSE'))
        }
        $proofInputs.pds_prepared_sha256=Get-PackHash $proofPath
        $proofInputs.pds_manifest_sha256=Get-PackHash $baselinePath
        $nodeRoot='C:\MemoryLab\Tools\node-v24.18.1-win-x64'
        Copy-PackFile (Join-Path $nodeRoot 'node.exe') 'node/node.exe' $proof.node_sha256
        foreach($member in $proof.files){
            Assert-NativeArchiveName $member.path
            $source=Join-Path $runtime $member.path
            # Verification covers all prepared files; only production paths enter
            # a runtime bundle. The two disposable proof probes remain outside.
            if($member.path -cin @('probe-native-sqlite.cjs','probe-stdio.mjs')){Assert-PackHash $source $member.sha256;continue}
            if($member.path -cnotmatch '^(?:package\.json|pnpm-lock\.yaml|pds-server\.mjs|http-stdio\.mjs|node_modules/.+)$'){throw 'Unexpected prepared PDS root member; review before selecting'}
            Copy-PackFile $source ('pds/'+$member.path) $member.sha256
        }
        foreach($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')){Copy-PackFile (Join-Path $pgBin $name) ('node/'+$name) (Get-PackHash (Join-Path $pgBin $name))}
        Copy-PackFile (Join-Path $nodeRoot 'LICENSE') 'notices/node.txt' (Get-PackHash (Join-Path $nodeRoot 'LICENSE'))
        $pdsNotice=Join-Path $runtime 'node_modules/@atproto/pds/LICENSE'
        Copy-PackFile $pdsNotice 'notices/pds.txt' (Get-PackHash $pdsNotice)
        Copy-PackFile (Join-Path $SourceRoot 'LICENSE') 'notices/memory-pds-adapter.txt' (Get-PackHash (Join-Path $SourceRoot 'LICENSE'))
        # All full dependency notices remain at their original node_modules paths.
        # This index binds their exact bytes instead of substituting licence names.
        $notices=@($proof.files | Where-Object {$_.path -cmatch '^node_modules/' -and [IO.Path]::GetFileName($_.path) -match '^(?:LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)(?:[.-].*)?$'})
        if($notices.Count -lt 1){throw 'Complete dependency licence files missing'}
        $index=($notices | Sort-Object path | ForEach-Object { $_.sha256+'  pds/'+$_.path }) -join "`n"
        [IO.File]::WriteAllText((Join-Path $script:payload 'notices/locked-dependencies.txt'),"Full dependency notices retained verbatim at these payload paths:`n$index`n",[Text.UTF8Encoding]::new($false))
        $manifest.node_version='24.18.1';$manifest.pds_version='0.5.27';$manifest.pds_source_revision=$baseline.source_revision
        $manifest.pds_image_digest='sha256:d95725b24dbe53af9d91dc69750556931ebed6c396f2cfa42b221434db642f12'
        $manifest.pds_package_sha256=$baseline.package_sha256;$manifest.pds_lock_sha256=$baseline.lock_sha256;$manifest.pnpm_version=$baseline.pnpm.version
        # These are the reviewed build baseline, not a claim that packaging ran
        # a provider/database probe. The preparation receipt records its native
        # SQLite probe; final runtime execution remains separately unverified.
        $manifest.better_sqlite3_version='12.11.1';$manifest.sqlite_version='3.53.2'
        $manifest.native_addons=@($proof.files | Where-Object {$_.path -cmatch '^node_modules/.+\.node$'} | ForEach-Object {
            [ordered]@{name=('pds/'+$_.path);sha256=$_.sha256}
        })
        $manifest.components=@((New-PackComponent 'node' '24.18.1' $proof.node_sha256 'See bundled Node licence' 'notices/node.txt'),
            (New-PackComponent 'pds' '0.5.27' $manifest.pds_image_digest.Substring(7) 'MIT' 'notices/pds.txt'),
            (New-PackComponent 'locked-dependencies' $baseline.lock_sha256 $baseline.lock_sha256 'Multiple; complete licence files retained in dependency tree' 'notices/locked-dependencies.txt'),
            (New-PackComponent 'memory-pds-adapter' 'stdio-v1' (Get-PackHash (Join-Path $runtime 'pds-server.mjs')) 'AGPL-3.0-or-later' 'notices/memory-pds-adapter.txt'),$crtComponent)
        if($PdsProfilePath){$manifest.components+=New-PackComponent 'memory-pds-path-patch' 'windows-sha256-did-paths-v1' $proof.path_patch.patch_sha256 'AGPL-3.0-or-later' 'notices/memory-pds-path-patch.txt'}
    }
    'Pi' {
        if($PiBuildOperationId -eq [guid]::Empty){throw 'Exact retained Pi build operation required'}
        $build=Join-Path 'C:\MemoryLab\NativePi' ('build-'+$PiBuildOperationId.ToString())
        $evidencePath=Join-Path $build 'build-evidence.json';$evidence=Read-PackJson $evidencePath
        $prepared=Read-PackJson (Join-Path $build 'prepared.json');$baselinePath=Join-Path $SourceRoot 'scripts/windows/native-pi.json';$baseline=Read-PackJson $baselinePath
        if($evidence.schema_version -ne 1 -or $prepared.format -cne 'memory-windows-pi-prepared-v1' -or
            $prepared.manifest_sha256 -cne (Get-PackHash $baselinePath) -or $prepared.binary_sha256 -cne $evidence.artifact.binary_sha256 -or
            $evidence.rustc -cnotmatch ('(?m)^commit-hash: '+[regex]::Escape($baseline.rustc_commit)+'\r?$')){throw 'Recorded successful pinned Pi build required'}
        $proofInputs.pi_evidence_sha256=Get-PackHash $evidencePath
        $proofInputs.pi_manifest_sha256=Get-PackHash $baselinePath
        Copy-PackFile (Join-Path $build 'target/release/pi.exe') 'pi/pi.exe' $evidence.artifact.binary_sha256
        foreach($name in @('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll')){Copy-PackFile (Join-Path $pgBin $name) ('pi/'+$name) (Get-PackHash (Join-Path $pgBin $name))}
        Copy-PackFile (Join-Path $build 'source.tar') 'source/pi-source.tar' $baseline.source_archive_sha256
        foreach($name in @('no-tools-1.patch','windows-broker-1.patch','windows-broker-safe-handle-1.patch')){
            $source=Join-Path $SourceRoot ('crates/mem-supervisor-pi/patches/'+$name);Copy-PackFile $source ('source/'+$name) (Get-PackHash $source)
        }
        foreach($name in @('Cargo.toml','src/lib.rs')){
            $source=Join-Path $SourceRoot ('crates/mem-supervisor-pi/windows-pipe-helper/'+$name);Copy-PackFile $source ('source/windows-pipe-helper/'+$name) (Get-PackHash $source)
        }
        Copy-PackFile (Join-Path $build 'source/Cargo.lock') 'source/Cargo.lock' $baseline.patched_lock_sha256
        $piLicense=Join-Path $SourceRoot 'crates/mem-supervisor-pi/patches/LICENSE.pi'
        Copy-PackFile $piLicense 'notices/pi.txt' (Get-PackHash $piLicense)
        Copy-PackFile (Join-Path $SourceRoot 'LICENSE') 'notices/memory-windows-pipe.txt' (Get-PackHash (Join-Path $SourceRoot 'LICENSE'))
        $manifest.pi_version='0.5.1';$manifest.artifact=$evidence.artifact;$manifest.source_archive_sha256=$baseline.source_archive_sha256
        $manifest.upstream_lock_sha256=$baseline.upstream_lock_sha256;$manifest.rust_toolchain=$baseline.rust_toolchain;$manifest.rustc_commit=$baseline.rustc_commit;$manifest.target=$baseline.target
        $manifest.components=@((New-PackComponent 'pi' '0.5.1' $baseline.source_archive_sha256 'MIT-with-OpenAI-Anthropic-Rider' 'notices/pi.txt'),
            (New-PackComponent 'memory-windows-pipe' '0.1.0' $evidence.artifact.helper_source_sha256 'AGPL-3.0-or-later' 'notices/memory-windows-pipe.txt'),$crtComponent)
    }
}
$manifest.payload_files=Get-PackInventory $script:payload
# Validate exact schema/provenance before archiving. The temporary all-zero ZIP
# digest is only a shape-valid placeholder; no manifest/receipt is written yet.
$null=Get-NativePrerequisitePlan ([pscustomobject]$manifest)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip=Join-Path $output 'runtime.zip'
$fileLimit=if($manifest.kind -ceq 'memory-native-pds-runtime'){65536}else{20000}
New-NativeCanonicalArchive $script:payload $zip $manifest.payload_files -FileLimit $fileLimit
$manifest.package_sha256=Get-PackHash $zip
$null=Get-NativePrerequisitePlan ([pscustomobject]$manifest)
$manifestPath=Join-Path $output 'runtime.json';Write-PackJson $manifestPath $manifest
Assert-NativeRuntimeManifestSize (Get-Item -LiteralPath $manifestPath).Length $manifest.kind
$result=[ordered]@{format='memory-native-runtime-package-v1';operation_id=$OperationId.ToString();kind=$manifest.kind
    manifest=$manifestPath;manifest_sha256=(Get-PackHash $manifestPath);package=$zip;package_sha256=$manifest.package_sha256
    source_inputs=$proofInputs;payload_count=$manifest.payload_files.Count;native_execution_verified=$false;installed=$false;service_created=$false}
Write-PackJson (Join-Path $output 'result.json') $result
$result | ConvertTo-Json -Depth 8
