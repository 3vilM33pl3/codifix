# Pinned native build only. No provider execution, installation or publication.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$SourceRepository,
    [Parameter(Mandatory=$true)][string]$BuildRoot,
    [switch]$InstallToolchain
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Revision = '7c018475d54d4e09c907691e47a520f1f09e3288'
$Nightly = 'nightly-2026-08-31'
$PatchRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../patches'))
$NoTools = Join-Path $PatchRoot 'no-tools-1.patch'
$Broker = Join-Path $PatchRoot 'windows-broker-1.patch'
$Portability = Join-Path $PatchRoot 'windows-broker-safe-handle-1.patch'
$Helper = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../windows-pipe-helper'))
$NoToolsHash = 'bec724b3d1cef1df4f4cddc50573ce6af1724c743496e7f5e900f4e4a6c815bc'
$BrokerHash = '61a620316d545b100cf17a83503aba4da46094346c7996c502508b9060aebdf4'
$PortabilityHash = '241551af0ec017a63f31cf2aba93122f961998f7380a60abe162504d78cd70c6'
$HelperManifestHash = '89c7d41c710c69122099b353177271a493ccfa5997918b5b8c62268714db0371'
$HelperSourceHash = '3fd55c35ffb6418a8200ea221f96b9300f8bea90d9b9e1cab2f3ce68f9d859c3'
$PatchedLockHash = '362e708ba62267ca061a7e8172f663a4994b78990141d383879198132c74dfd6'
foreach ($Name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY')) {
    if ([Environment]::GetEnvironmentVariable($Name)) { throw 'Build in a credential-free environment' }
}
function Run([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed ($LASTEXITCODE)" }
}
function Hash([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
if ((Hash $NoTools) -ne $NoToolsHash -or (Hash $Broker) -ne $BrokerHash) { throw 'Reviewed patch bytes changed' }
if ((Hash $Portability) -ne $PortabilityHash -or (Hash (Join-Path $Helper 'Cargo.toml')) -ne $HelperManifestHash -or
    (Hash (Join-Path $Helper 'src/lib.rs')) -ne $HelperSourceHash) { throw 'Reviewed ownership helper or portability patch changed' }
$BuildRoot = [IO.Path]::GetFullPath($BuildRoot)
if (Test-Path -LiteralPath $BuildRoot) { throw 'Use a fresh build directory; existing source and evidence are never overwritten' }
New-Item -ItemType Directory -Path $BuildRoot | Out-Null
$Source = Join-Path $BuildRoot 'source'
New-Item -ItemType Directory -Path $Source | Out-Null
$Archive = Join-Path $BuildRoot 'source.tar'
Run 'git' @('-C',$SourceRepository,'cat-file','-e',"${Revision}^{commit}")
Run 'git' @('-C',$SourceRepository,'archive','--format=tar',"--output=$Archive",$Revision)
Run 'tar.exe' @('-xf',$Archive,'-C',$Source)
$BeforeLock = Hash (Join-Path $Source 'Cargo.lock')
if ($BeforeLock -ne '1aea4e3fe13a86760317d9c3da80d7461bd5cd5c013508ab5446a391c7fa14e5') { throw 'Pinned upstream Cargo.lock mismatch' }
Run 'git' @('-C',$Source,'apply','--check',$NoTools)
Run 'git' @('-C',$Source,'apply',$NoTools)
Run 'git' @('-C',$Source,'apply','--check',$Broker)
Run 'git' @('-C',$Source,'apply',$Broker)
Run 'git' @('-C',$Source,'apply','--check',$Portability)
Run 'git' @('-C',$Source,'apply',$Portability)
$CopiedHelper = Join-Path $Source 'memory-windows-pipe'
New-Item -ItemType Directory -Path (Join-Path $CopiedHelper 'src') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Helper 'Cargo.toml') -Destination (Join-Path $CopiedHelper 'Cargo.toml')
Copy-Item -LiteralPath (Join-Path $Helper 'src/lib.rs') -Destination (Join-Path $CopiedHelper 'src/lib.rs')
if ((Hash (Join-Path $Source 'Cargo.lock')) -ne $PatchedLockHash) { throw 'Patched lock differs: only the exact local ownership helper is allowed' }
if ($InstallToolchain) { Run 'rustup' @('toolchain','install',$Nightly,'--profile','minimal') }
$PreviousTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = Join-Path $BuildRoot 'target'
Push-Location $Source
try {
    $HelperTests = Join-Path $BuildRoot 'pipe-helper-tests.exe'
    Run 'rustc' @("+$Nightly",'--edition=2024','--test',(Join-Path $CopiedHelper 'src/lib.rs'),'-D','warnings','-o',$HelperTests)
    Run $HelperTests @('--test-threads=1')
    Run 'cargo' @("+$Nightly",'build','--locked','--release','--bin','pi')
} finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $PreviousTarget
}
if ((Hash (Join-Path $Source 'Cargo.lock')) -ne $PatchedLockHash) { throw 'Patched lock changed during build' }
$Binary = Join-Path $BuildRoot 'target/release/pi.exe'
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'No native Pi executable was produced' }
$Artifact = [ordered]@{
    source_revision=$Revision
    no_tools_patch_sha256=$NoToolsHash
    broker_patch_sha256=$BrokerHash
    portability_patch_sha256=$PortabilityHash
    helper_manifest_sha256=$HelperManifestHash
    helper_source_sha256=$HelperSourceHash
    patched_lock_sha256=$PatchedLockHash
    binary_sha256=(Hash $Binary)
}
[IO.File]::WriteAllText((Join-Path $BuildRoot 'windows-artifact.json'), ($Artifact | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
$RustVersion = & rustc "+$Nightly" -vV
if ($LASTEXITCODE -ne 0) { throw 'Cannot record native compiler' }
$Evidence = [ordered]@{
    schema_version=1
    target='x86_64-pc-windows-msvc'
    source_archive_sha256=(Hash $Archive)
    upstream_lock_sha256=$BeforeLock
    patched_lock_sha256=$PatchedLockHash
    rust_toolchain=$Nightly
    rustc=($RustVersion -join "`n")
    artifact=$Artifact
    signing_status='unsigned-internal-candidate'
    execution_verified=$false
    isolation_verified=$false
}
[IO.File]::WriteAllText((Join-Path $BuildRoot 'build-evidence.json'), ($Evidence | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
Write-Host 'Native build recorded. This is not execution or AppContainer acceptance evidence.'
