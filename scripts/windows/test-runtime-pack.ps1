# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable helpers only. No guest, native dependency execution or installation.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'guest-runtime-pack.ps1') -FunctionsOnly
. (Join-Path $PSScriptRoot '../../packaging/windows/native-instance-functions.ps1')
. (Join-Path $PSScriptRoot '../../packaging/windows/native-prerequisites-functions.ps1')
function Reject-Pack([scriptblock]$Action){$failed=$false;try{&$Action}catch{$failed=$true};if(-not $failed){throw 'Unsafe packaging input accepted'}}
$root=Join-Path ([IO.Path]::GetTempPath()) ('memory-runtime-pack-test-'+[guid]::NewGuid())
$null=New-Item -ItemType Directory -Path $root
try{
    $script:payload=Join-Path $root 'payload';$null=New-Item -ItemType Directory -Path $script:payload
    $source=Join-Path $root 'actual-license.txt';[IO.File]::WriteAllText($source,"Actual synthetic test bytes`n",[Text.UTF8Encoding]::new($false))
    $hash=Get-PackHash $source
    Copy-PackFile $source 'notices/component.txt' $hash
    if([IO.File]::ReadAllText((Join-Path $script:payload 'notices/component.txt')) -cne [IO.File]::ReadAllText($source)){throw 'Notice bytes changed'}
    Reject-Pack {Copy-PackFile $source 'notices/component.txt' $hash}
    Reject-Pack {Copy-PackFile $source '../escape.txt' $hash}
    Reject-Pack {Copy-PackFile $source 'notices/changed.txt' ('0'*64)}
    if(Test-Path (Join-Path $script:payload 'notices/changed.txt')){throw 'Changed source was copied before pin validation'}
    $inventory=@(Get-PackInventory $script:payload)
    if($inventory.Count -ne 1 -or $inventory[0].name -cne 'notices/component.txt' -or $inventory[0].sha256 -cne $hash){throw 'Incomplete payload digest inventory'}
    $zip=Join-Path $root 'runtime.zip';New-NativeCanonicalArchive $script:payload $zip $inventory -FileLimit 65536
    $zipHash=Get-PackHash $zip
    $stream=[IO.File]::OpenRead($zip);$archive=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Read)
    try{
        if($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -cne 'notices/component.txt'){throw 'Runtime ZIP did not use canonical slash entries'}
        Assert-NativeArchiveName $archive.Entries[0].FullName
        $inputStream=$archive.Entries[0].Open();$sha=[Security.Cryptography.SHA256]::Create()
        try{$entryHash=([BitConverter]::ToString($sha.ComputeHash($inputStream))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose();$inputStream.Dispose()}
        if($entryHash -cne $hash){throw 'Runtime ZIP member bytes changed'}
    }finally{$archive.Dispose();$stream.Dispose()}
    Reject-Pack {New-NativeCanonicalArchive $script:payload $zip $inventory}
    if((Get-PackHash $zip) -cne $zipHash){throw 'Existing runtime package evidence was overwritten'}
    Reject-Pack {New-NativeCanonicalArchive $script:payload (Join-Path $root 'over-limit.zip') @($inventory[0],$inventory[0]) -FileLimit 1}
    Reject-Pack {New-NativeCanonicalArchive $script:payload (Join-Path $root 'changed.zip') @(@{name='notices/component.txt';sha256=('0'*64)})}
    Write-PackJson (Join-Path $root 'receipt.json') ([ordered]@{source=$hash;installed=$false})
    $receipt=Read-PackJson (Join-Path $root 'receipt.json')
    if($receipt.source -cne $hash -or $receipt.installed -ne $false){throw 'Packaging receipt changed'}
    Reject-Pack {Read-PackJson (Join-Path $root 'receipt.json') 1}
    $component=New-PackComponent 'example' '1.0' $hash 'Actual terms in notice' 'notices/component.txt'
    if($component.source_sha256 -cne $hash -or $component.notice_path -cne 'notices/component.txt'){throw 'Component lost provenance'}
    $bin=Join-Path $root 'bin';$lib=Join-Path $root 'lib';$null=New-Item -ItemType Directory -Path $bin,$lib
    $dlls=@('vcruntime140.dll','vcruntime140_1.dll','msvcp140.dll','libssl-3-x64.dll')
    $extensions=@('pg_trgm.dll','pgcrypto.dll','vector.dll')
    foreach($name in $dlls+@('postgres.exe')){[IO.File]::WriteAllText((Join-Path $bin $name),$name)}
    foreach($name in $extensions){[IO.File]::WriteAllText((Join-Path $lib $name),$name)}
    $prepared=[pscustomobject]@{runtime_support=@($dlls | ForEach-Object {[pscustomobject]@{name=$_;sha256=(Get-PackHash (Join-Path $bin $_))}})
        extensions=@($extensions | ForEach-Object {[pscustomobject]@{name=$_;sha256=(Get-PackHash (Join-Path $lib $_))}})
        postgres_sha256=(Get-PackHash (Join-Path $bin 'postgres.exe'))}
    Assert-PackPostgresRuntime $bin $lib $prepared
    [IO.File]::WriteAllText((Join-Path $bin 'libssl-3-x64.dll'),'altered non-CRT DLL')
    Reject-Pack {Assert-PackPostgresRuntime $bin $lib $prepared}
    [IO.File]::WriteAllText((Join-Path $bin 'libssl-3-x64.dll'),'libssl-3-x64.dll')
    $prepared.extensions=$prepared.extensions[0..1]
    Reject-Pack {Assert-PackPostgresRuntime $bin $lib $prepared}
    $pgTree=Join-Path $root 'postgres-runtime'
    foreach($directory in @('bin','lib','share','pgAdmin 4')){
        $path=Join-Path $pgTree ('pgsql/'+$directory)
        $null=New-Item -ItemType Directory -Path $path -Force
        [IO.File]::WriteAllText((Join-Path $path 'preserved.dat'),$directory)
    }
    $selected=@(Get-PackPostgresInventory $pgTree)
    $expected=@('pgsql/bin/preserved.dat','pgsql/lib/preserved.dat','pgsql/share/preserved.dat')
    if($selected.Count -ne 3 -or @(Compare-Object $expected @($selected | ForEach-Object name) -CaseSensitive).Count){throw 'PostgreSQL runtime roots incomplete or optional applications included'}
    foreach($member in $selected){Assert-PackHash (Join-Path $pgTree $member.name) $member.sha256}
    # Missing and empty required roots fail rather than producing a thin,
    # apparently valid package without extension support or server share files.
    Remove-Item -LiteralPath (Join-Path $pgTree 'pgsql/share/preserved.dat')
    Reject-Pack {Get-PackPostgresInventory $pgTree}
    Remove-Item -LiteralPath (Join-Path $pgTree 'pgsql/share')
    Reject-Pack {Get-PackPostgresInventory $pgTree}
    $patch=[pscustomobject]@{layout='windows-sha256-did-paths-v1';patch_sha256='4ba316f92a401d8a9854fe96595232bb845dfe14fa33aefe51837aacc5efcef0';files=@(
        [pscustomobject]@{name='actor-store/actor-store.js';original_sha256='33ba4bcf0b0afb06868b80a4cb0025388a2a7e8006338ce80a70785d2079af4f';patched_sha256='c192431cb2de642bbe5c8256fbe30f0de6fe5dc23f3b20b8baec71e969909965'},
        [pscustomobject]@{name='disk-blobstore.js';original_sha256='cc18ec422b23ba3bc06baea321146e512ce9cb14608ec90ccfe5e9bc71359811';patched_sha256='9da2aa49a53b58826ae1b24fd7ba8d4fbbe7c9aaf9c7ee8a24c91c792b736f5a'})}
    $original=[pscustomobject]@{manifest_sha256=$hash;node_sha256=$hash;source_revision='pinned';native_sqlite_verified=$true
        files=@([pscustomobject]@{path='package.json';sha256=$hash})+@($patch.files | ForEach-Object {[pscustomobject]@{path=('node_modules/@atproto/pds/dist/'+$_.name);sha256=$_.original_sha256}})}
    $patched=[pscustomobject]@{format='memory-windows-pds-patched-proof-v1';original_prepared_sha256=$hash;manifest_sha256=$hash;node_sha256=$hash;source_revision='pinned';native_sqlite_verified=$true;path_patch=$patch
        files=@([pscustomobject]@{path='package.json';sha256=$hash})+@($patch.files | ForEach-Object {[pscustomobject]@{path=('node_modules/@atproto/pds/dist/'+$_.name);sha256=$_.patched_sha256}})}
    Assert-PackPdsPatchDelta $original $patched $hash
    foreach($change in @({param($m)$m.files[0].sha256=('b'*64)},
        {param($m)$m.files+=$m.files[0]},
        {param($m)$m.original_prepared_sha256=('b'*64)},
        {param($m)$m.node_sha256=('b'*64)},
        {param($m)$m.path_patch.patch_sha256=('b'*64)})){
        $copy=$patched | ConvertTo-Json -Depth 10 | ConvertFrom-Json;&$change $copy
        Reject-Pack {Assert-PackPdsPatchDelta $original $copy $hash}
    }
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
$text=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'guest-runtime-pack.ps1') -Raw
foreach($forbidden in @('Invoke-WebRequest','Start-Service','New-Service','initdb.exe','npm install','pnpm install')){
    if($text.Contains($forbidden)){throw 'Runtime packer acquired activation/download behavior'}
}
Write-Output 'Runtime packaging hash/copy/inventory/notice/receipt helper checks passed (portable only; no native bundle produced).'
