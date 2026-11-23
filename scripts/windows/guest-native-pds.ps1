# SPDX-License-Identifier: AGPL-3.0-or-later
# Disposable dependency proof only; never provisions an account or live PDS.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$SourceRoot
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'Native PDS proof is restricted to the disposable Memory VM'
}
$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.format -ne 'memory-windows-pds-proof-v1' -or
    $manifest.node_version -ne '24.18.1' -or $manifest.pds_version -ne '0.5.27' -or
    $manifest.source_revision -ne '374cf1d4ba782d4391bbb73e4e2d3f320d4846d6' -or
    $manifest.pnpm.version -ne '10.34.1') { throw 'Unreviewed native PDS baseline' }
function Run-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Native PDS proof command failed: $([IO.Path]::GetFileName($Exe)) ($LASTEXITCODE)" }
}
function Assert-Hash([string]$Path, [string]$Expected, [string]$Algorithm='SHA256') {
    if ((Get-FileHash -LiteralPath $Path -Algorithm $Algorithm).Hash.ToLowerInvariant() -ne $Expected) {
        throw 'Native PDS proof input changed; preserved for inspection'
    }
}
$root = 'C:\MemoryLab\NativePds'
$tools = 'C:\MemoryLab\Tools'
$runtime = Join-Path $root 'runtime'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
New-Item -ItemType Directory -Force $root | Out-Null
Run-Checked 'icacls.exe' @($root,'/inheritance:r','/grant:r',"*${sid}:(OI)(CI)F",'*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F')
$upstream = Join-Path $SourceRoot 'packaging\windows-pds\upstream'
Assert-Hash (Join-Path $upstream 'package.json') $manifest.package_sha256
Assert-Hash (Join-Path $upstream 'pnpm-lock.yaml') $manifest.lock_sha256
foreach ($patch in $manifest.security_patches.PSObject.Properties) {
    if ($patch.Name -cnotmatch '^[A-Za-z0-9_@.+-]+\.patch$') { throw 'Invalid reviewed PDS patch name' }
    Assert-Hash (Join-Path $upstream "patches\$($patch.Name)") $patch.Value
}
$node = Join-Path $tools 'node-v24.18.1-win-x64\node.exe'
if ((& $node --version) -ne 'v24.18.1') { throw 'Pinned native Node is required' }
$python = Join-Path $tools 'python-3.13.9\python.exe'
Run-Checked $python @('--version')
$pnpmArchive = Join-Path $root 'pnpm-10.34.1.tgz'
if (-not (Test-Path $pnpmArchive)) {
    if ($manifest.pnpm.url -ne 'https://registry.npmjs.org/pnpm/-/pnpm-10.34.1.tgz') { throw 'Unreviewed pnpm origin' }
    Invoke-WebRequest -UseBasicParsing -Uri $manifest.pnpm.url -OutFile ($pnpmArchive + '.partial')
    Move-Item -LiteralPath ($pnpmArchive + '.partial') -Destination $pnpmArchive
}
Assert-Hash $pnpmArchive $manifest.pnpm.sha512 'SHA512'
$pnpmRoot = Join-Path $root 'pnpm'
New-Item -ItemType Directory -Force $pnpmRoot,$runtime | Out-Null
Run-Checked "$env:SystemRoot\System32\tar.exe" @('-xzf',$pnpmArchive,'-C',$pnpmRoot)
$pnpm = Join-Path $pnpmRoot 'package\bin\pnpm.cjs'
foreach ($name in @('package.json','pnpm-lock.yaml')) {
    $target = Join-Path $runtime $name
    if (Test-Path $target) {
        if ((Get-FileHash $target).Hash -ne (Get-FileHash (Join-Path $upstream $name)).Hash) {
            throw 'Interrupted PDS preparation has different immutable inputs'
        }
    } else { Copy-Item -LiteralPath (Join-Path $upstream $name) -Destination $target }
}
$patchRoot = Join-Path $runtime 'patches'
New-Item -ItemType Directory -Force $patchRoot | Out-Null
foreach ($patch in $manifest.security_patches.PSObject.Properties) {
    $target = Join-Path $patchRoot $patch.Name
    if (Test-Path $target) {
        Assert-Hash $target $patch.Value
    } else {
        Copy-Item -LiteralPath (Join-Path $upstream "patches\$($patch.Name)") -Destination $target
    }
}
$env:PATH = "$(Split-Path $node);$(Split-Path $python);$env:PATH"
$env:npm_config_python = $python
$env:npm_config_build_from_source = 'true'
$env:CI = 'true'
Copy-Item -LiteralPath (Join-Path $SourceRoot 'packaging\windows-pds\probe-native-sqlite.cjs') -Destination (Join-Path $runtime 'probe-native-sqlite.cjs') -Force
Push-Location $runtime
try {
    Run-Checked $node @($pnpm,'install','--prod','--frozen-lockfile','--ignore-scripts','--config.node-linker=hoisted')
    # Upstream explicitly allows this one native lifecycle build, never arbitrary
    # repository setup hooks. Node-gyp uses the prepared MSVC/Python toolchain.
    Run-Checked $node @($pnpm,'rebuild','better-sqlite3','--config.node-linker=hoisted')
    Run-Checked $node @((Join-Path $runtime 'probe-native-sqlite.cjs'))
} finally { Pop-Location }
Assert-Hash (Join-Path $runtime 'package.json') $manifest.package_sha256
Assert-Hash (Join-Path $runtime 'pnpm-lock.yaml') $manifest.lock_sha256
foreach ($patch in $manifest.security_patches.PSObject.Properties) {
    Assert-Hash (Join-Path $patchRoot $patch.Name) $patch.Value
}
foreach ($name in @('pds-server.mjs','http-stdio.mjs','probe-stdio.mjs')) {
    Copy-Item -LiteralPath (Join-Path $SourceRoot "packaging\windows-pds\$name") -Destination (Join-Path $runtime $name) -Force
}
# A frozen lock does not remove every obsolete file from an interrupted/reused
# hoisted tree. Test the actual installed dependency resolution before issuing a
# receipt; stale elliptic or missing API patches must require a fresh staging
# directory, never silently pass because the lock alone audits clean.
$previousTestRoot = $env:CODIFIX_PDS_TEST_ROOT
$previousLogEnabled = $env:LOG_ENABLED
try {
    $env:CODIFIX_PDS_TEST_ROOT = $runtime
    $env:LOG_ENABLED = 'false'
    Run-Checked $node @('--test',(Join-Path $SourceRoot 'packaging\windows-pds\security.test.mjs'))
} finally {
    $env:CODIFIX_PDS_TEST_ROOT = $previousTestRoot
    $env:LOG_ENABLED = $previousLogEnabled
}
$entries = @(Get-ChildItem -LiteralPath $runtime -Recurse -Force)
if ($entries | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) {
    throw 'Native runtime contains a reparse point; no deployment claim'
}
$files = @($entries | Where-Object { -not $_.PSIsContainer } | Sort-Object FullName | ForEach-Object {
    @{path=$_.FullName.Substring($runtime.Length+1).Replace('\','/');sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant();bytes=$_.Length}
})
$receipt = [ordered]@{
    format='memory-windows-pds-proof-result-v1'; manifest_sha256=(Get-FileHash $ManifestPath).Hash.ToLowerInvariant()
    native_sqlite_verified=$true; pds_protocol_verified=$false; source_revision=$manifest.source_revision
    node_sha256=(Get-FileHash $node).Hash.ToLowerInvariant(); files=$files
}
$receipt | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $root 'prepared.json') -Encoding UTF8
@{native_sqlite_verified=$true;pds_protocol_verified=$false;files=$files.Count} | ConvertTo-Json
