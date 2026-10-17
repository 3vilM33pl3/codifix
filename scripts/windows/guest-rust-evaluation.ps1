# SPDX-License-Identifier: AGPL-3.0-or-later
# Real offline compilation in a disposable lab; never a paid R3 campaign.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$SourceRoot,
      [Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$') { throw 'Disposable native Rust proof required' }
foreach ($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw 'No provider credentials permitted' }
}
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\native-rust-eval')
if (Test-Path -LiteralPath $root) { throw 'Preserve existing proof; use a new operation' }
New-Item -ItemType Directory -Path $root | Out-Null
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Private proof directory required' }
$git='C:\MemoryLab\Tools\git\cmd\git.exe'
$head=(& $git -C $SourceRoot rev-parse HEAD)
if ($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$') { throw 'Committed source required' }
$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
if ($LASTEXITCODE -or $changes) { throw 'Clean committed source required' }
$rust=Join-Path $env:USERPROFILE '.rustup\toolchains\1.99.1-x86_64-pc-windows-msvc'
$vc='C:\MemoryLab\BuildTools\VC\Tools\MSVC\14.44.35207'
$sdk='C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0'
$code=Join-Path $root 'code'
New-Item -ItemType Directory -Path $code,(Join-Path $root 'io') | Out-Null
# Copy only explicit compiler/runtime/linker/library trees, never the developer
# home, whole Windows SDK, credentials or hidden scoring material. The helper
# inventories and pins every copied regular file before each sandbox launch.
function Copy-ProofTree([string]$From,[string]$To) {
    $items=@(Get-ChildItem -LiteralPath $From -Recurse -Force)
    if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Toolchain reparse entry rejected' }
    New-Item -ItemType Directory -Path $To | Out-Null
    Get-ChildItem -LiteralPath $From -Force | Copy-Item -Destination $To -Recurse
}
Copy-ProofTree (Join-Path $rust 'bin') (Join-Path $code 'rust\bin')
Copy-ProofTree (Join-Path $rust 'lib\rustlib\x86_64-pc-windows-msvc\lib') (Join-Path $code 'rust\lib\rustlib\x86_64-pc-windows-msvc\lib')
Copy-ProofTree (Join-Path $vc 'bin\Hostx64\x64') (Join-Path $code 'vc\bin')
Copy-ProofTree (Join-Path $vc 'lib\x64') (Join-Path $code 'vc\lib')
Copy-ProofTree (Join-Path $sdk 'um\x64') (Join-Path $code 'sdk\um')
Copy-ProofTree (Join-Path $sdk 'ucrt\x64') (Join-Path $code 'sdk\ucrt')
$node='C:\MemoryLab\Tools\node-v24.18.1-win-x64\node.exe'
Copy-Item -LiteralPath $node -Destination $code
Copy-Item -LiteralPath (Join-Path $SourceRoot 'evals\r3\isolation\native-guest.mjs') -Destination $code
Copy-Item -LiteralPath 'C:\MemoryLab\inputs\evaluation-helper.exe' -Destination $root
$inventory=@(Get-ChildItem -LiteralPath $code -Recurse -File | Sort-Object FullName | ForEach-Object {
    @{path=$_.FullName;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}
})
$driver='C:\MemoryLab\inputs\guest-rust-proof.mjs'
$inputRecord=@{format='memory-native-rust-proof-input-v1';source_commit=$head;files=$inventory;
  script_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant();
  driver_sha256=(Get-FileHash -LiteralPath $driver).Hash.ToLowerInvariant();
  helper_sha256=(Get-FileHash -LiteralPath (Join-Path $root 'evaluation-helper.exe')).Hash.ToLowerInvariant();
  compiler_version='1.99.1';msvc_version='14.44.35207';sdk_version='10.0.26100.0'}
[IO.File]::WriteAllText((Join-Path $root 'inventory.json'), ($inputRecord | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
$process=Start-Process -FilePath $node -ArgumentList @($driver,$SourceRoot,$root) -NoNewWindow -PassThru `
    -RedirectStandardOutput (Join-Path $root 'stdout.log') -RedirectStandardError (Join-Path $root 'stderr.log')
$null=$process.Handle
if (-not $process.WaitForExit(600000)) {
    & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
    throw 'Owned native Rust proof exceeded ten minutes'
}
$process.WaitForExit()
if ($process.ExitCode -ne 0) { throw 'Native Rust proof failed; inspect retained private diagnostics' }
$result=Get-Content -LiteralPath (Join-Path $root 'result.json') -Raw | ConvertFrom-Json
if ($result.state -cne 'succeeded' -or $result.real_provider_requests -ne 0 -or $result.official_evidence -ne $false) { throw 'Invalid proof receipt' }
