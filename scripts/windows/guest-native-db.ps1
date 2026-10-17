# SPDX-License-Identifier: AGPL-3.0-or-later
# Required-database tests against the existing disposable native PG17 cluster.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$SourceRoot,
      [Parameter(Mandatory=$true)][guid]$OperationId,
      [ValidateSet('Area','Service','Supervisor','Fleet','Workspace')][string]$Scope='Area')
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$') { throw 'Explicit disposable native database source required' }
foreach ($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY','DATABASE_URL','MEMORY_LAYER_TEST_DATABASE_URL')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw 'Native database proof rejects ambient provider/database credentials' }
}
$pg='C:\MemoryLab\NativePostgres'
$verified=Get-Content -LiteralPath (Join-Path $pg 'verified.json') -Raw | ConvertFrom-Json
if ($verified.format -cne 'memory-windows-postgres-execution-v1' -or $verified.host -cne '127.0.0.1' -or
    $verified.port -ne 55471 -or $verified.production -ne $false -or
    (Get-Content -LiteralPath (Join-Path $pg 'data\PG_VERSION') -Raw).Trim() -cne '17') { throw 'Previously verified exact lab PostgreSQL required' }
$git='C:\MemoryLab\Tools\git\cmd\git.exe'
$head=(& $git -C $SourceRoot rev-parse HEAD)
if ($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$') { throw 'Committed source required' }
$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
if ($LASTEXITCODE -or $changes) { throw 'Clean database test source required' }
$label=switch ($Scope) { 'Service' {'native-service-db'} 'Supervisor' {'native-supervisor'} 'Fleet' {'native-pds-fleet'} 'Workspace' {'native-workspace-db'} default {'native-db'} }
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\'+$label)
if (Test-Path -LiteralPath $root) { throw 'Retain existing database test evidence; use a new operation identity' }
New-Item -ItemType Directory -Path $root | Out-Null
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Database proof evidence protection failed' }
$cargo=Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$rustc=Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
$compiler=(& $rustc -vV)
if ($LASTEXITCODE -or ($compiler -join "`n") -notmatch '(?m)^host: x86_64-pc-windows-msvc\r?$') { throw 'Native MSVC compiler required' }
$lock=Join-Path $SourceRoot 'Cargo.lock'; $lockHash=(Get-FileHash -LiteralPath $lock).Hash.ToLowerInvariant()
$receipt=[ordered]@{format='memory-native-database-tests-v1';source_commit=$head;lock_sha256=$lockHash;compiler=($compiler -join "`n")
    state='running';required_database=$true;native_database='PostgreSQL17/127.0.0.1:55471';scope=$Scope;workflow_verified=$false}
$result=Join-Path $root 'result.json'; $receipt | ConvertTo-Json | Set-Content -LiteralPath $result -Encoding UTF8
$secret=ConvertTo-SecureString (Get-Content -LiteralPath (Join-Path $pg 'test-password.dpapi') -Raw).Trim()
$pointer=[Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
try { $password=[Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
Push-Location $SourceRoot
try {
    # Only this synthetic VM test principal. The credential never enters command
    # arguments, source files, receipts or the host's configuration.
    $env:MEMORY_LAYER_TEST_DATABASE_URL='postgresql://memorylab_test:'+ [uri]::EscapeDataString($password)+'@127.0.0.1:55471/postgres'
    $password=$null
    $env:MEMORY_LAYER_TEST_REQUIRE_DB='1'
    $env:CARGO_TARGET_DIR='C:\MemoryLab\build\product-msvc'
    $env:CARGO_BUILD_TARGET='x86_64-pc-windows-msvc'; $env:CARGO_BUILD_JOBS='2'
    $env:PATH=(Split-Path -Parent $cargo)+';'+$env:PATH
    if ($Scope -eq 'Supervisor') {
        $build=Start-Process -FilePath $cargo -ArgumentList @('build','--locked','-p','mem-supervisor-pi','--bin','memory-pi-sandbox') -NoNewWindow -PassThru `
            -RedirectStandardOutput (Join-Path $root 'build.stdout.log') -RedirectStandardError (Join-Path $root 'build.stderr.log')
        $null=$build.Handle
        if (-not $build.WaitForExit(1800000)) { & "$env:SystemRoot\System32\taskkill.exe" /PID $build.Id /T /F | Out-Null; throw 'Owned supervisor build deadline' }
        $build.WaitForExit()
        if ($build.ExitCode -ne 0) { throw 'Supervisor driver build failed' }
        $piBuild='C:\MemoryLab\NativePi\build-9579fb57-0767-48c7-8ee6-66db32d04454'
        $evidence=Get-Content -LiteralPath (Join-Path $piBuild 'build-evidence.json') -Raw | ConvertFrom-Json
        $pi=Join-Path $piBuild 'target\release\pi.exe'
        if ($evidence.artifact.binary_sha256 -cne '70951dc2f14b3857b630a91928f2bd9ccff328e1d4a81081fadcffb27e72263b' -or
            (Get-FileHash -LiteralPath $pi).Hash.ToLowerInvariant() -cne $evidence.artifact.binary_sha256) { throw 'Unchanged pinned native Pi required' }
        $code=Join-Path $root 'code'; $private=Join-Path $root 'private'
        New-Item -ItemType Directory -Path $code,$private | Out-Null
        Copy-Item -LiteralPath $pi -Destination (Join-Path $code 'pi.exe')
        Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\debug\memory-pi-sandbox.exe') -Destination $code
        $driver=Join-Path $code 'memory-pi-sandbox.exe'
        $config=[ordered]@{pi=(Join-Path $code 'pi.exe');driver=$driver;driver_sha256=(Get-FileHash $driver).Hash.ToLowerInvariant()
            private_runtime=$private;windows_artifacts=$evidence.artifact}
        $configPath=Join-Path $root 'config.json'
        [IO.File]::WriteAllText($configPath,($config | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
        $env:MEMORY_PI_WINDOWS_SANDBOX_CONFIG=$configPath
        $env:MEMORY_PI_WINDOWS_SANDBOX_CONFIG_SHA256=(Get-FileHash $configPath).Hash.ToLowerInvariant()
    }
    $program=$cargo
    $arguments=if ($Scope -eq 'Service') { @('test','--locked','-p','mem-service','--lib','--','--test-threads=1') }
        elseif ($Scope -eq 'Workspace') { @('test','--workspace','--all-targets','--locked','--','--test-threads=1') }
        elseif ($Scope -eq 'Supervisor') { @('test','--locked','-p','mem-supervisor-runtime','--lib',
            'transport::tests::broker::launcher_windows::native_production_launcher_delivers_revokes_accounts_and_recovers','--','--ignored','--exact','--test-threads=1','--nocapture') }
        elseif ($Scope -eq 'Fleet') {
            $program='C:\MemoryLab\Tools\node-v24.18.1-win-x64\node.exe'
            @('C:\MemoryLab\inputs\guest-pds-fleet.mjs',$SourceRoot,$root,$sid,'--fleet','C:\MemoryLab\inputs\native-pds-profile.json')
        }
        else { @('test','--locked','-p','mem-area-ops','-p','mem-area-store','--all-targets','--','--test-threads=1') }
    $process=Start-Process -FilePath $program -ArgumentList $arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $root 'cargo.stdout.log') -RedirectStandardError (Join-Path $root 'cargo.stderr.log')
    $null=$process.Handle
    if (-not $process.WaitForExit(7200000)) {
        & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
        throw 'Owned native tests exceeded two hours; inspect retained logs'
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Required-database native tests failed; inspect retained private diagnostics' }
    if ($Scope -eq 'Supervisor' -and (Get-Content (Join-Path $root 'cargo.stdout.log') -Raw) -notmatch 'test result: ok\. 1 passed;') { throw 'Native supervisor proof must select and pass one real test' }
    if ($Scope -eq 'Fleet') {
        $fleet=Get-Content (Join-Path $root 'fleet-proof.json') -Raw | ConvertFrom-Json
        if ($fleet.format -cne 'memory-native-pds-fleet-proof-v1' -or $fleet.state -cne 'succeeded' -or
            $fleet.native_pds_pipe -ne $true -or $fleet.real_provider_requests -ne 0 -or @($fleet.results).Count -ne 3) { throw 'Native fleet proof missing' }
        $receipt.fleet_workflows_verified=$true
    }
    if ((Get-FileHash -LiteralPath $lock).Hash.ToLowerInvariant() -cne $lockHash) { throw 'Tests changed frozen Cargo.lock' }
    $receipt.state='succeeded'
} catch { $receipt.state='failed'; throw }
finally {
    $env:MEMORY_LAYER_TEST_DATABASE_URL=$null; $env:MEMORY_LAYER_TEST_REQUIRE_DB=$null; $password=$null
    $env:MEMORY_PI_WINDOWS_SANDBOX_CONFIG=$null; $env:MEMORY_PI_WINDOWS_SANDBOX_CONFIG_SHA256=$null
    Pop-Location
    $receipt | ConvertTo-Json | Set-Content -LiteralPath $result -Encoding UTF8
}
