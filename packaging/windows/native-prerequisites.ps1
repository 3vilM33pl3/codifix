# SPDX-License-Identifier: AGPL-3.0-or-later
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Preview','Install')][string]$Action,
    [Parameter(Mandatory=$true)][guid]$OperationId,
    [Parameter(Mandatory=$true)][string]$PackagePath,
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ManifestSha256,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedInputSha256
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows required; no WSL/Docker fallback' }
if ($OperationId -eq [guid]::Empty) { throw 'Nonzero operation UUID required' }
Initialize-NativeReleaseGuard
$manifestGuard = $null; $package = $null; $guard = $null; $lock = $null
try {
    # Require operator-protected input directories, and retain their handles
    # through validation/copy. Existing runtime/instance directories are never
    # treated as download caches or overwritten.
    $manifestGuard = [MemoryLayer.ProtectedRelease]::Open((Split-Path -Parent ([IO.Path]::GetFullPath($ManifestPath))))
    $manifestLength=(Get-Item -LiteralPath $ManifestPath).Length
    if ($manifestLength -gt 32MB) { throw 'Prerequisite manifest too large' }
    if ((Get-FileHash -LiteralPath $ManifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Prerequisite manifest differs from reviewed digest' }
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
    $plan = Get-NativePrerequisitePlan $manifest
    Assert-NativeRuntimeManifestSize $manifestLength $plan.kind
    $packageParent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($PackagePath))
    $packageGuard = [MemoryLayer.ProtectedRelease]::Open($packageParent)
    try {
        $package = [IO.File]::Open($PackagePath,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        if ($package.Length -gt 2GB) { throw 'Prerequisite ZIP too large' }
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = ([BitConverter]::ToString($sha.ComputeHash($package))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
        if ($hash -cne $plan.package_sha256) { throw 'Prerequisite package differs from manifest' }
        $package.Position = 0
        $identity = Get-NativeRuntimeIdentity $plan $ManifestSha256
        $root = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Memory Layer\runtimes'
        $release = Join-Path $root $identity
        $binding = [ordered]@{ schema_version=1; manifest_sha256=$ManifestSha256; plan=$plan; destination=$release }
        $digest = Get-NativeInputDigest $binding
        if ($Action -eq 'Preview') { [pscustomobject]@{ input_sha256=$digest; plan=$binding; mutates_database=$false; starts_services=$false } | ConvertTo-Json -Depth 16; return }
        if ($ExpectedInputSha256 -cne $digest) { throw 'Run Preview and supply its exact input digest' }
        $principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
        if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Elevated operator required' }
        $memoryRoot = Split-Path -Parent $root
        Set-NativePrivateDirectory $memoryRoot -SharedAncestor
        Set-NativePrivateDirectory $root -SharedAncestor
        $journals = Join-Path $root 'operations'
        Set-NativePrivateDirectory $journals
        $lock = [IO.File]::Open((Join-Path $journals 'operator.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
        $receipt = Join-Path $journals ($OperationId.ToString() + '.json')
        $journal = [ordered]@{ schema_version=1; input_sha256=$digest; operation_id=$OperationId.ToString(); phase='prepared'; destination=$release; database_provisioned=$false; service_created=$false }
        if (Test-Path -LiteralPath $receipt) {
            $prior = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
            Assert-NativeRetry $prior $digest
            if ($prior.operation_id -cne $OperationId.ToString()) { throw 'Prerequisite receipt operation identity differs' }
        }
        Write-NativeJournal $receipt $journal
        if (-not (Test-Path -LiteralPath $release)) {
            $stage = Join-Path $root ($identity + '.staging-' + $OperationId)
            if (Test-Path -LiteralPath $stage) {
                # Preserve partial/rejected input, never merge into an old tree.
                $old = [MemoryLayer.ProtectedRelease]::Open($stage); $old.Dispose()
                [IO.Directory]::Move($stage,($stage + '.retained-' + [guid]::NewGuid()))
            }
            Set-NativePrivateDirectory $stage
            $journal.phase='staging'; Write-NativeJournal $receipt $journal
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            $archive = [IO.Compression.ZipArchive]::new($package,[IO.Compression.ZipArchiveMode]::Read,$true)
            try {
                $entryLimit=if($plan.kind -ceq 'memory-native-pds-runtime'){100000}else{30000}
                if ($archive.Entries.Count -gt $entryLimit) { throw 'Prerequisite archive too many entries' }
                $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase); $total=[int64]0
                foreach ($entry in $archive.Entries) {
                    Assert-NativeArchiveName $entry.FullName
                    if (-not $seen.Add($entry.FullName.TrimEnd('/'))) { throw 'Duplicate/colliding prerequisite path' }
                    $total += $entry.Length
                    if ($total -gt 2GB) { throw 'Expanded prerequisite exceeds 2GiB' }
                    $target = Join-Path $stage $entry.FullName.Replace('/',[IO.Path]::DirectorySeparatorChar)
                    if ($entry.FullName.EndsWith('/')) { $null=New-Item -ItemType Directory -Path $target -Force; continue }
                    $null=New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force
                    $source=$entry.Open(); $output=[IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
                    try {
                        $buffer=[byte[]]::new(1MB); $copied=[int64]0
                        while (($count=$source.Read($buffer,0,$buffer.Length)) -gt 0) {
                            $copied += $count
                            if ($copied -gt $entry.Length) { throw 'Prerequisite entry exceeds its declared size' }
                            $output.Write($buffer,0,$count)
                        }
                        if ($copied -ne $entry.Length) { throw 'Incomplete prerequisite entry' }
                        $output.Flush($true)
                    } finally { $source.Dispose(); $output.Dispose() }
                }
            } finally { $archive.Dispose() }
            $guard=[MemoryLayer.ProtectedRelease]::Open($stage)
            try { Assert-NativePayload $stage $plan.payload_files } finally { $guard.Dispose(); $guard=$null }
            [IO.Directory]::Move($stage,$release)
        }
        $guard=[MemoryLayer.ProtectedRelease]::Open($release)
        Assert-NativePayload $release $plan.payload_files
        $journal.phase='verified'; Write-NativeJournal $receipt $journal
        [pscustomobject]@{ phase='verified'; runtime=$release; receipt=$receipt; input_sha256=$digest; native_execution_verified=$false; native_stack_bundled=$false } | ConvertTo-Json
    } finally { if ($package) { $package.Dispose() }; $packageGuard.Dispose() }
} finally { if ($guard) { $guard.Dispose() }; if ($lock) { $lock.Dispose() }; if ($manifestGuard) { $manifestGuard.Dispose() } }
