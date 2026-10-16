# SPDX-License-Identifier: AGPL-3.0-or-later
# Disposable VM only. This is not the customer prerequisite installer.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ManifestPath)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'Build tools may only be provisioned inside the disposable Memory VM'
}
$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.format -ne 'memory-windows-build-tools-v1') { throw 'Unknown build-tool manifest' }
if ($manifest.rust_toolchain -notmatch '^\d+\.\d+\.\d+$' -or $manifest.wix_version -ne '4.0.5') {
    throw 'Unreviewed compiler or WiX version'
}
$artifacts = @($manifest.artifacts)
$ids = @($artifacts | ForEach-Object { $_.id } | Sort-Object)
if (($ids -join ',') -ne 'dotnet,git,msvc,node,python,rustup') { throw 'Incomplete or duplicate tool set' }
$root = 'C:\MemoryLab\BuildCache'
$tools = 'C:\MemoryLab\Tools'
New-Item -ItemType Directory -Force $root,$tools | Out-Null
& icacls.exe $root /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Build cache ACL failed' }
$receipt = [ordered]@{format='memory-windows-build-tools-result-v1'; manifest_sha256=(Get-FileHash -LiteralPath $ManifestPath -Algorithm SHA256).Hash.ToLowerInvariant(); completed=@(); restart_required=$false}
function Save-Receipt {
    $temp = Join-Path $root 'result.partial.json'
    $receipt | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $temp -Encoding UTF8
    $target = Join-Path $root 'result.json'
    if ([IO.File]::Exists($target)) { [IO.File]::Replace($temp, $target, $null) }
    else { [IO.File]::Move($temp, $target) }
}
function Run-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Native command failed: $([IO.Path]::GetFileName($Exe)) ($LASTEXITCODE)" }
}
function Read-Version([string]$Exe, [string[]]$Arguments) {
    $output = @(& $Exe @Arguments)
    if ($LASTEXITCODE -ne 0 -or $output.Count -ne 1 -or -not $output[0]) {
        throw "Tool version verification failed: $([IO.Path]::GetFileName($Exe))"
    }
    return ([string]$output[0]).Trim()
}
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
foreach ($artifact in $artifacts) {
    if ($artifact.file -notmatch '^[a-zA-Z0-9_.-]+$' -or ([uri]$artifact.url).Scheme -ne 'https') {
        throw 'Unsafe build-tool artifact locator'
    }
    $algorithm = [string]$artifact.algorithm
    if ($algorithm -notin @('SHA256','SHA512') -or $artifact.hash -notmatch '^[0-9a-f]+$' -or
        $artifact.hash.Length -ne $(if ($algorithm -eq 'SHA256') {64} else {128})) {
        throw 'Invalid reviewed artifact digest'
    }
    $path = Join-Path $root $artifact.file
    if (-not (Test-Path -LiteralPath $path)) {
        Invoke-WebRequest -UseBasicParsing -Uri $artifact.url -OutFile ($path + '.partial')
        Move-Item -LiteralPath ($path + '.partial') -Destination $path
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm $algorithm).Hash.ToLowerInvariant() -ne $artifact.hash) {
        throw "Artifact digest mismatch: $($artifact.id); retained for inspection"
    }
    if ($artifact.microsoft_signature) {
        $signature = Get-AuthenticodeSignature -LiteralPath $path
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
            throw 'Build-tools Microsoft signature verification failed'
        }
    }
    $completed = [ordered]@{id=$artifact.id; algorithm=$algorithm; hash=$artifact.hash; status='verified'}
    switch ($artifact.id) {
        'node' {
            $node = Join-Path $tools 'node-v24.18.1-win-x64'
            if (-not (Test-Path (Join-Path $node 'node.exe'))) { Expand-Archive -LiteralPath $path -DestinationPath $tools }
            $env:PATH = "$node;$env:PATH"
            Run-Checked (Join-Path $node 'node.exe') @('--version')
        }
        'git' {
            $git = Join-Path $tools 'git'
            if (-not (Test-Path (Join-Path $git 'cmd\git.exe'))) { Expand-Archive -LiteralPath $path -DestinationPath $git }
            $env:PATH = "$(Join-Path $git 'cmd');$env:PATH"
            Run-Checked (Join-Path $git 'cmd\git.exe') @('--version')
        }
        'python' {
            $python = Join-Path $tools 'python-3.13.9'
            if (-not (Test-Path (Join-Path $python 'python.exe'))) { Expand-Archive -LiteralPath $path -DestinationPath $python }
            Run-Checked (Join-Path $python 'python.exe') @('--version')
        }
        'dotnet' {
            $dotnet = Join-Path $tools 'dotnet'
            if (-not (Test-Path (Join-Path $dotnet 'dotnet.exe'))) { Expand-Archive -LiteralPath $path -DestinationPath $dotnet }
            $env:DOTNET_ROOT = $dotnet
            $env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
            $env:PATH = "$dotnet;$env:USERPROFILE\.dotnet\tools;$env:PATH"
            Run-Checked (Join-Path $dotnet 'dotnet.exe') @('--version')
        }
        'msvc' {
            $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
            $installed = if (Test-Path $vswhere) { & $vswhere -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath }
            if (-not $installed) {
                $process = Start-Process -FilePath $path -ArgumentList @('--quiet','--wait','--norestart','--nocache','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended','--installPath','C:\MemoryLab\BuildTools') -Wait -PassThru
                if ($process.ExitCode -notin @(0,3010)) { throw "MSVC installation failed ($($process.ExitCode))" }
                $receipt.restart_required = ($process.ExitCode -eq 3010)
            }
            $installed = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if ($LASTEXITCODE -ne 0 -or -not $installed -or
                -not (Test-Path -LiteralPath (Join-Path $installed 'VC\Auxiliary\Build\vcvars64.bat'))) {
                throw 'MSVC installation did not provide the required native toolchain'
            }
            $completed.installation_path = $installed
            $completed.installation_version = Read-Version $vswhere @('-latest','-products','*','-requires','Microsoft.VisualStudio.Component.VC.Tools.x86.x64','-property','installationVersion')
        }
        'rustup' {
            if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\rustup.exe")) {
                Run-Checked $path @('-y','--profile','minimal','--default-host','x86_64-pc-windows-msvc','--default-toolchain',$manifest.rust_toolchain)
            }
            $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
            Run-Checked "$env:USERPROFILE\.cargo\bin\rustup.exe" @('toolchain','install',$manifest.rust_toolchain,'--profile','minimal','--component','clippy,rustfmt')
            Run-Checked "$env:USERPROFILE\.cargo\bin\rustup.exe" @('default',$manifest.rust_toolchain)
        }
    }
    $completed.status = 'installed'
    $receipt.completed += $completed
    Save-Receipt
}
$env:DOTNET_ROOT = Join-Path $tools 'dotnet'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_ROLL_FORWARD = 'Major'
$env:PATH = "$env:DOTNET_ROOT;$env:USERPROFILE\.dotnet\tools;$env:USERPROFILE\.cargo\bin;$(Join-Path $tools 'git\cmd');$(Join-Path $tools 'node-v24.18.1-win-x64');$env:PATH"
if (-not (Test-Path "$env:USERPROFILE\.dotnet\tools\wix.exe")) {
    Run-Checked (Join-Path $env:DOTNET_ROOT 'dotnet.exe') @('tool','install','--global','wix','--version',$manifest.wix_version)
}
$receipt.rust = Read-Version 'rustc.exe' @('--version')
$receipt.cargo = Read-Version 'cargo.exe' @('--version')
$receipt.node = Read-Version 'node.exe' @('--version')
$receipt.git = Read-Version 'git.exe' @('--version')
$receipt.python = Read-Version (Join-Path $tools 'python-3.13.9\python.exe') @('--version')
$receipt.dotnet = Read-Version 'dotnet.exe' @('--version')
$receipt.wix = Read-Version 'wix.exe' @('--version')
if ($receipt.rust -notmatch ('^rustc ' + [regex]::Escape($manifest.rust_toolchain) + ' ') -or
    $receipt.node -ne 'v24.18.1' -or $receipt.python -ne 'Python 3.13.9' -or
    $receipt.dotnet -ne '10.0.402' -or $receipt.wix -notmatch '^4\.0\.5(?:[.+-]|$)') {
    throw 'Installed tool version differs from the reviewed build baseline'
}
$receipt.completed_at = [DateTime]::UtcNow.ToString('o')
Save-Receipt
$receipt | ConvertTo-Json -Depth 12
