# SPDX-License-Identifier: AGPL-3.0-or-later
# Thin disposable-VM adapter around the existing pinned native build operator.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Prepare','Verify')][string]$Action,
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$SourceRoot,
    [Parameter(Mandatory=$true)][guid]$OperationId
)
$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Native Pi proof is restricted to the disposable AMD64 Memory VM' }
if ($OperationId -eq [guid]::Empty -or $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)*$' -or $SourceRoot.Split('\') -contains '..') { throw 'Exact operation and copied lab Memory source required' }
function Assert-NoReparse([string]$Path) {
    $current=[IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item=Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Pi proof rejects reparse-point paths and ancestors' }
        }
        $parent=[IO.Path]::GetDirectoryName($current)
        if ($parent -eq $current) { break }; $current=$parent
    }
}
Assert-NoReparse $SourceRoot; Assert-NoReparse $ManifestPath
$manifest=Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.format -cne 'memory-windows-pi-proof-v1' -or $manifest.source_revision -cne '7c018475d54d4e09c907691e47a520f1f09e3288' -or
    $manifest.source_git_url -cne 'https://github.com/Dicklesworthstone/pi_agent_rust.git' -or
    $manifest.source_archive_sha256 -cne '5408d25ab40e49ce42ea1622c3bdd0128f13dacc53e11e3548cacf48d8908bbc' -or
    $manifest.upstream_lock_sha256 -cne '1aea4e3fe13a86760317d9c3da80d7461bd5cd5c013508ab5446a391c7fa14e5' -or
    $manifest.patched_lock_sha256 -cne '362e708ba62267ca061a7e8172f663a4994b78990141d383879198132c74dfd6' -or
    $manifest.rust_toolchain -cne 'nightly-2026-08-31' -or $manifest.rustc_commit -cne '90850177249efe0321573c569aec5d12b257f8d6' -or
    $manifest.target -cne 'x86_64-pc-windows-msvc') { throw 'Unreviewed native Pi baseline' }
function Assert-Hash([string]$Path,[string]$Hash) {
    Assert-NoReparse $Path
    if ($Hash -cnotmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Hash) { throw 'Native Pi input differs from reviewed digest' }
}
function Run([string]$Program,[string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Native Pi command failed: $([IO.Path]::GetFileName($Program)) ($LASTEXITCODE); preserve this attempt" }
}
$expected=@('crates/mem-supervisor-pi/scripts/build-windows.ps1','crates/mem-supervisor-pi/patches/no-tools-1.patch',
    'crates/mem-supervisor-pi/patches/windows-broker-1.patch','crates/mem-supervisor-pi/patches/LICENSE.pi',
    'packaging/ca-bookworm/pi_smoke.py','packaging/windows-pi/smoke.py',
    'crates/mem-supervisor-pi/patches/windows-broker-safe-handle-1.patch',
    'crates/mem-supervisor-pi/windows-pipe-helper/Cargo.toml','crates/mem-supervisor-pi/windows-pipe-helper/src/lib.rs')
if (@($manifest.inputs).Count -ne $expected.Count -or @(Compare-Object $expected @($manifest.inputs | ForEach-Object path) -CaseSensitive).Count) { throw 'Pi proof requires the exact existing builder, notices, patches and protocol adapters' }
for ($index=0; $index -lt $expected.Count; $index++) {
    if ($manifest.inputs[$index].path -cne $expected[$index]) { throw 'Pi proof input order differs from reviewed manifest' }
}
foreach ($inputFile in $manifest.inputs) { Assert-Hash (Join-Path $SourceRoot $inputFile.path.Replace('/','\')) $inputFile.sha256 }
$root='C:\MemoryLab\NativePi'; $tools='C:\MemoryLab\Tools'
Assert-NoReparse $root; Assert-NoReparse $tools
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
New-Item -ItemType Directory -Path $root -Force | Out-Null
Run "$env:SystemRoot\System32\icacls.exe" @($root,'/inheritance:r','/grant:r',"*${sid}:(OI)(CI)F",'*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F')
$build=Join-Path $root ('build-'+$OperationId.ToString())
Assert-NoReparse $build
$receipt=Join-Path $build 'build-evidence.json'; $prepared=Join-Path $build 'prepared.json'
$manifestHash=(Get-FileHash -LiteralPath $ManifestPath).Hash.ToLowerInvariant()
$git=Join-Path $tools 'git\cmd\git.exe'
$python=Join-Path $tools 'python-3.13.9\python.exe'
$originalProfile=$env:USERPROFILE
$rustBin=Join-Path $originalProfile '.cargo\bin'
$system=[Environment]::GetFolderPath('Windows')
$privateProfile=Join-Path $root 'home'; $temporary=Join-Path $root 'temp'
foreach ($privatePath in @($privateProfile,$temporary,(Join-Path $root 'cargo'),(Join-Path $root 'rustup'))) { Assert-NoReparse $privatePath }
New-Item -ItemType Directory -Path $privateProfile,$temporary -Force | Out-Null
$previous=@{}; Get-ChildItem Env: | ForEach-Object { $previous[$_.Name]=$_.Value }
try {
    # No caller/provider credentials or user Cargo/Git configuration enter
    # source fetch, dependency builds, or the synthetic execution path.
    Get-ChildItem Env: | ForEach-Object { [Environment]::SetEnvironmentVariable($_.Name,$null,'Process') }
    foreach ($entry in @{
        SystemRoot=$system; WINDIR=$system; SystemDrive=([IO.Path]::GetPathRoot($system).TrimEnd('\'))
        COMSPEC=(Join-Path $system 'System32\cmd.exe'); OS='Windows_NT'; COMPUTERNAME='MEMORY-WINLAB'
        PROCESSOR_ARCHITECTURE='AMD64'; USERPROFILE=$privateProfile; HOME=$privateProfile; TEMP=$temporary; TMP=$temporary
        PATH=($rustBin+';'+(Split-Path -Parent $git)+';'+(Split-Path -Parent $python)+';'+(Join-Path $system 'System32'))
        CARGO_HOME=(Join-Path $root 'cargo'); RUSTUP_HOME=(Join-Path $root 'rustup'); CARGO_BUILD_JOBS='2'
        GIT_TERMINAL_PROMPT='0'; GIT_CONFIG_NOSYSTEM='1'; GIT_CONFIG_GLOBAL='NUL'
        # PowerShell stores its process-only -ExecutionPolicy setting in this
        # environment entry. Preserve the fixed lab policy after scrubbing;
        # otherwise invoking the reviewed builder reverts to machine Restricted.
        PSExecutionPolicyPreference='Bypass'
        'ProgramFiles'=[Environment]::GetFolderPath('ProgramFiles'); 'ProgramFiles(x86)'=[Environment]::GetFolderPath('ProgramFilesX86')
    }.GetEnumerator()) { [Environment]::SetEnvironmentVariable($entry.Key,$entry.Value,'Process') }
    if ($Action -eq 'Prepare' -and -not (Test-Path -LiteralPath $receipt)) {
        if (Test-Path -LiteralPath $build) { throw 'Incomplete native build retained; inspect its failure and use a new explicit operation for another attempt' }
        $repository=Join-Path $root 'upstream.git'; $hooks=Join-Path $root 'empty-hooks'
        Assert-NoReparse $repository; Assert-NoReparse $hooks
        New-Item -ItemType Directory -Path $hooks -Force | Out-Null
        if (-not (Test-Path -LiteralPath $repository)) { Run $git @('init','--bare',('--template='+$hooks),$repository) }
        Run $git @('-C',$repository,'-c','credential.helper=','-c',('core.hooksPath='+$hooks),'fetch','--depth=1','--no-tags',$manifest.source_git_url,$manifest.source_revision)
        $archive=Join-Path $root 'source-7c018475.tar'
        Assert-NoReparse $archive
        if (-not (Test-Path -LiteralPath $archive)) { Run $git @('-C',$repository,'archive','--format=tar',('--output='+$archive),$manifest.source_revision) }
        Assert-Hash $archive $manifest.source_archive_sha256
        $vswhere=Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio\Installer\vswhere.exe'
        $vs=@(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
        if ($LASTEXITCODE -ne 0 -or $vs.Count -ne 1) { throw 'Native MSVC tools are required' }
        $vcvars=Join-Path $vs[0] 'VC\Auxiliary\Build\vcvars64.bat'
        $lines=@(& (Join-Path $system 'System32\cmd.exe') /d /s /c ('""'+$vcvars+'" >nul && set"'))
        if ($LASTEXITCODE -ne 0) { throw 'Cannot initialize native MSVC environment' }
        foreach ($line in $lines) { $pair=$line.Split('=',2); if ($pair.Length -eq 2 -and $pair[0]) { [Environment]::SetEnvironmentVariable($pair[0],$pair[1],'Process') } }
        & (Join-Path $SourceRoot 'crates\mem-supervisor-pi\scripts\build-windows.ps1') -SourceRepository $repository -BuildRoot $build -InstallToolchain
    }
    if (-not (Test-Path -LiteralPath $receipt)) { throw 'A successfully recorded native Pi build is required before verification' }
    $evidence=Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
    if ($evidence.source_link_materialization.path -cne '.beads/beads.jsonl' -or
        $evidence.source_link_materialization.target -cne 'issues.jsonl' -or
        $evidence.source_link_materialization.policy -cne 'git-core-symlinks-false') { throw 'Unreviewed native source-link materialization' }
    if ($evidence.target -cne $manifest.target -or $evidence.rust_toolchain -cne $manifest.rust_toolchain -or
        $evidence.source_archive_sha256 -cne $manifest.source_archive_sha256 -or $evidence.upstream_lock_sha256 -cne $manifest.upstream_lock_sha256 -or
        $evidence.patched_lock_sha256 -cne $manifest.patched_lock_sha256 -or $evidence.artifact.patched_lock_sha256 -cne $manifest.patched_lock_sha256 -or
        $evidence.rustc -notmatch ('(?m)^commit-hash: '+[regex]::Escape($manifest.rustc_commit)+'\r?$') -or
        $evidence.rustc -notmatch '(?m)^host: x86_64-pc-windows-msvc\r?$' -or
        $evidence.artifact.source_revision -cne $manifest.source_revision -or
        $evidence.artifact.no_tools_patch_sha256 -cne $manifest.inputs[1].sha256 -or
        $evidence.artifact.broker_patch_sha256 -cne $manifest.inputs[2].sha256 -or
        $evidence.artifact.portability_patch_sha256 -cne $manifest.inputs[6].sha256 -or
        $evidence.artifact.helper_manifest_sha256 -cne $manifest.inputs[7].sha256 -or
        $evidence.artifact.helper_source_sha256 -cne $manifest.inputs[8].sha256) { throw 'Recorded build differs from source/compiler/patch contract' }
    $binary=Join-Path $build 'target\release\pi.exe'
    Assert-Hash $binary $evidence.artifact.binary_sha256
    if (Test-Path -LiteralPath $prepared) {
        $old=Get-Content -LiteralPath $prepared -Raw | ConvertFrom-Json
        if ($old.manifest_sha256 -cne $manifestHash -or $old.binary_sha256 -cne $evidence.artifact.binary_sha256) { throw 'Prepared Pi operation has different inputs' }
    } else {
        @{format='memory-windows-pi-prepared-v1';manifest_sha256=$manifestHash;binary_sha256=$evidence.artifact.binary_sha256;native_execution_verified=$false;isolation_verified=$false} |
            ConvertTo-Json | Set-Content -LiteralPath $prepared -Encoding UTF8
    }
    if ($Action -eq 'Verify') {
        $scratch=Join-Path $root ('smoke-'+$OperationId.ToString()); New-Item -ItemType Directory -Path $scratch -Force | Out-Null
        Run $python @((Join-Path $SourceRoot 'packaging\windows-pi\smoke.py'),'--candidate',$binary,'--candidate-sha256',$evidence.artifact.binary_sha256,
            '--protocol-library',(Join-Path $SourceRoot 'packaging\ca-bookworm\pi_smoke.py'),'--protocol-sha256',$manifest.inputs[4].sha256,'--scratch',$scratch)
    }
    Write-Output 'Native Pi build/proof command completed; AppContainer, inherited-broker and durable supervisor acceptance remain separate'
} finally {
    Get-ChildItem Env: | ForEach-Object { [Environment]::SetEnvironmentVariable($_.Name,$null,'Process') }
    foreach ($entry in $previous.GetEnumerator()) { [Environment]::SetEnvironmentVariable($entry.Key,$entry.Value,'Process') }
}
