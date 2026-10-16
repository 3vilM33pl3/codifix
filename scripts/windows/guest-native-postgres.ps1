# SPDX-License-Identifier: AGPL-3.0-or-later
# Proof tooling for disposable Memory VM, not a customer database installer.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [ValidateSet('Prepare','Start','Stop','Verify')][string]$Action = 'Prepare'
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'Native prerequisite proof is restricted to the disposable Memory VM'
}
$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.format -ne 'memory-windows-postgres-proof-v1' -or
    $manifest.postgres_version -ne '17.11' -or $manifest.pgvector_version -ne '0.8.7') {
    throw 'Unreviewed native PostgreSQL inputs'
}
$root = 'C:\MemoryLab\NativePostgres'
$cache = Join-Path $root 'downloads'
$runtime = Join-Path $root 'runtime'
$pg = Join-Path $runtime 'pgsql'
$data = Join-Path $root 'data'
$secret = Join-Path $root 'test-password.dpapi'
$bin = Join-Path $pg 'bin'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
New-Item -ItemType Directory -Force $root | Out-Null
& icacls.exe $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Private database proof directory ACL failed' }
function Run-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Native prerequisite command failed: $([IO.Path]::GetFileName($Exe)) ($LASTEXITCODE)" }
}
function Input-Artifact($Artifact) {
    if ($Artifact.file -notmatch '^[A-Za-z0-9_.-]+$' -or $Artifact.sha256 -notmatch '^[0-9a-f]{64}$' -or
        ([uri]$Artifact.url).Scheme -ne 'https') { throw 'Unsafe native dependency descriptor' }
    $path = Join-Path $cache $Artifact.file
    if (-not (Test-Path $path)) {
        Invoke-WebRequest -UseBasicParsing -Uri $Artifact.url -OutFile ($path + '.partial')
        Move-Item -LiteralPath ($path + '.partial') -Destination $path
    }
    if ((Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Artifact.sha256) {
        throw 'Native dependency digest mismatch; retain inputs for inspection'
    }
    return $path
}
if ($Action -eq 'Prepare') {
    New-Item -ItemType Directory -Force $cache,$runtime | Out-Null
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $postgres = Input-Artifact $manifest.postgres
    $vector = Input-Artifact $manifest.pgvector
    if (-not (Test-Path (Join-Path $bin 'postgres.exe'))) {
        Expand-Archive -LiteralPath $postgres -DestinationPath $runtime
    }
    $source = Join-Path $root 'source'
    New-Item -ItemType Directory -Force $source | Out-Null
    if (-not (Test-Path (Join-Path $source 'pgvector-0.8.7\Makefile.win'))) {
        Run-Checked "$env:SystemRoot\System32\tar.exe" @('-xzf',$vector,'-C',$source)
    }
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { throw 'MSVC native toolchain is required before pgvector proof' }
    $vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
    $environment = & "$env:SystemRoot\System32\cmd.exe" /d /s /c "`"`"$vcvars`" >nul && set`""
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize native MSVC environment' }
    foreach ($line in $environment) {
        if ($line -match '^(PATH|INCLUDE|LIB|LIBPATH|VCINSTALLDIR|VCToolsInstallDir|VCToolsRedistDir|WindowsSdkDir|WindowsSDKVersion)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
    # Do not let the developer machine's system-wide CRT hide a missing runtime
    # dependency. Retain the exact redistributable DLLs beside PostgreSQL, where
    # the clean-runtime candidate will need them too. This is not evidence that
    # the candidate has run without Visual Studio; that remains a separate test.
    if (-not $env:VCToolsRedistDir) { throw 'MSVC did not identify its redistributable runtime' }
    $crt = Join-Path $env:VCToolsRedistDir 'x64\Microsoft.VC143.CRT'
    $crtFiles = @(Get-ChildItem -LiteralPath $crt -Filter '*.dll' -File)
    if (-not ($crtFiles | Where-Object Name -eq 'vcruntime140.dll') -or
        -not ($crtFiles | Where-Object Name -eq 'msvcp140.dll')) {
        throw 'MSVC x64 redistributable runtime is incomplete'
    }
    foreach ($file in $crtFiles) {
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $bin $file.Name) -Force
    }
    Run-Checked (Join-Path $bin 'postgres.exe') @('--version')
    $env:PGROOT = $pg
    Push-Location (Join-Path $source 'pgvector-0.8.7')
    try {
        Run-Checked 'nmake.exe' @('/F','Makefile.win')
        Run-Checked 'nmake.exe' @('/F','Makefile.win','install')
    } finally { Pop-Location }
    $libraries = @('vector.dll','pgcrypto.dll','pg_trgm.dll')
    foreach ($name in $libraries) {
        if (-not (Test-Path (Join-Path $pg "lib\$name"))) { throw "Required native extension is missing: $name" }
    }
    $support = @(Get-ChildItem -LiteralPath $bin -Filter '*.dll' -File | Sort-Object Name | ForEach-Object {
        @{name=$_.Name;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant();version=$_.VersionInfo.FileVersion}
    })
    $receipt = [ordered]@{format='memory-windows-postgres-proof-result-v1'; manifest_sha256=(Get-FileHash $ManifestPath).Hash.ToLowerInvariant(); postgres_version=(& (Join-Path $bin 'postgres.exe') --version); postgres_sha256=(Get-FileHash (Join-Path $bin 'postgres.exe')).Hash.ToLowerInvariant(); extensions=@($libraries | ForEach-Object { @{name=$_;sha256=(Get-FileHash (Join-Path $pg "lib\$_")).Hash.ToLowerInvariant()} }); runtime_support=$support; crt_source=$crt; clean_runtime_verified=$false; native_execution_verified=$false}
    $receipt | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $root 'prepared.json') -Encoding UTF8
    $receipt | ConvertTo-Json -Depth 8
    return
}
if (-not (Test-Path (Join-Path $root 'prepared.json'))) { throw 'Prepare native dependencies first' }
$prepared = Get-Content (Join-Path $root 'prepared.json') -Raw | ConvertFrom-Json
if ($prepared.format -ne 'memory-windows-postgres-proof-result-v1' -or
    $prepared.manifest_sha256 -ne (Get-FileHash $ManifestPath).Hash.ToLowerInvariant() -or
    $prepared.postgres_sha256 -ne (Get-FileHash (Join-Path $bin 'postgres.exe')).Hash.ToLowerInvariant()) {
    throw 'Prepared native runtime does not match its retained manifest and receipt'
}
if ((@($prepared.extensions | ForEach-Object { $_.name } | Sort-Object) -join ',') -ne 'pg_trgm.dll,pgcrypto.dll,vector.dll') {
    throw 'Prepared native extension inventory is incomplete'
}
foreach ($extension in $prepared.extensions) {
    if ($extension.name -notin @('vector.dll','pgcrypto.dll','pg_trgm.dll') -or
        $extension.sha256 -ne (Get-FileHash (Join-Path $pg "lib\$($extension.name)")).Hash.ToLowerInvariant()) {
        throw 'Prepared native extension changed after verification'
    }
}
$supportNames = @($prepared.runtime_support | ForEach-Object { $_.name } | Sort-Object)
$actualSupportNames = @(Get-ChildItem -LiteralPath $bin -Filter '*.dll' -File | ForEach-Object { $_.Name } | Sort-Object)
if (-not $supportNames.Count -or ($supportNames -join ',') -ne ($actualSupportNames -join ',') -or
    'vcruntime140.dll' -notin $supportNames -or 'msvcp140.dll' -notin $supportNames) {
    throw 'Prepared native support DLL inventory is incomplete or changed'
}
foreach ($library in $prepared.runtime_support) {
    if ($library.name -notmatch '^[A-Za-z0-9_.-]+\.dll$' -or
        $library.sha256 -ne (Get-FileHash -LiteralPath (Join-Path $bin $library.name)).Hash.ToLowerInvariant()) {
        throw 'Prepared native support DLL changed after verification'
    }
}
if ($Action -eq 'Stop') {
    Run-Checked (Join-Path $bin 'pg_ctl.exe') @('-D',$data,'-m','fast','-w','stop')
    return
}
if (-not (Test-Path $secret)) {
    if (Test-Path $data) { throw 'Database exists without its retained test credential; restore the clean snapshot' }
    $password = [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')
    ConvertFrom-SecureString (ConvertTo-SecureString $password -AsPlainText -Force) | Set-Content $secret -Encoding ASCII
}
$secure = ConvertTo-SecureString (Get-Content $secret -Raw).Trim()
$pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try { $password = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
if ($Action -eq 'Start') {
    if (-not (Test-Path (Join-Path $data 'PG_VERSION'))) {
        if (Test-Path $data) { throw 'Partial initdb retained; inspect or restore the clean lab snapshot' }
        $passwordFile = Join-Path $root 'initdb-password.private'
        $password | Set-Content $passwordFile -Encoding ASCII
        try { Run-Checked (Join-Path $bin 'initdb.exe') @('-D',$data,'-U','memorylab_test','--auth-host=scram-sha-256','--auth-local=scram-sha-256','--encoding=UTF8','--pwfile',$passwordFile) }
        finally { Remove-Item -LiteralPath $passwordFile -ErrorAction SilentlyContinue }
    }
    if ((Get-Content (Join-Path $data 'PG_VERSION') -Raw).Trim() -ne '17') { throw 'Unexpected native database major version' }
    & (Join-Path $bin 'pg_ctl.exe') -D $data status | Out-Null
    if ($LASTEXITCODE -eq 3) {
        Run-Checked (Join-Path $bin 'pg_ctl.exe') @('-D',$data,'-l',(Join-Path $root 'postgres.log'),'-o','-h 127.0.0.1 -p 55471','-w','start')
    } elseif ($LASTEXITCODE -ne 0) { throw 'Native PostgreSQL status is uncertain' }
}
$previous = $env:PGPASSWORD
try {
    $env:PGPASSWORD = $password
    Run-Checked (Join-Path $bin 'psql.exe') @('-h','127.0.0.1','-p','55471','-U','memorylab_test','-d','postgres','-v','ON_ERROR_STOP=1','-c','CREATE EXTENSION IF NOT EXISTS vector; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS pg_trgm; SELECT version(); SELECT extname, extversion FROM pg_extension ORDER BY extname;')
} finally { $env:PGPASSWORD = $previous; $password = $null }
@{format='memory-windows-postgres-execution-v1'; verified_at=[DateTime]::UtcNow.ToString('o'); host='127.0.0.1'; port=55471; production=$false} | ConvertTo-Json | Set-Content (Join-Path $root 'verified.json') -Encoding UTF8
