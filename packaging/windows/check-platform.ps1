# SPDX-License-Identifier: AGPL-3.0-or-later
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Cargo([string[]]$Arguments) {
    & cargo @Arguments --manifest-path (Join-Path $Root 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw "cargo failed ($LASTEXITCODE): $Arguments" }
}

& rustc --version --verbose
if ($LASTEXITCODE -ne 0) { throw 'Rust compiler unavailable' }
& cargo --version
if ($LASTEXITCODE -ne 0) { throw 'Cargo unavailable' }
if ($env:OS -ne 'Windows_NT' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'This native smoke test requires Windows x64 (not WSL)'
}
# Early filesystem/platform coverage, not the full release acceptance gate.
Invoke-Cargo -Arguments @('test', '-p', 'mem-platform-files', '-p', 'mem-platform', '--locked', '--all-targets')
& cargo clippy -p mem-platform-files -p mem-platform --all-targets --locked --manifest-path (Join-Path $Root 'Cargo.toml') -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Windows platform Clippy failed' }
