# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable helpers only. No guest, native dependency execution or installation.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'guest-runtime-pack.ps1') -FunctionsOnly
. (Join-Path $PSScriptRoot '../../packaging/windows/native-instance-functions.ps1')
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
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
$text=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'guest-runtime-pack.ps1') -Raw
foreach($forbidden in @('Invoke-WebRequest','Start-Service','New-Service','initdb.exe','npm install','pnpm install')){
    if($text.Contains($forbidden)){throw 'Runtime packer acquired activation/download behavior'}
}
Write-Output 'Runtime packaging hash/copy/inventory/notice/receipt helper checks passed (portable only; no native bundle produced).'
