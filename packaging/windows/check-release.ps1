# SPDX-License-Identifier: AGPL-3.0-or-later
# Packaging acceptance only, not complete native-stack or supervisor parity.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$DistDir,
    [Parameter(Mandatory = $true)][string]$ExpectedVersion,
    [Parameter(Mandatory = $true)][string]$ExpectedCommit,
    [Parameter(Mandatory = $true)][string]$TestDatabaseUrl,
    [ValidateRange(1,65535)][int]$InstallerBuild = 1
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Refusing MSI installation outside a disposable GitHub Windows runner'
}
. (Join-Path $PSScriptRoot 'release-identity.ps1')
$expectedIdentity = Get-WindowsReleaseIdentity -ApplicationVersion $ExpectedVersion -InstallerBuild $InstallerBuild
$DistDir = (Resolve-Path -LiteralPath $DistDir).Path
$manifests = @(Get-ChildItem -LiteralPath $DistDir -Filter '*.manifest.json')
if ($manifests.Count -ne 1) { throw 'Expected exactly one Windows release manifest' }
$manifest = Get-Content -LiteralPath $manifests[0].FullName -Raw | ConvertFrom-Json
if ($manifest.application_version -ne $ExpectedVersion -or $manifest.source_commit -ne $ExpectedCommit -or
    $manifest.native_stack_bundled -ne $false -or $manifest.installer_build -ne $InstallerBuild -or
    $manifest.installer_version -ne $expectedIdentity.installer_version -or $manifest.signing_status -ne 'unsigned') {
    throw 'Unexpected stable Windows preview identity or capability claim'
}
foreach ($artifact in $manifest.artifacts) {
    if ([IO.Path]::GetFileName($artifact.name) -cne $artifact.name) { throw 'Unsafe artifact name' }
    $path = Join-Path $DistDir $artifact.name
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $artifact.sha256 -or
        (Get-Item -LiteralPath $path).Length -ne $artifact.bytes) { throw 'Artifact checksum/length mismatch' }
}
$stage = Join-Path $env:RUNNER_TEMP ('codifix-release-check-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $stage | Out-Null
$zip = @($manifest.artifacts | Where-Object { $_.name.EndsWith('.zip') })
if ($zip.Count -ne 1) { throw 'Expected one portable ZIP' }
Expand-Archive -LiteralPath (Join-Path $DistDir $zip[0].name) -DestinationPath $stage
foreach ($file in $manifest.payload_files) {
    $path = [IO.Path]::GetFullPath((Join-Path $stage $file.name))
    if (-not $path.StartsWith($stage + '\', [StringComparison]::OrdinalIgnoreCase) -or
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $file.sha256) {
        throw 'Extracted payload mismatch'
    }
}
function Check-Codifix([string]$Binary) {
    $version = & $Binary --version
    if ($LASTEXITCODE -ne 0 -or $version -cne "codifix $ExpectedVersion") { throw 'Wrong packaged version' }
    foreach ($arguments in @(@('--help'), @('federation', '--help'), @('auth', '--help'), @('urls', '--help'))) {
        & $Binary @arguments | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Packaged CLI help failed' }
    }
}
Check-Codifix (Join-Path $stage 'bin\codifix.exe')
# Exact disposable native PostgreSQL database, provisioned by the caller. Do not
# discover server credentials or use an installed application's configuration.
$database = [uri]$TestDatabaseUrl
if ($database.Scheme -notin @('postgres', 'postgresql') -or $database.Host -notin @('127.0.0.1', 'localhost') -or
    $database.AbsolutePath -notmatch '^/codifix_release_[a-z0-9_]+$') { throw 'Explicit disposable loopback database required' }
$token = [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
$origin = "http://127.0.0.1:$port"
$config = Join-Path $stage 'test-config.toml'
$webRoot = Join-Path $stage 'share\codifix\web'
@"
[service]
bind_addr = '127.0.0.1:$port'
api_token = '$token'
web_root = '$webRoot'
[database]
url = '$TestDatabaseUrl'
[features]
llm_curation = false
[embeddings]
enabled = false
create_enabled = false
[background_work]
enabled = false
[cluster]
enabled = false
[automation]
enabled = false
[offline]
enabled = false
[reinforcement]
enabled = false
[provenance]
reverify_enabled = false
[consolidation]
enabled = false
[procedural]
enabled = false
[mcp]
enabled = false
http_enabled = false
[telemetry]
enabled = false
"@ | Set-Content -LiteralPath $config -Encoding utf8
$headers = @{ Authorization = "Bearer $token" }
function Request([string]$Path, $Body = $null) {
    if ($null -eq $Body) { return Invoke-RestMethod -Uri ($origin + $Path) -Headers $headers -TimeoutSec 15 }
    return Invoke-RestMethod -Uri ($origin + $Path) -Headers $headers -Method Post -ContentType 'application/json' -Body ($Body | ConvertTo-Json -Depth 12) -TimeoutSec 30
}
$binary = Join-Path $stage 'bin\codifix.exe'
for ($generation = 1; $generation -le 2; $generation++) {
    $service = Start-Process -FilePath $binary -ArgumentList @('--config', "`"$config`"", 'service', 'run') -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $stage "service-$generation.stdout.log") -RedirectStandardError (Join-Path $stage "service-$generation.stderr.log")
    try {
        $ready = $false
        for ($attempt = 0; $attempt -lt 120; $attempt++) {
            if ($service.HasExited) { break }
            try { $null = Request '/healthz'; $ready = $true; break } catch { Start-Sleep -Milliseconds 500 }
        }
        if (-not $ready) { throw 'Packaged service failed readiness; inspect service logs' }
        if ($generation -eq 1) {
            $capture = @{project='release-smoke'; task_title='Synthetic release capture'; user_prompt='Remember the synthetic backup port'; writer_id='release-test'
                agent_summary='The synthetic backup listener uses port 43117.'; idempotency_key=[guid]::NewGuid().ToString()
                structured_candidates=@(@{canonical_text='The synthetic backup listener uses port 43117.'; summary='Synthetic backup port'; memory_type='implementation'; confidence=0.95; importance=4
                    tags=@('synthetic'); sources=@(@{source_kind='note'; excerpt='The synthetic backup listener uses port 43117.'})})}
            $first = Request '/v1/capture/task' $capture
            $again = Request '/v1/capture/task' $capture
            if ($first.raw_capture_id -ne $again.raw_capture_id) { throw 'Capture replay duplicated data' }
            $null = Request '/v1/curate' @{project='release-smoke';raw_capture_id=$first.raw_capture_id}
        }
        $browse = Request '/v1/projects/release-smoke/memories'
        if (@($browse.items | Where-Object { $_.preview -match '43117' }).Count -ne 1) { throw 'Curated memory missing or duplicated' }
        $answer = Request '/v1/query' @{project='release-smoke';query='synthetic backup port';retrieval_mode='lexical';answer_mode='deterministic'}
        if (($answer.results | ConvertTo-Json -Depth 16) -notmatch '43117') { throw 'Packaged native search lost curated data' }
        $page = Invoke-WebRequest -Uri $origin -Headers $headers -TimeoutSec 15
        if ($page.Content -notmatch '<html|<!doctype') { throw 'Packaged Web root did not serve HTML' }
    } finally {
        if (-not $service.HasExited) { Stop-Process -Id $service.Id -Force; $service.WaitForExit() }
        $service.Dispose()
    }
}
function Invoke-Msi([string[]]$Arguments) {
    $process = Start-Process msiexec.exe -Wait -PassThru -ArgumentList $Arguments
    if ($process.ExitCode -notin @(0, 3010)) { throw "MSI failed with exit $($process.ExitCode)" }
}
foreach ($artifact in @($manifest.artifacts | Where-Object { $_.name.EndsWith('.msi') })) {
    $package = Join-Path $DistDir $artifact.name
    $install = if ($artifact.name.EndsWith('-server-tools.msi')) {
        Join-Path $env:ProgramFiles 'Codifix Native Server Tools'
    } else { Join-Path $env:LOCALAPPDATA 'Programs\Codifix' }
    # User-created data is outside the package component inventory and must
    # survive both repair and uninstall; no database or service is activated.
    Invoke-Msi @('/i', "`"$package`"", '/qn', '/norestart', '/l*v', "`"$stage\install-$($artifact.name).log`"")
    Check-Codifix (Join-Path $install 'bin\codifix.exe')
    $sentinel = Join-Path $install 'release-user-data-sentinel.txt'
    [IO.File]::WriteAllText($sentinel, 'preserved user data')
    Invoke-Msi @('/fa', "`"$package`"", '/qn', '/norestart')
    Check-Codifix (Join-Path $install 'bin\codifix.exe')
    Invoke-Msi @('/x', "`"$package`"", '/qn', '/norestart')
    if ([IO.File]::ReadAllText($sentinel) -cne 'preserved user data') { throw 'MSI removed user data' }
    if (Test-Path -LiteralPath (Join-Path $install 'bin\codifix.exe')) { throw 'MSI executable remained after uninstall' }
}
Write-Host "Portable ZIP, per-user MSI and server-tools MSI checks passed; logs: $stage"
