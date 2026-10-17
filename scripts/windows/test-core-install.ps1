# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure portable checks: never starts a guest, SCM service or database.
$ErrorActionPreference='Stop'
$scriptPath=Join-Path $PSScriptRoot 'guest-core-install.ps1'
$tokens=$null;$errors=$null
$null=[Management.Automation.Language.Parser]::ParseFile($scriptPath,[ref]$tokens,[ref]$errors)
if($errors.Count){throw ($errors | Out-String)}
. $scriptPath -FunctionsOnly
function Reject-CoreInstall([scriptblock]$Action){$failed=$false;try{$null=&$Action}catch{$failed=$true};if(-not $failed){throw 'Unsafe core installation input accepted'}}
$operation=[guid]'399bb47c-2931-46e5-967f-e5c88625eb26';$instance=[guid]'a4444444-4444-4444-8444-444444444444'
$config=@'
[area]
name = 'workshop'
instance_id = 'a4444444-4444-4444-8444-444444444444'
[service]
bind_addr = '127.0.0.1:54321'
api_token = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
web_root = 'C:\MemoryLab\source\native-stack-v6\web\dist'
[database]
url = 'postgresql://memorylab_test:fake%21password@127.0.0.1:55471/lab_core_399bb47c293146e5967fe5c88625eb26'
[features]
llm_curation = false
[embeddings]
enabled = false
create_enabled = false
[background_work]
enabled = false
[cluster]
enabled = false
[automation]
enabled = false
[offline]
enabled = false
[reinforcement]
enabled = false
[provenance]
reverify_enabled = false
[consolidation]
enabled = false
[procedural]
enabled = false
[mcp]
enabled = false
http_enabled = false
[telemetry]
enabled = false
'@
$binding=Get-CoreInstallBinding $config $operation $instance
if($binding.port -ne 54321 -or $binding.password -cne 'fake!password'){throw 'Exact private binding was not preserved'}
$crlf=$config.Replace("`r`n","`n").Replace("`n","`r`n")
$null=Get-CoreInstallBinding $crlf $operation $instance
Reject-CoreInstall {Get-CoreInstallBinding $config ([guid]::NewGuid()) $instance}
Reject-CoreInstall {Get-CoreInstallBinding $config $operation ([guid]::NewGuid())}
Reject-CoreInstall {Get-CoreInstallBinding ($config.Replace('127.0.0.1:54321','0.0.0.0:54321')) $operation $instance}
Reject-CoreInstall {Get-CoreInstallBinding ($config.Replace('llm_curation = false','llm_curation = true')) $operation $instance}
Reject-CoreInstall {Get-CoreInstallBinding ($config+"`n[provider]`napi_key = 'fake'") $operation $instance}
Reject-CoreInstall {Get-CoreInstallBinding ($config+"`nweb_root = 'extra'") $operation $instance}
$web='C:\Program Files\Memory Layer\releases\windows-v2.1.0-rc.5-build.60000\web'
$changed=Set-CoreInstalledWeb $config $web
$null=Get-CoreInstallBinding $changed $operation $instance
if(-not $changed.Contains("web_root = '"+$web+"'")){throw 'Installed Web path was not used literally'}
if((Set-CoreInstalledWeb $changed 'C:\MemoryLab\source\native-stack-v6\web\dist') -cne $config){throw 'Config fields beyond Web path changed'}
Reject-CoreInstall {Set-CoreInstalledWeb $config "bad'path"}
$root=Join-Path ([IO.Path]::GetTempPath()) ('memory-core-install-test-'+[guid]::NewGuid())
$null=New-Item -ItemType Directory -Path $root
try{
    $source=Join-Path $root 'source.bin';$copy=Join-Path $root 'copy.bin'
    Write-CoreInstallNew $source ([Text.UTF8Encoding]::new($false).GetBytes('pinned lab bytes'))
    $hash=Get-CoreInstallHash $source;Copy-CoreInstallFile $source $copy $hash
    if((Get-CoreInstallHash $copy) -cne $hash){throw 'Pinned copy changed'}
    Reject-CoreInstall {Copy-CoreInstallFile $source $copy $hash}
    Reject-CoreInstall {Copy-CoreInstallFile $source (Join-Path $root 'wrong.bin') ('0'*64)}
    if(Test-Path (Join-Path $root 'wrong.bin')){throw 'Changed source was copied'}
    Reject-CoreInstall {Write-CoreInstallNew $source ([byte[]]@(0))}
    $core=Join-Path $root 'completed-core';$debug=Join-Path $root 'mutable-cache';$payload=Join-Path $root 'payload'
    $null=New-Item -ItemType Directory -Path (Join-Path $core 'code'),$debug,$payload
    $privateMemory=Join-Path $core 'code/memory.exe';$cachedMemory=Join-Path $debug 'memory.exe'
    $cachedWrapper=Join-Path $debug 'memory-windows-service.exe'
    Write-CoreInstallNew $privateMemory ([Text.Encoding]::UTF8.GetBytes('original pinned Memory'))
    Write-CoreInstallNew $cachedMemory ([Text.Encoding]::UTF8.GetBytes('cargo test changed Memory'))
    Write-CoreInstallNew $cachedWrapper ([Text.Encoding]::UTF8.GetBytes('original pinned wrapper'))
    $memoryPin=Get-CoreInstallHash $privateMemory;$wrapperPin=Get-CoreInstallHash $cachedWrapper
    $selected=Get-CoreInstallExecutableSource 'memory.exe' $core $debug
    Copy-CoreInstallFile $selected (Join-Path $payload 'memory.exe') $memoryPin 'memory.exe'
    if((Get-CoreInstallHash (Join-Path $payload 'memory.exe')) -cne $memoryPin){throw 'Mutable cargo binary replaced completed core proof'}
    $selected=Get-CoreInstallExecutableSource 'memory-windows-service.exe' $core $debug
    Copy-CoreInstallFile $selected (Join-Path $payload 'memory-windows-service.exe') $wrapperPin 'memory-windows-service.exe'
    [IO.File]::AppendAllText($cachedWrapper,'changed')
    $failure=$null
    try{Copy-CoreInstallFile $selected (Join-Path $payload 'changed-wrapper.exe') $wrapperPin 'memory-windows-service.exe'}catch{$failure=$_.Exception.Message}
    if($failure -cne 'Pinned core installation input unavailable or changed: memory-windows-service.exe'){throw 'Component mismatch was not opaque and exact'}
    if(Test-Path (Join-Path $payload 'changed-wrapper.exe')){throw 'Changed wrapper was copied'}
}finally{Remove-Item -LiteralPath $root -Recurse -Force}
Write-Output 'Core installation parser and pure helpers passed; SCM/native execution was not attempted.'
