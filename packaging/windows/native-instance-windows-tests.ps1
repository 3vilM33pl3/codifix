# SPDX-License-Identifier: AGPL-3.0-or-later
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows is required; ACL/link tests cannot be replaced by Linux checks' }
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
Initialize-NativeReleaseGuard
$outer = Join-Path ([IO.Path]::GetTempPath()) ('memory-native-release-test-' + [guid]::NewGuid())
$release = Join-Path $outer 'release'
$null = New-Item -ItemType Directory -Path $release
$caller = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = [Security.AccessControl.DirectorySecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($caller.Value, 'S-1-5-18', 'S-1-5-32-544')) {
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
}
Set-Acl -LiteralPath $release -AclObject $acl
$file = Join-Path $release 'synthetic.txt'
[IO.File]::WriteAllText($file, 'synthetic fixture')
$payload = @([pscustomobject]@{ name = 'synthetic.txt'; sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() })
$guard = $null
function Assert-Rejected($Operation, [string]$Message) {
    $rejected = $false
    try { & $Operation } catch { $rejected = $true }
    if (-not $rejected) { throw $Message }
}
try {
    Assert-NativePayload $release $payload
    $guard = [MemoryLayer.ProtectedRelease]::Open($release)
    Assert-Rejected { $handle = [IO.File]::Open($file, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite); $handle.Dispose() } 'Validated release bytes remained writable during use'
    Assert-Rejected { [IO.File]::Move($file, (Join-Path $release 'renamed.txt')) } 'Validated release file was replaceable during use'
    Assert-Rejected { [IO.Directory]::Move($release, (Join-Path $outer 'renamed')) } 'Validated release directory was replaceable during use'
    $guard.Dispose(); $guard = $null

    $originalAcl = Get-Acl -LiteralPath $file
    $writableAcl = Get-Acl -LiteralPath $file
    $writableAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'Write', 'Allow'))
    Set-Acl -LiteralPath $file -AclObject $writableAcl
    Assert-NativePayload $release $payload # Correct hash alone is insufficient.
    Assert-Rejected { $bad = [MemoryLayer.ProtectedRelease]::Open($release); $bad.Dispose() } 'Everyone-writable valid-hash release accepted'
    Set-Acl -LiteralPath $file -AclObject $originalAcl

    $alias = Join-Path $outer 'alias.txt'
    $null = New-Item -ItemType HardLink -Path $alias -Target $file
    Assert-NativePayload $release $payload
    Assert-Rejected { $bad = [MemoryLayer.ProtectedRelease]::Open($release); $bad.Dispose() } 'Hardlinked valid-hash release accepted'
    Remove-Item -LiteralPath $alias -Force

    $junction = Join-Path $outer 'junction'
    $null = New-Item -ItemType Junction -Path $junction -Target $release
    Assert-Rejected { $bad = [MemoryLayer.ProtectedRelease]::Open($junction); $bad.Dispose() } 'Reparse-point release accepted'
    # Windows PowerShell 5.1 treats a nonempty junction like a directory and
    # prompts even with -Force. Delete only the junction entry, never its tree.
    [IO.Directory]::Delete($junction)
    if (-not [IO.File]::Exists($file)) { throw 'Removing the junction affected its target' }
    Write-Output 'Native immutable-release ACL, hardlink, reparse and held-handle tests passed'
} finally {
    if ($guard) { $guard.Dispose() }
    Remove-Item -LiteralPath $outer -Recurse -Force
}
