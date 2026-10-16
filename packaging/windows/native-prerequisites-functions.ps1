# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure descriptor validation; no network, service or database side effects.
function Get-NativePrerequisitePlan($Manifest) {
    if ($Manifest.schema_version -ne 1 -or $Manifest.kind -cne 'memory-native-postgres-runtime' -or
        $Manifest.platform -cne 'windows-x86_64' -or $Manifest.postgres_version -cne '17.11' -or
        $Manifest.pgvector_version -cne '0.8.7' -or $Manifest.crt_version -notmatch '^\d+(\.\d+){1,3}$' -or
        $Manifest.package_sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Unsupported or unpinned native prerequisite descriptor' }
    $files = @($Manifest.payload_files)
    if ($files.Count -lt 1 -or $files.Count -gt 20000) { throw 'Complete bounded native payload inventory required' }
    $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $files) {
        Assert-NativeArchiveName $file.name
        if ($file.name.EndsWith('/') -or -not $names.Add($file.name) -or $file.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Duplicate or invalid native payload entry' }
        if ($file.name -match '(?i)(^|/)(data|pgdata|node_modules)(/|$)' -or $file.name -match '(?i)(^|/)(postgresql\.conf|pg_hba\.conf|pgpass\.conf|postmaster\.pid)$') { throw 'Runtime bundle must not contain database state or credentials' }
    }
    foreach ($required in @('pgsql/bin/postgres.exe','pgsql/bin/initdb.exe','pgsql/bin/pg_ctl.exe','pgsql/bin/psql.exe',
        'pgsql/bin/vcruntime140.dll','pgsql/bin/msvcp140.dll','pgsql/lib/vector.dll','pgsql/lib/pgcrypto.dll',
        'pgsql/lib/pg_trgm.dll','pgsql/share/extension/vector.control','pgsql/share/extension/vector--0.8.7.sql',
        'pgsql/share/extension/pgcrypto.control','pgsql/share/extension/pg_trgm.control')) {
        if (-not $names.Contains($required)) { throw "Incomplete native prerequisite: $required" }
    }
    $components = @($Manifest.components)
    if ($components.Count -ne 3) { throw 'PostgreSQL, pgvector and CRT provenance/notices required' }
    foreach ($name in @('postgresql','pgvector','msvc-crt')) {
        $entry = @($components | Where-Object { $_.name -ceq $name })
        if ($entry.Count -ne 1 -or $entry[0].source_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            -not $entry[0].version -or -not $entry[0].license -or -not $names.Contains($entry[0].notice_path)) { throw 'Missing native component source digest or bundled licence notice' }
        $version = switch ($name) { 'postgresql' { $Manifest.postgres_version }; 'pgvector' { $Manifest.pgvector_version }; 'msvc-crt' { $Manifest.crt_version } }
        if ($entry[0].version -cne $version) { throw 'Native component versions disagree' }
    }
    return [pscustomobject][ordered]@{ kind = $Manifest.kind; postgres_version = $Manifest.postgres_version
        pgvector_version = $Manifest.pgvector_version; crt_version = $Manifest.crt_version
        package_sha256 = $Manifest.package_sha256; payload_files = $files; components = $components
        native_stack_bundled = $false; database_provisioned = $false; service_created = $false }
}

function Get-NativePrerequisiteBinding($Plan, [string]$ManifestSha256, [guid]$OperationId, [string]$RuntimeRoot, $Receipt) {
    if ($ManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or $OperationId -eq [guid]::Empty) { throw 'Exact prerequisite manifest and operation identities required' }
    $destination=Join-Path $RuntimeRoot ('postgres-17.11-vector-0.8.7-' + $ManifestSha256)
    $input=Get-NativeInputDigest ([ordered]@{schema_version=1;manifest_sha256=$ManifestSha256;plan=$Plan;destination=$destination})
    if ($Receipt.schema_version -ne 1 -or $Receipt.phase -cne 'verified' -or
        $Receipt.operation_id -cne $OperationId.ToString() -or $Receipt.destination -cne $destination -or
        $Receipt.input_sha256 -cne $input -or $Receipt.database_provisioned -ne $false -or
        $Receipt.service_created -ne $false) { throw 'Native prerequisite receipt does not match the verified runtime' }
    return [pscustomobject][ordered]@{schema_version=1;manifest_sha256=$ManifestSha256;operation_id=$OperationId.ToString()
        input_sha256=$input;runtime_directory=$destination;postgres_bin=(Join-Path $destination 'pgsql/bin')
        verification_scope='protected_payload_only';database_readiness_verified=$false}
}

function Close-NativePrerequisiteLease($Lease) {
    if ($null -ne $Lease) { foreach ($guard in $Lease.guards) { $guard.Dispose() } }
}

function Open-NativePrerequisiteLease([string]$ManifestPath, [string]$ManifestSha256, [guid]$OperationId) {
    Initialize-NativeReleaseGuard
    $guards=[Collections.Generic.List[IDisposable]]::new()
    try {
        $guards.Add([MemoryLayer.ProtectedRelease]::Open((Split-Path -Parent ([IO.Path]::GetFullPath($ManifestPath)))))
        if ((Get-Item -LiteralPath $ManifestPath).Length -gt 8MB -or
            (Get-FileHash -LiteralPath $ManifestPath).Hash.ToLowerInvariant() -cne $ManifestSha256) { throw 'Prerequisite manifest differs from pinned input' }
        $plan=Get-NativePrerequisitePlan (Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json)
        $root=Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Memory Layer\runtimes'
        $journals=Join-Path $root 'operations'
        $guards.Add([MemoryLayer.ProtectedRelease]::Open($journals))
        $receiptPath=Join-Path $journals ($OperationId.ToString() + '.json')
        if ((Get-Item -LiteralPath $receiptPath).Length -gt 1MB) { throw 'Prerequisite receipt too large' }
        $receipt=Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
        $binding=Get-NativePrerequisiteBinding $plan $ManifestSha256 $OperationId $root $receipt
        $guards.Add([MemoryLayer.ProtectedRelease]::Open($binding.runtime_directory))
        Assert-NativePayload $binding.runtime_directory $plan.payload_files
        return [pscustomobject]@{binding=$binding;guards=$guards}
    } catch { foreach ($guard in $guards) { $guard.Dispose() }; throw }
}
