# SPDX-License-Identifier: AGPL-3.0-or-later
# Required-database tests against the existing disposable native PG17 cluster.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$SourceRoot,
      [Parameter(Mandatory=$true)][guid]$OperationId,
      [ValidateSet('Area','Service')][string]$Scope='Area')
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
$label=if ($Scope -eq 'Service') { 'native-service-db' } else { 'native-db' }
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
    $arguments=if ($Scope -eq 'Service') { @('test','--locked','-p','mem-service','--lib','--','--test-threads=1') }
        else { @('test','--locked','-p','mem-area-ops','-p','mem-area-store','--all-targets','--','--test-threads=1') }
    $process=Start-Process -FilePath $cargo -ArgumentList $arguments -NoNewWindow -PassThru `
        -RedirectStandardOutput (Join-Path $root 'cargo.stdout.log') -RedirectStandardError (Join-Path $root 'cargo.stderr.log')
    $null=$process.Handle
    if (-not $process.WaitForExit(7200000)) {
        & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
        throw 'Owned native tests exceeded two hours; inspect retained logs'
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Required-database native tests failed; inspect retained private diagnostics' }
    if ((Get-FileHash -LiteralPath $lock).Hash.ToLowerInvariant() -cne $lockHash) { throw 'Tests changed frozen Cargo.lock' }
    $receipt.state='succeeded'
} catch { $receipt.state='failed'; throw }
finally {
    $env:MEMORY_LAYER_TEST_DATABASE_URL=$null; $env:MEMORY_LAYER_TEST_REQUIRE_DB=$null; $password=$null
    Pop-Location
    $receipt | ConvertTo-Json | Set-Content -LiteralPath $result -Encoding UTF8
}
