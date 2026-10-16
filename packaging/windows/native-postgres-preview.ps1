# SPDX-License-Identifier: AGPL-3.0-or-later
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Name,
    [Parameter(Mandatory=$true)][ValidateRange(1024,65535)][int]$Port,
    [Parameter(Mandatory=$true)][string]$ClusterPath,
    [Parameter(Mandatory=$true)][string]$PasswordFile,
    [Parameter(Mandatory=$true)][guid]$OperationId,
    [Parameter(Mandatory=$true)][string]$RuntimeManifestPath,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$RuntimeManifestSha256,
    [Parameter(Mandatory=$true)][guid]$RuntimeOperationId,
    [Parameter(Mandatory=$true)][ValidateLength(1,1024)][string]$RecoveryReference
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
. (Join-Path $PSScriptRoot 'native-postgres-functions.ps1')
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows required; this preview has no WSL/Docker fallback' }
$review=$null
try {
    $review=Open-NativePostgresReview $Name $Port $ClusterPath $PasswordFile $OperationId $RuntimeManifestPath $RuntimeManifestSha256 $RuntimeOperationId $RecoveryReference
    $plan=$review.plan
    if (Test-Path -LiteralPath $plan.cluster_directory) { throw 'Existing cluster is not a fresh provisioning target; no adoption or database upgrade is implemented' }
    if (Get-Service -Name $plan.service_name -ErrorAction SilentlyContinue) { throw 'Selected PostgreSQL service already exists; do not overwrite it' }
    [pscustomobject]@{input_sha256=$review.input_sha256;plan=$plan;starts_services=$false;mutates_database=$false;execution_available=$true} | ConvertTo-Json -Depth 16
} finally {
    Close-NativePostgresReview $review
}
