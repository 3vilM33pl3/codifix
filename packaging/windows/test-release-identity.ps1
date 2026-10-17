# SPDX-License-Identifier: AGPL-3.0-or-later
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-identity.ps1')
$first = Get-WindowsReleaseIdentity -ApplicationVersion '3.0.0-rc.1' -InstallerBuild 1
$next = Get-WindowsReleaseIdentity -ApplicationVersion '3.0.0-rc.1' -InstallerBuild 2
$patch = Get-WindowsReleaseIdentity -ApplicationVersion '3.0.1' -InstallerBuild 3
$stable = Get-WindowsReleaseIdentity -ApplicationVersion '3.0.0' -InstallerBuild 2
if ($stable.installer_version -ne '3.0.2' -or $stable.release_identity -ne 'windows-v3.0.0-build.2') { throw 'Incorrect stable preview identity' }
if ($stable.asset_prefix -eq $first.asset_prefix) { throw 'Stable package collides with the RC artifact' }
if ($first.installer_version -ne '3.0.1') { throw 'RC installer version is not independent of semver' }
if ([version]$next.installer_version -le [version]$first.installer_version) { throw 'MSI ordering regressed' }
if ([version]$patch.installer_version -le [version]$next.installer_version) { throw 'Patch ordering regressed' }
if ($first.release_identity -ne 'windows-v3.0.0-rc.1-build.1') { throw 'Incorrect candidate identity' }
if ($first.asset_prefix -eq $next.asset_prefix) { throw 'Candidate artifact identity reused' }
foreach ($bad in @('2.1', '3.0.0+untracked', '../3.0.0', '256.1.0')) {
    $rejected = $false
    try { $null = Get-WindowsReleaseIdentity -ApplicationVersion $bad -InstallerBuild 1 } catch { $rejected = $true }
    if (-not $rejected) { throw "Accepted invalid release version: $bad" }
}
Write-Output 'Windows release identity tests passed'
