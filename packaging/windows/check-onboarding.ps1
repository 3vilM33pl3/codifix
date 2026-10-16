# SPDX-License-Identifier: AGPL-3.0-or-later
# Shared hosted/VM native API smoke; not complete onboarding acceptance.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    throw 'Native Windows x64 is required (not WSL)'
}
$manifest = Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path 'Cargo.toml'
foreach ($filter in @('credentials::windows::tests','workspace::filesystem::windows::tests',
                      'agents::settings::filesystem::tests')) {
    $discovered = @(& cargo test --locked -p mem-cli --no-default-features --bin memory --manifest-path $manifest $filter -- --list)
    if ($LASTEXITCODE -ne 0 -or -not ($discovered | Where-Object { $_ -match ': test$' })) {
        throw "Native Windows onboarding API filter selected no tests: $filter"
    }
    & cargo test --locked -p mem-cli --no-default-features --bin memory --manifest-path $manifest $filter
    if ($LASTEXITCODE -ne 0) { throw "Native Windows onboarding API smoke failed: $filter" }
}
# These unit filters invoke neither providers nor company login endpoints and
# use isolated synthetic credential namespaces and temporary filesystem state.
