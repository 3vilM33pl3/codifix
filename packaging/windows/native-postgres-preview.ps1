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
$runtime=$null; $guard=$null; $file=$null
try {
    $runtime=Open-NativePrerequisiteLease $RuntimeManifestPath $RuntimeManifestSha256 $RuntimeOperationId
    $root=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'MemoryLayer'
    $plan=Get-NativePostgresPlan $Name $Port $ClusterPath $root $PasswordFile $runtime.binding $OperationId $RecoveryReference
    if (Test-Path -LiteralPath $plan.cluster_directory) { throw 'Existing cluster is not a fresh provisioning target; no adoption or database upgrade is implemented' }
    if (Get-Service -Name $plan.service_name -ErrorAction SilentlyContinue) { throw 'Selected PostgreSQL service already exists; do not overwrite it' }
    # Retain a hard-link/reparse/owner/write-validated path plus a deny-write and
    # deny-delete file handle while validating the secret. No secret is emitted.
    $guard=[MemoryLayer.ProtectedRelease]::Open([IO.Path]::GetDirectoryName($PasswordFile))
    $file=[IO.File]::Open($PasswordFile,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    if ($file.Length -lt 32 -or $file.Length -gt 4096) { throw 'Use a bounded strong initialization password file (32–4096 UTF-8 bytes)' }
    $allowed=@('S-1-5-18','S-1-5-32-544',[Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
    $acl=Get-Acl -LiteralPath $PasswordFile
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and $rule.FileSystemRights -ne 0 -and $rule.IdentityReference.Value -notin $allowed) { throw 'Initialization password file is accessible to another principal' }
    }
    $bytes=[byte[]]::new([int]$file.Length)
    $offset=0
    while ($offset -lt $bytes.Length) {
        $count=$file.Read($bytes,$offset,$bytes.Length-$offset)
        if ($count -eq 0) { throw 'Initialization password changed during review' }
        $offset+=$count
    }
    $text=[Text.UTF8Encoding]::new($false,$true).GetString($bytes)
    if ($text.Contains("`r") -or $text.Contains("`n") -or $text.Contains([char]0) -or $text.Length -lt 32) { throw 'Initialization password must be one nonempty UTF-8 line without a terminator' }
    $sha=[Security.Cryptography.SHA256]::Create()
    try { $secretFingerprint=([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose(); [Array]::Clear($bytes,0,$bytes.Length); $text=$null }
    $digest=Get-NativeInputDigest ([ordered]@{plan=$plan;password_fingerprint=$secretFingerprint})
    [pscustomobject]@{input_sha256=$digest;plan=$plan;starts_services=$false;mutates_database=$false;execution_available=$false} | ConvertTo-Json -Depth 16
} finally {
    if ($file) { $file.Dispose() }; if ($guard) { $guard.Dispose() }
    Close-NativePrerequisiteLease $runtime
}
