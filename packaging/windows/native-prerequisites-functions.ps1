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
