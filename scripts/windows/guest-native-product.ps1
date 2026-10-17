# SPDX-License-Identifier: AGPL-3.0-or-later
# Compilation proof in the disposable VM, not release or workflow acceptance.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$SourceRoot,
    [Parameter(Mandatory=$true)][guid]$OperationId,
    [ValidateSet('Check','Build','Web')][string]$Action='Check'
)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -or $OperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)*$' -or
    $SourceRoot.Split('\') -contains '..') { throw 'Explicit disposable AMD64 lab source and operation required' }
foreach ($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw 'Product builds require a credential-free environment' }
}
$tools='C:\MemoryLab\Tools'
$git=Join-Path $tools 'git\cmd\git.exe'
$cargo=Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$rustc=Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
$head=(& $git -C $SourceRoot rev-parse HEAD)
if ($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$') { throw 'Cannot bind product source commit' }
$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
if ($LASTEXITCODE -or $changes) { throw 'Committed clean product source required' }
$compiler=(& $rustc -vV)
if ($LASTEXITCODE -or ($compiler -join "`n") -notmatch '(?m)^host: x86_64-pc-windows-msvc\r?$') { throw 'Native MSVC Rust compiler required' }
$label=switch($Action){'Build'{'product-build'} 'Web'{'product-web'} default{'product-check'}}
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\'+$label)
if (Test-Path -LiteralPath $root) { throw 'Product attempt exists; retain its evidence and use an explicit new operation' }
New-Item -ItemType Directory -Path $root | Out-Null
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Product evidence ACL failed' }
$lock=Join-Path $SourceRoot 'Cargo.lock'
$lockHash=(Get-FileHash -LiteralPath $lock).Hash.ToLowerInvariant()
$env:CARGO_TARGET_DIR='C:\MemoryLab\build\product-msvc'
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_BUILD_TARGET='x86_64-pc-windows-msvc'
$env:PATH=(Join-Path $env:USERPROFILE '.cargo\bin')+';'+(Join-Path $tools 'node-v24.18.1-win-x64')+';'+(Split-Path -Parent $git)+';'+$env:PATH
$receipt=[ordered]@{format='memory-native-product-check-v1';action=$Action;source_commit=$head;lock_sha256=$lockHash
    compiler=($compiler -join "`n");state='running';compilation_verified=$false;workflow_verified=$false}
$result=Join-Path $root 'result.json'
$receipt | ConvertTo-Json | Set-Content -LiteralPath $result -Encoding UTF8
Push-Location $SourceRoot
try {
    if ($Action -eq 'Web') {
        $node=Join-Path $tools 'node-v24.18.1-win-x64\node.exe'
        $prepared=Get-Content -LiteralPath 'C:\MemoryLab\NativePds\prepared.json' -Raw | ConvertFrom-Json
        if($prepared.node_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
            (Get-FileHash -LiteralPath $node).Hash.ToLowerInvariant() -cne $prepared.node_sha256){throw 'Pinned native Node required'}
        $webLock=Join-Path $SourceRoot 'web\package-lock.json'
        $webLockHash=(Get-FileHash -LiteralPath $webLock).Hash.ToLowerInvariant()
        $receipt.web_lock_sha256=$webLockHash; $receipt.node_sha256=$prepared.node_sha256
        $npm=Join-Path $tools 'node-v24.18.1-win-x64\node_modules\npm\bin\npm-cli.js'
        foreach($step in @(@('ci','--no-audit','--no-fund'),@('test'),@('run','build'))){
            $name=$step[0]
            $process=Start-Process -FilePath $node -ArgumentList (@($npm,'--prefix','web')+$step) -NoNewWindow -PassThru `
                -RedirectStandardOutput (Join-Path $root ($name+'.stdout.log')) -RedirectStandardError (Join-Path $root ($name+'.stderr.log'))
            $null=$process.Handle
            if(-not $process.WaitForExit(1800000)){
                & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
                throw 'Owned native Web step exceeded 30 minutes'
            }
            $process.WaitForExit(); if($process.ExitCode -ne 0){throw 'Native Web step failed; inspect private logs'}
        }
        if((Get-FileHash -LiteralPath $webLock).Hash.ToLowerInvariant() -cne $webLockHash){throw 'Web build changed frozen package lock'}
        $receipt.web_tests_verified=$true; $receipt.browser_workflow_verified=$false
        $receipt.state='succeeded'; $receipt.compilation_verified=$true
        return
    }
    # Separate bounded evidence files avoid PowerShell 5.1 turning ordinary
    # compiler stderr into a terminating error or dropping it from transcripts.
    $arguments=if ($Action -eq 'Build') {
        @('build','--workspace','--bins','--locked','--features','codifix-platform/windows-evaluation-helper,codifix-platform/windows-native-tests')
    } else { @('check','--workspace','--all-targets','--locked') }
    $process=Start-Process -FilePath $cargo -ArgumentList $arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $root 'cargo.stdout.log') -RedirectStandardError (Join-Path $root 'cargo.stderr.log')
    $null=$process.Handle
    if (-not $process.WaitForExit(7200000)) {
        & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
        throw 'Owned native compilation exceeded two hours; retained logs require inspection'
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Native workspace compilation failed; inspect retained compiler logs' }
    if ((Get-FileHash -LiteralPath $lock).Hash.ToLowerInvariant() -cne $lockHash) { throw 'Build changed the frozen Cargo.lock' }
    if ($Action -eq 'Build') {
        # Debug executables for disposable workflow acceptance, not candidate
        # release artifacts. Final packaging must bind its own frozen build.
        $receipt.artifacts=@(Get-ChildItem -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\debug') -Filter '*.exe' -File |
            Sort-Object Name | ForEach-Object { @{file=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()} })
        $receipt.release_candidate=$false
    }
    $receipt.state='succeeded'; $receipt.compilation_verified=$true
} catch { $receipt.state='failed'; throw }
finally {
    Pop-Location
    $receipt | ConvertTo-Json | Set-Content -LiteralPath $result -Encoding UTF8
}
