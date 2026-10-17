# SPDX-License-Identifier: AGPL-3.0-or-later
# Disposable native release-test database; never an installed-stack operator.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows' -or
    -not $env:RUNNER_TEMP -or -not $env:GITHUB_ENV) {
    throw 'Native release PostgreSQL bootstrap requires a disposable GitHub Windows runner'
}
$manifestPath = Join-Path $PSScriptRoot '..\..\scripts\windows\native-postgres.json'
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.format -ne 'memory-windows-postgres-proof-v1' -or
    $manifest.postgres_version -ne '17.11' -or $manifest.pgvector_version -ne '0.8.7' -or
    $manifest.postgres.sha256 -ne '6eabdf00d2893713b75db4336a23c3fdf505f056e217ec6e2e95d901750cfea3' -or
    $manifest.pgvector.sha256 -ne 'cac0b10c360f05b2d521200105ba3697e773d4cd3731f5a915a7e37ebe0bea85') {
    throw 'Unexpected native release-test dependency pins'
}
$root = Join-Path $env:RUNNER_TEMP ('memory-release-postgres-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not protect disposable PostgreSQL state' }
function Run-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Native prerequisite failed: $([IO.Path]::GetFileName($Exe)) ($LASTEXITCODE)" }
}
function Download-Pinned($Artifact) {
    if ($Artifact.file -notmatch '^[A-Za-z0-9_.-]+$' -or ([uri]$Artifact.url).Scheme -ne 'https') {
        throw 'Unsafe dependency descriptor'
    }
    $path = Join-Path $root $Artifact.file
    Invoke-WebRequest -Uri $Artifact.url -OutFile $path
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Artifact.sha256) {
        throw 'Downloaded dependency differs from its pinned digest'
    }
    return $path
}
$pg = Join-Path $env:ProgramFiles 'PostgreSQL\17'
# Reuse hosted tools only when the native SDK is present. Otherwise extract the
# existing digest-pinned upstream ZIP into this test's private directory: unlike
# a general installer, this does not create or reconfigure a machine service.
if (-not (Test-Path -LiteralPath (Join-Path $pg 'bin\postgres.exe')) -or
    -not (Test-Path -LiteralPath (Join-Path $pg 'include\server\postgres.h')) -or
    -not (Test-Path -LiteralPath (Join-Path $pg 'lib\postgres.lib'))) {
    $archive = Download-Pinned $manifest.postgres
    $runtime = Join-Path $root 'runtime'
    New-Item -ItemType Directory -Path $runtime | Out-Null
    Run-Checked "$env:SystemRoot\System32\tar.exe" @('-xf', $archive, '-C', $runtime)
    $pg = Join-Path $runtime 'pgsql'
}
$bin = Join-Path $pg 'bin'
$pgVersion = & (Join-Path $bin 'postgres.exe') --version
if ($LASTEXITCODE -ne 0 -or $pgVersion -notmatch '^postgres \(PostgreSQL\) 17\.') { throw 'Native PostgreSQL 17 required' }
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Native MSVC toolchain unavailable' }
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Native MSVC installation unavailable' }
$vcvars = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
if (-not (Test-Path -LiteralPath $vcvars)) { throw 'Native MSVC environment script unavailable' }
# PowerShell 7 rewrites embedded quotes passed through native argument binding;
# the historical Windows PowerShell invocation becomes literal \"\"C:\Program
# on hosted runners. Supply cmd.exe's raw command line through .NET instead.
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = "$env:SystemRoot\System32\cmd.exe"
$start.Arguments = '/d /s /c ""' + $vcvars + '" >nul && set"'
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
try {
    $null = $process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(120000)) {
        $process.Kill($true); $process.WaitForExit()
        throw 'Native MSVC environment initialization exceeded two minutes'
    }
    if ($process.ExitCode -ne 0) { throw "Could not initialize native MSVC environment (exit $($process.ExitCode))" }
    $environment = $stdout.GetAwaiter().GetResult() -split '\r?\n'
    $null = $stderr.GetAwaiter().GetResult()
} finally { $process.Dispose() }
foreach ($line in $environment) {
    if ($line -match '^(PATH|INCLUDE|LIB|LIBPATH|VCINSTALLDIR|VCToolsInstallDir|VCToolsRedistDir|WindowsSdkDir|WindowsSDKVersion)=(.*)$') {
        [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
    }
}
$archive = Download-Pinned $manifest.pgvector
Run-Checked "$env:SystemRoot\System32\tar.exe" @('-xzf', $archive, '-C', $root)
$previousPgRoot = $env:PGROOT
$env:PGROOT = $pg
Push-Location (Join-Path $root 'pgvector-0.8.7')
try {
    Run-Checked 'nmake.exe' @('/F', 'Makefile.win')
    Run-Checked 'nmake.exe' @('/F', 'Makefile.win', 'install')
} finally { Pop-Location; $env:PGROOT = $previousPgRoot }
foreach ($name in @('vector.dll', 'pgcrypto.dll', 'pg_trgm.dll')) {
    if (-not (Test-Path -LiteralPath (Join-Path $pg "lib\$name"))) { throw "Missing native extension: $name" }
}
$data = Join-Path $root 'data'
$password = [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')
$passwordFile = Join-Path $root 'initdb-password.private'
[IO.File]::WriteAllText($passwordFile, $password + "`n", [Text.Encoding]::ASCII)
try {
    Run-Checked (Join-Path $bin 'initdb.exe') @('-D', $data, '-U', 'memory_release', '--auth-host=scram-sha-256', '--auth-local=scram-sha-256', '--encoding=UTF8', '--pwfile', $passwordFile)
} finally { Remove-Item -LiteralPath $passwordFile -ErrorAction SilentlyContinue }
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
$started = $false
$previousPassword = $env:PGPASSWORD
try {
    Run-Checked (Join-Path $bin 'pg_ctl.exe') @('-D', $data, '-l', (Join-Path $root 'postgres.log'), '-o', "-h 127.0.0.1 -p $port", '-w', 'start')
    $started = $true
    $env:PGPASSWORD = $password
    Run-Checked (Join-Path $bin 'createdb.exe') @('-h', '127.0.0.1', '-p', "$port", '-U', 'memory_release', 'memory_release_smoke')
    Run-Checked (Join-Path $bin 'psql.exe') @('-h', '127.0.0.1', '-p', "$port", '-U', 'memory_release', '-d', 'memory_release_smoke', '-v', 'ON_ERROR_STOP=1', '-c', 'CREATE EXTENSION vector; CREATE EXTENSION pgcrypto; CREATE EXTENSION pg_trgm;')
    $url = "postgresql://memory_release:${password}@127.0.0.1:$port/memory_release_smoke"
    # Only synthetic runner credentials, still mask them from accidental logs.
    Write-Output "::add-mask::$password"
    Write-Output "::add-mask::$url"
    @("MEMORY_RELEASE_TEST_DATABASE_URL=$url", "MEMORY_RELEASE_PGDATA=$data", "MEMORY_RELEASE_PGBIN=$bin") |
        Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8
    [ordered]@{
        postgres_version = $pgVersion
        postgres_sha256 = (Get-FileHash -LiteralPath (Join-Path $bin 'postgres.exe')).Hash.ToLowerInvariant()
        pgvector_version = $manifest.pgvector_version
        pgvector_source_sha256 = $manifest.pgvector.sha256
        vector_dll_sha256 = (Get-FileHash -LiteralPath (Join-Path $pg 'lib\vector.dll')).Hash.ToLowerInvariant()
        host = '127.0.0.1'; port = $port; database = 'memory_release_smoke'; production = $false
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'receipt.json') -Encoding utf8
    Write-Host "Native PostgreSQL/pgvector test cluster ready; metadata: $root\receipt.json"
} catch {
    if ($started) { & (Join-Path $bin 'pg_ctl.exe') -D $data -m fast -w stop | Out-Null }
    throw
} finally { $env:PGPASSWORD = $previousPassword; $password = $null; $url = $null }
