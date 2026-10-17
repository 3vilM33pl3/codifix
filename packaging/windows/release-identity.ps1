# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure identity calculation: safe to load in build and smoke-test sessions.
function Get-WindowsReleaseIdentity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ApplicationVersion,
        [Parameter(Mandatory = $true)][ValidateRange(1, 65535)][int]$InstallerBuild
    )
    if ($ApplicationVersion -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$') {
        throw 'Expected application semver without build metadata'
    }
    $major = [int]$Matches[1]
    $minor = [int]$Matches[2]
    if ($major -gt 255 -or $minor -gt 255) { throw 'MSI major/minor fields must be at most 255' }
    # The release ledger must increase build across patches/RCs within major.minor.
    return [pscustomobject][ordered]@{
        release_identity = "windows-v$ApplicationVersion-build.$InstallerBuild"
        asset_prefix = "codifix-$ApplicationVersion-windows-x86_64-build.$InstallerBuild"
        installer_version = "$major.$minor.$InstallerBuild"
    }
}
