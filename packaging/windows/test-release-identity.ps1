# SPDX-License-Identifier: AGPL-3.0-or-later
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'release-identity.ps1')
$first = Get-WindowsReleaseIdentity -ApplicationVersion '2.1.0-rc.5' -InstallerBuild 1
$next = Get-WindowsReleaseIdentity -ApplicationVersion '2.1.0-rc.5' -InstallerBuild 2
$patch = Get-WindowsReleaseIdentity -ApplicationVersion '2.1.1' -InstallerBuild 3
if ($first.installer_version -ne '2.1.1') { throw 'RC installer version is not independent of semver' }
if ([version]$next.installer_version -le [version]$first.installer_version) { throw 'MSI ordering regressed' }
if ([version]$patch.installer_version -le [version]$next.installer_version) { throw 'Patch ordering regressed' }
if ($first.release_identity -ne 'windows-v2.1.0-rc.5-build.1') { throw 'Incorrect candidate identity' }
if ($first.asset_prefix -eq $next.asset_prefix) { throw 'Candidate artifact identity reused' }
foreach ($bad in @('2.1', '2.1.0+untracked', '../2.1.0', '256.1.0')) {
    $rejected = $false
    try { $null = Get-WindowsReleaseIdentity -ApplicationVersion $bad -InstallerBuild 1 } catch { $rejected = $true }
    if (-not $rejected) { throw "Accepted invalid release version: $bad" }
}
Write-Output 'Windows release identity tests passed'
