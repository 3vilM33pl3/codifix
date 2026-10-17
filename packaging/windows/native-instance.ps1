# SPDX-License-Identifier: AGPL-3.0-or-later
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateSet('Preview', 'Install', 'Upgrade')][string]$Action,
    [ValidateSet('Install', 'Upgrade')][string]$PreviewAction = 'Install',
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][guid]$OperationId,
    [Parameter(Mandatory = $true)][string]$PackagePath,
    [Parameter(Mandatory = $true)][string]$ManifestPath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ManifestSha256,
    [Parameter(Mandatory = $true)][string]$ConfigPath,
    [string]$HealthUri,
    [Parameter(Mandatory = $true)][guid]$ExpectedInstanceId,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedInputSha256,
    [ValidateSet('memory.exe', 'memory-federation-gateway.exe','memory-pds-pipe.exe')][string]$ExecutableName = 'memory.exe',
    [guid]$ExpectedSpaceId = [guid]::Empty,
    [guid]$StatusAudienceId = [guid]::Empty,
    [string]$NativeRuntimeManifestPath,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$NativeRuntimeManifestSha256,
    [guid]$NativeRuntimeOperationId = [guid]::Empty,
    [string]$PdsEnvironmentPath,
    [ValidatePattern('^[0-9a-f]{64}$')][string]$PdsEnvironmentSha256,
    [Parameter(Mandatory = $true)][ValidateLength(1, 1024)][string]$RecoveryReference,
    [switch]$DatabaseCompatibilityConfirmed
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
. (Join-Path $PSScriptRoot 'release-identity.ps1')
if ($env:OS -ne 'Windows_NT') { throw 'Native Windows is required; no Docker/WSL fallback exists' }

function Get-Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Assert-NoReparse([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    while ($null -ne $item) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse points are forbidden in operator paths' }
        $parent = Split-Path -Parent $item.FullName
        $item = if ($parent -and $parent -ne $item.FullName) { Get-Item -LiteralPath $parent -Force } else { $null }
    }
}
function Set-PrivateDirectory([string]$Path, [string]$ServiceSid = '', [switch]$ReadOnly) {
    Set-NativePrivateDirectory $Path $ServiceSid -ReadOnly:$ReadOnly
}
function Invoke-Sc([string[]]$Arguments) {
    # PowerShell 5.1 native invocation rewrites embedded quotes in binPath.
    # Preserve each sc.exe argument through the shared Windows argv encoder,
    # as the native PostgreSQL installer already does for SCM creation.
    $start=[Diagnostics.ProcessStartInfo]::new()
    $start.FileName=Join-Path $env:SystemRoot 'System32\sc.exe'
    $start.Arguments=($Arguments | ForEach-Object {ConvertTo-NativeCommandArgument $_}) -join ' '
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true;$start.WorkingDirectory=$operationRoot
    $process=[Diagnostics.Process]::new();$process.StartInfo=$start
    try{
        if(-not $process.Start()){throw 'Exact SCM process failed to start'}
        $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
        if(-not $process.WaitForExit(30000)){
            $process.Kill();$null=$process.WaitForExit(10000)
            throw 'Exact SCM process exceeded deadline; inspect retained operation before retry'
        }
        $process.WaitForExit();$out=$stdout.GetAwaiter().GetResult();$err=$stderr.GetAwaiter().GetResult()
        $code=$process.ExitCode
        if($out.Length+$err.Length -gt 64KB){throw 'SCM output exceeded its bound; inspect retained operation'}
        # Deliberately discard native stdout/stderr. Only the static verb and
        # numeric OS outcome may enter the private operation journal.
    }finally{$process.Dispose()}
    if ($code -ne 0) {
        $journal.scm_failure=Get-NativeScmFailure $Arguments[0] $code
        Write-NativeJournal $journalPath $journal
        throw "SCM $($journal.scm_failure.operation) failed with code $code; inspect the private operation journal"
    }
}
function Get-SelectedService { return Get-CimInstance Win32_Service -Filter "Name='$($plan.service_name)'" }
function Save-Phase([string]$Phase) {
    if (@($journal.history).Count -ge 1000) { throw 'Operation journal attempt limit reached; inspect retained evidence' }
    $journal.phase = $Phase
    $journal.history = @($journal.history) + @([ordered]@{ phase = $Phase; observed_at = [DateTime]::UtcNow.ToString('o') })
    Write-NativeJournal $journalPath $journal
}

# Read a bounded exact file, holding a deny-write/delete handle until parsed.
Assert-NoReparse $ManifestPath
$manifestFile = [IO.File]::Open($ManifestPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    if ($manifestFile.Length -gt 8MB) { throw 'Manifest too large' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $actualManifestHash = ([BitConverter]::ToString($sha.ComputeHash($manifestFile))).Replace('-', '').ToLowerInvariant() } finally { $sha.Dispose() }
    if ($actualManifestHash -ne $ManifestSha256) { throw 'Manifest digest differs from reviewed input' }
    $manifestFile.Position = 0
    $reader = [IO.StreamReader]::new($manifestFile, [Text.Encoding]::UTF8, $true, 4096, $true)
    try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
} finally { $manifestFile.Dispose() }
if ($manifest.schema_version -ne 1 -or $manifest.platform -ne 'windows-x86_64') { throw 'Unsupported candidate manifest' }
$identity = Get-WindowsReleaseIdentity $manifest.application_version $manifest.installer_build
if ($identity.release_identity -ne $manifest.release_identity -or $identity.installer_version -ne $manifest.installer_version) { throw 'Candidate identities disagree' }
if (-not $manifest.payload_files -or @($manifest.payload_files).Count -gt 20000) { throw 'Manifest needs complete bounded payload hashes' }
if ($OperationId -eq [guid]::Empty) { throw 'A nonzero operation UUID is required' }
$manifestNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($entry in $manifest.payload_files) {
    Assert-NativeArchiveName $entry.name
    if (-not $manifestNames.Add($entry.name) -or $entry.sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Duplicate or malformed payload entry' }
}
$plan = Get-NativeInstancePlan $Name $manifest.release_identity $ExecutableName $HealthUri $ExpectedInstanceId $ExpectedSpaceId $StatusAudienceId
$zipName = Split-Path -Leaf $PackagePath
$zipEntry = @($manifest.artifacts | Where-Object { $_.name -ceq $zipName -and $_.name.EndsWith('.zip') })
if ($zipEntry.Count -ne 1) { throw 'ZIP must match exactly one reviewed manifest artifact' }
Assert-NoReparse $PackagePath
if ((Get-Hash $PackagePath) -ne $zipEntry[0].sha256) { throw 'ZIP hash differs from manifest' }
Assert-NoReparse $ConfigPath
$configFile = [IO.File]::Open($ConfigPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
try {
    if ($configFile.Length -gt 1MB) { throw 'Configuration too large' }
    $buffer = [IO.MemoryStream]::new()
    try { $configFile.CopyTo($buffer); $configBytes = $buffer.ToArray() } finally { $buffer.Dispose() }
} finally { $configFile.Dispose() }
$configHash = Get-NativeInputDigest ([Convert]::ToBase64String($configBytes))
if ($ExecutableName -eq 'memory-federation-gateway.exe') {
    $gatewayConfig = [Text.Encoding]::UTF8.GetString($configBytes) | ConvertFrom-Json
    Assert-NativeGatewayConfig $gatewayConfig $ExpectedSpaceId $ExpectedInstanceId $StatusAudienceId
}
$root = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'MemoryLayer'
$releasesRoot = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Memory Layer\releases'
$release = Join-Path $releasesRoot $plan.release_identity
$state = Join-Path (Join-Path $root 'instances') $Name
$configurationRoot = Join-Path $root 'configurations'
$configurationDirectory = Join-Path $configurationRoot $Name
$installedConfig = Join-Path $configurationDirectory $(if ($ExecutableName -eq 'memory.exe') { 'memory-layer.toml' } elseif($ExecutableName -eq 'memory-pds-pipe.exe'){'pds-pipe.json'} else { 'gateway.json' })
$command = @((Join-Path $release 'bin\memory-windows-service.exe'), '--name', $plan.service_name, '--directory', $state, '--', (Join-Path $release "bin\$ExecutableName")) + @(Get-NativeServiceArguments $ExecutableName $installedConfig)
$binaryPath = ($command | ForEach-Object { ConvertTo-NativeCommandArgument $_ }) -join ' '
$operationAction = if ($Action -eq 'Preview') { $PreviewAction } else { $Action }
$binding = [ordered]@{ action = $operationAction; recovery_reference = $RecoveryReference; plan = $plan; manifest_sha256 = $ManifestSha256; package_sha256 = $zipEntry[0].sha256; config_fingerprint = $configHash; binary_path = $binaryPath }
$runtimeBinding=$null
$runtimeKind=if($ExecutableName -ceq 'memory-pds-pipe.exe'){'memory-native-pds-runtime'}else{'memory-native-postgres-runtime'}
if ($NativeRuntimeManifestPath -or $NativeRuntimeManifestSha256 -or $NativeRuntimeOperationId -ne [guid]::Empty) {
    if (-not $NativeRuntimeManifestPath -or -not $NativeRuntimeManifestSha256 -or $NativeRuntimeOperationId -eq [guid]::Empty) { throw 'Local native runtime binding requires manifest path, digest and verified operation ID together' }
    $previewLease=Open-NativePrerequisiteLease $NativeRuntimeManifestPath $NativeRuntimeManifestSha256 $NativeRuntimeOperationId $runtimeKind
    try {
        $runtimeBinding=$previewLease.binding; $binding.native_runtime=$runtimeBinding
        if($ExecutableName -ceq 'memory-pds-pipe.exe'){
            $pdsConfig=[Text.Encoding]::UTF8.GetString($configBytes) | ConvertFrom-Json
            Assert-NativePdsServiceConfig $pdsConfig $runtimeBinding $previewLease.plan $Name $state $configurationDirectory $PdsEnvironmentSha256
        }
    }
    finally { Close-NativePrerequisiteLease $previewLease }
}
$pdsEnvironmentBytes=$null
if($ExecutableName -ceq 'memory-pds-pipe.exe'){
    if(-not $runtimeBinding -or -not $PdsEnvironmentPath -or -not $PdsEnvironmentSha256){throw 'Native PDS requires exact patched runtime and protected environment inputs'}
    Assert-NoReparse $PdsEnvironmentPath
    $environmentGuard=[MemoryLayer.ProtectedRelease]::Open((Split-Path -Parent ([IO.Path]::GetFullPath($PdsEnvironmentPath))))
    try{
        $allowed=@('S-1-5-18','S-1-5-32-544',[Security.Principal.WindowsIdentity]::GetCurrent().User.Value,$pdsConfig.pipe.server_sid)
        $acl=Get-Acl -LiteralPath $PdsEnvironmentPath
        if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $allowed){throw 'Private PDS environment owner differs'}
        foreach($ace in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])){
            if($ace.AccessControlType -eq 'Allow' -and [int64]$ace.FileSystemRights -ne 0 -and $ace.IdentityReference.Value -notin $allowed){throw 'Private PDS environment has another reader'}
        }
        if((Get-Item -LiteralPath $PdsEnvironmentPath).Length -gt 64KB -or (Get-Hash $PdsEnvironmentPath) -cne $PdsEnvironmentSha256){throw 'Private PDS environment differs from reviewed digest'}
        $pdsEnvironmentBytes=[IO.File]::ReadAllBytes($PdsEnvironmentPath)
        Assert-NativePdsEnvironment ([Text.Encoding]::UTF8.GetString($pdsEnvironmentBytes) | ConvertFrom-Json) $state
    }finally{$environmentGuard.Dispose()}
    $binding.pds_environment_sha256=$PdsEnvironmentSha256
}elseif($PdsEnvironmentPath -or $PdsEnvironmentSha256){throw 'PDS environment inputs do not apply to other service roles'}
$inputDigest = Get-NativeInputDigest $binding
if ($Action -eq 'Preview') {
    [ordered]@{ action = $operationAction; recovery_reference = $RecoveryReference; plan = $plan; native_runtime=$runtimeBinding; input_sha256 = $inputDigest; release_directory = $release; state_directory = $state; signing_status = $manifest.signing_status; native_stack_bundled = $manifest.native_stack_bundled; database_backup_performed = $false } | ConvertTo-Json -Depth 8
    exit 0
}
if (-not $ExpectedInputSha256 -or $ExpectedInputSha256 -ne $inputDigest) { throw 'Execution inputs differ from the explicitly reviewed preview digest' }
if (-not $DatabaseCompatibilityConfirmed -or -not $RecoveryReference -or $RecoveryReference.Length -gt 1024) {
    throw 'Execution requires an operator recovery reference and confirmed database compatibility; this adapter neither backs up nor rolls back databases'
}
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Run execution in an elevated PowerShell' }
# Exact machine-owned roots only; never alter another installation or user data.
foreach ($path in @($root, (Join-Path $root 'instances'), $configurationRoot, (Split-Path -Parent $releasesRoot), $releasesRoot)) { Set-NativePrivateDirectory $path -SharedAncestor }
foreach ($path in @((Join-Path $root 'operations'), (Join-Path $root 'registrations'))) { Set-PrivateDirectory $path }
$registrationPath = Join-Path (Join-Path $root 'registrations') "$Name.json"
$operationRoot = Join-Path (Join-Path $root 'operations') $OperationId.ToString()
Set-PrivateDirectory $operationRoot
$lock = [IO.File]::Open((Join-Path $root 'operator.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
$releaseGuard = $null
$runtimeLease = $null
try {
    # Retain exact runtime/receipt handles through service changes. This is file
    # identity verification, not proof that PostgreSQL is running or reachable.
    if ($runtimeBinding) {
        $runtimeLease=Open-NativePrerequisiteLease $NativeRuntimeManifestPath $NativeRuntimeManifestSha256 $NativeRuntimeOperationId $runtimeKind
        if ((Get-NativeInputDigest $runtimeLease.binding) -cne (Get-NativeInputDigest $runtimeBinding)) { throw 'Native runtime changed after preview' }
    }
    $journalPath = Join-Path $operationRoot 'journal.json'
    if (Test-Path -LiteralPath $journalPath) {
        $journal = Get-Content -LiteralPath $journalPath -Raw | ConvertFrom-Json
        Assert-NativeRetry $journal $inputDigest
    } else {
        $existing = Get-SelectedService
        if ($Action -eq 'Install' -and $existing) { throw 'Existing named service requires Upgrade' }
        if ($Action -eq 'Upgrade' -and -not $existing) { throw 'Named service does not exist; use Install' }
        if ($Action -eq 'Install' -and (Test-Path -LiteralPath $registrationPath)) { throw 'Prior installation receipt exists; reconcile it before reinstalling' }
        if ($Action -eq 'Upgrade') {
            if (-not (Test-Path -LiteralPath $registrationPath)) { throw 'Existing service has no operator installation receipt; explicit legacy adoption is not implemented' }
            $registration = Get-Content -LiteralPath $registrationPath -Raw | ConvertFrom-Json
            if ($registration.native_runtime -and -not $runtimeBinding) { throw 'Upgrade cannot silently discard a prior native runtime binding' }
            if ($registration.binary_path -ne $existing.PathName -or $registration.expected_instance_id -ne $plan.expected_instance_id) { throw 'Existing service differs from its exact-instance installation receipt' }
            if ([version]$manifest.installer_version -le [version]$registration.installer_version) { throw 'Upgrade requires a strictly newer installer build; stale executable recovery is forbidden' }
        }
        $journal = [ordered]@{ schema_version = 1; operation_id = $OperationId.ToString(); action = $operationAction; input_sha256 = $inputDigest; phase = 'prepared'; history = @(); retained_staging = @(); plan = $plan; previous_binary_path = $(if ($existing) { $existing.PathName } else { $null }); recovery_reference = $RecoveryReference; database_backup_performed = $false }
        Write-NativeJournal $journalPath $journal
    }
    if ($journal.phase -eq 'completed') { Write-Output "Already completed: $OperationId ($($plan.service_name)); no service restarted"; exit 0 }
    $existing = Get-SelectedService
    if ($existing -and ($existing.StartName -ne $plan.account -or $existing.PathName -notin @($journal.previous_binary_path, $binaryPath))) { throw 'SCM configuration changed outside this operation' }
    Initialize-NativeReleaseGuard
    if (Test-Path -LiteralPath $release) {
        # Refuse even correctly hashed bytes if another identity can modify,
        # replace or alias them. Retain validated handles until startup finishes.
        $releaseGuard = [MemoryLayer.ProtectedRelease]::Open($release)
        Assert-NativePayload $release $manifest.payload_files
    }
    if (Test-Path -LiteralPath $installedConfig) {
        Assert-NoReparse $installedConfig
        $existingBytes = [IO.File]::ReadAllBytes($installedConfig)
        if ((Get-NativeInputDigest ([Convert]::ToBase64String($existingBytes))) -ne $configHash) { throw 'Upgrade may not modify instance configuration; review a separate configuration change' }
    }
    if ($existing -and $existing.State -ne 'Stopped') {
        Stop-Service -Name $plan.service_name
        (Get-Service -Name $plan.service_name).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30))
    }
    $existingSid = if ($existing) { ([Security.Principal.NTAccount]::new($plan.account)).Translate([Security.Principal.SecurityIdentifier]).Value } else { '' }
    Set-PrivateDirectory $state $existingSid
    Save-Phase 'service_stopped'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    if (-not (Test-Path -LiteralPath $release)) {
        $temporaryRelease = Join-Path $releasesRoot ($plan.release_identity + '.staging-' + $OperationId)
        if (Test-Path -LiteralPath $temporaryRelease) {
            Assert-NoReparse $temporaryRelease
            $retained = $temporaryRelease + '.interrupted-' + [guid]::NewGuid().ToString()
            [IO.Directory]::Move($temporaryRelease, $retained)
            $journal.retained_staging = @($journal.retained_staging) + @($retained)
            Save-Phase 'interrupted_staging_retained'
        }
        Set-PrivateDirectory $temporaryRelease
        Save-Phase 'staging_created'
        $archive = [IO.Compression.ZipFile]::OpenRead($PackagePath)
        try {
            if ($archive.Entries.Count -gt 20000) { throw 'Too many ZIP entries' }
            $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            $total = [int64]0
            foreach ($entry in $archive.Entries) {
                Assert-NativeArchiveName $entry.FullName
                if (-not $seen.Add($entry.FullName.TrimEnd('/'))) { throw 'Case-insensitive or duplicate archive path' }
                $total += $entry.Length
                if ($total -gt 2GB) { throw 'Expanded release exceeds 2GiB' }
                $target = Join-Path $temporaryRelease $entry.FullName.Replace('/', '\')
                if ($entry.FullName.EndsWith('/')) { New-Item -ItemType Directory -Path $target -Force | Out-Null; continue }
                $null = New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force
                $source = $entry.Open(); $output = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
                try { $source.CopyTo($output); $output.Flush($true) } finally { $output.Dispose(); $source.Dispose() }
            }
        } finally { $archive.Dispose() }
        $stagingGuard = $null
        try {
            $stagingGuard = [MemoryLayer.ProtectedRelease]::Open($temporaryRelease)
            Assert-NativePayload $temporaryRelease $manifest.payload_files
        } catch {
            $journal.retained_staging = @($journal.retained_staging) + @($temporaryRelease)
            Save-Phase 'staging_rejected'
            throw
        } finally { if ($stagingGuard) { $stagingGuard.Dispose() } }
        # The private staging tree is fully verified before it becomes an
        # immutable release. A failed validation leaves it recoverably staged.
        [IO.Directory]::Move($temporaryRelease, $release)
        $releaseGuard = [MemoryLayer.ProtectedRelease]::Open($release)
    }
    Assert-NativePayload $release $manifest.payload_files
    if (-not (Test-Path (Join-Path $release 'bin\memory-windows-service.exe')) -or -not (Test-Path (Join-Path $release "bin\$ExecutableName"))) { throw 'Required native executables missing' }
    Save-Phase 'release_verified'
    if (-not $existing) { Invoke-Sc @('create', $plan.service_name, 'binPath=', $binaryPath, 'start=', 'demand', 'obj=', $plan.account) }
    else { Invoke-Sc @('config', $plan.service_name, 'binPath=', $binaryPath, 'start=', 'demand') }
    Invoke-Sc @('sidtype', $plan.service_name, 'unrestricted')
    $sid = ([Security.Principal.NTAccount]::new($plan.account)).Translate([Security.Principal.SecurityIdentifier]).Value
    if($ExecutableName -ceq 'memory-pds-pipe.exe' -and $pdsConfig.pipe.server_sid -cne $sid){throw 'Configured PDS pipe server SID differs from exact SCM virtual account; service remains stopped'}
    foreach ($path in @($root, (Join-Path $root 'instances'), $configurationRoot, (Split-Path -Parent $releasesRoot), $releasesRoot)) {
        Set-NativePrivateDirectory $path $sid -SharedAncestor
    }
    Set-PrivateDirectory $state $sid
    Set-NativePrivateDirectory $configurationDirectory $sid -ReadOnly
    # Shared release bytes are readable but not writable by virtual services.
    $readRule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'ReadAndExecute', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $releaseAcl = Get-Acl -LiteralPath $release; $releaseAcl.AddAccessRule($readRule); Set-Acl -LiteralPath $release -AclObject $releaseAcl
    if($ExecutableName -ceq 'memory-pds-pipe.exe'){
        $runtimeDirectory=$runtimeBinding.runtime_directory
        $runtimeRoot=Split-Path -Parent $runtimeDirectory
        foreach($ancestor in @((Split-Path -Parent $runtimeRoot),$runtimeRoot)){Set-NativePrivateDirectory $ancestor $sid -SharedAncestor}
        $acl=Get-Acl -LiteralPath $runtimeDirectory;$acl.AddAccessRule($readRule);Set-Acl -LiteralPath $runtimeDirectory -AclObject $acl
        $environmentTarget=Join-Path $configurationDirectory 'pds-environment.json'
        if(Test-Path -LiteralPath $environmentTarget){
            Assert-NoReparse $environmentTarget
            if((Get-Hash $environmentTarget) -cne $PdsEnvironmentSha256){throw 'Existing private PDS environment differs; explicit configuration change required'}
        }else{
            $file=[IO.File]::Open($environmentTarget,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            try{$file.Write($pdsEnvironmentBytes,0,$pdsEnvironmentBytes.Length);$file.Flush($true)}finally{$file.Dispose()}
        }
    }
    if (-not (Test-Path -LiteralPath $installedConfig)) {
        $temporaryConfig = Join-Path $configurationDirectory ([guid]::NewGuid().ToString() + '.config.tmp')
        $file = [IO.File]::Open($temporaryConfig, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $file.Write($configBytes, 0, $configBytes.Length); $file.Flush($true) } finally { $file.Dispose() }
        [IO.File]::Move($temporaryConfig, $installedConfig)
    }
    Save-Phase 'service_configured'
    Start-Service -Name $plan.service_name
    (Get-Service -Name $plan.service_name).WaitForStatus('Running', [TimeSpan]::FromSeconds(30))
    Save-Phase 'process_running'
    $ready = $false
    if ($ExecutableName -eq 'memory-federation-gateway.exe') {
        $probe = Join-Path $release 'bin\memory-federation-gateway.exe'
        for ($attempt = 0; $attempt -lt 30; $attempt++) {
            try {
                # Reuse Rust's existing signed nonce/key/freshness verifier.
                # Never place the enrolled bearer on argv or operational output.
                $output = @(& $probe check-status $installedConfig $ExpectedSpaceId.ToString() $ExpectedInstanceId.ToString() $StatusAudienceId.ToString() --service-name $plan.service_name 2>$null)
                $probeExit = $LASTEXITCODE
                $status = ($output -join "`n") | ConvertFrom-Json
                if ($probeExit -eq 0 -and (Test-NativeGatewayReadiness $status $ExpectedSpaceId $ExpectedInstanceId $StatusAudienceId) -and (Get-Service -Name $plan.service_name).Status -eq 'Running') { $ready = $true; break }
            } catch { }
            Start-Sleep -Seconds 1
        }
    } elseif($ExecutableName -eq 'memory-pds-pipe.exe') {
        $probe=Join-Path $release 'bin\memory-pds-pipe.exe'
        for($attempt=0;$attempt -lt 30;$attempt++){
            try{
                $output=@(& $probe check-status $installedConfig --service-name $plan.service_name 2>$null)
                $probeExit=$LASTEXITCODE;$status=($output -join "`n") | ConvertFrom-Json
                if($probeExit -eq 0 -and (Test-NativePdsReadiness $status $sid $pdsConfig.pipe.client_sid $pdsConfig.pipe.name) -and
                    (Get-Service -Name $plan.service_name).Status -eq 'Running'){$ready=$true;break}
            }catch{}
            Start-Sleep -Seconds 1
        }
    } else {
    Add-Type -AssemblyName System.Net.Http
    $handler = [Net.Http.HttpClientHandler]::new()
    $handler.UseProxy = $false
    $handler.AllowAutoRedirect = $false
    $client = [Net.Http.HttpClient]::new($handler)
    $client.Timeout = [TimeSpan]::FromSeconds(2)
    $client.MaxResponseContentBufferSize = 65536
    try {
        for ($attempt = 0; $attempt -lt 30; $attempt++) {
            $response = $null
            try {
                $response = $client.GetAsync($plan.health_uri).GetAwaiter().GetResult()
                $health = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult() | ConvertFrom-Json
                if ([int]$response.StatusCode -eq 200 -and (Test-NativeReadinessHealth $health $ExpectedInstanceId) -and (Get-Service -Name $plan.service_name).Status -eq 'Running') { $ready = $true; break }
            } catch { } finally { if ($response) { $response.Dispose() } }
            Start-Sleep -Seconds 1
        }
    } finally { $client.Dispose(); $handler.Dispose() }
    }
    if (-not $ready) { throw 'Exact-instance readiness failed; no old database or executable was automatically restored' }
    Invoke-Sc @('config', $plan.service_name, 'start=', 'delayed-auto')
    Write-NativeJournal $registrationPath ([ordered]@{ schema_version = 1; operation_id = $OperationId.ToString(); expected_instance_id = $plan.expected_instance_id; installer_version = $manifest.installer_version; release_identity = $plan.release_identity; manifest_sha256 = $ManifestSha256; binary_path = $binaryPath; config_fingerprint = $configHash; native_runtime=$runtimeBinding })
    Save-Phase 'completed'
    $readiness = if ($ExecutableName -eq 'memory.exe') { 'exact-instance readiness verified' } elseif($ExecutableName -eq 'memory-pds-pipe.exe'){'PDS pipe peer and public XRPC verified; repository/account readiness remains separate'} else { 'signed gateway space/authority/current Pull and writer fence verified; PDS publication health is separate' }
    Write-Output "Completed: $OperationId ($($plan.service_name)); $readiness; data preserved"
} finally { if ($releaseGuard) { $releaseGuard.Dispose() }; Close-NativePrerequisiteLease $runtimeLease; $lock.Dispose() }
