# SPDX-License-Identifier: AGPL-3.0-or-later
# Actual synthetic Windows knowledge adapter, not a paid R3 campaign.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$SourceRoot,
      [Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or
    $OperationId -eq [guid]::Empty -or
    $SourceRoot -cnotmatch '^C:\\MemoryLab\\source(?:\\[A-Za-z0-9_.-]+)+$') { throw 'Explicit disposable native evaluation source required' }
foreach ($name in @('OPENAI_API_KEY','ANTHROPIC_API_KEY','GEMINI_API_KEY')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw 'Native evaluation proof requires no provider credentials' }
}
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if ($identity.Name -ine 'MEMORY-WINLAB\memorylab' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) { throw 'Ordinary interactive lab identity required' }
$git='C:\MemoryLab\Tools\git\cmd\git.exe'
$head=(& $git -C $SourceRoot rev-parse HEAD)
if ($LASTEXITCODE -or $head -cnotmatch '^[0-9a-f]{40}$') { throw 'Committed source required' }
$changes=(& $git -C $SourceRoot status --porcelain --untracked-files=no)
if ($LASTEXITCODE -or $changes) { throw 'Clean evaluation source required' }
$root=Join-Path 'C:\MemoryLab\runs' ($OperationId.ToString()+'\native-eval')
if (Test-Path -LiteralPath $root) { throw 'Retain existing proof; use a new operation identity' }
New-Item -ItemType Directory -Path $root | Out-Null
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*$($identity.User.Value):(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Native evaluation root protection failed' }
$code=Join-Path $root 'code'; $io=Join-Path $root 'io'
New-Item -ItemType Directory -Path $code,$io | Out-Null
$inputs='C:\MemoryLab\inputs'
$node='C:\MemoryLab\Tools\node-v24.18.1-win-x64\node.exe'
Copy-Item -LiteralPath $node -Destination (Join-Path $code 'node.exe')
Copy-Item -LiteralPath (Join-Path $SourceRoot 'evals\r3\isolation\native-guest.mjs') -Destination $code
Copy-Item -LiteralPath (Join-Path $inputs 'r3_score.exe') -Destination $code
Copy-Item -LiteralPath (Join-Path $inputs 'evaluation-helper.exe') -Destination $root
@{format='memory-native-evaluation-input-v1';source_commit=$head;
  node_sha256=(Get-FileHash -LiteralPath $node).Hash.ToLowerInvariant();
  helper_sha256=(Get-FileHash -LiteralPath (Join-Path $root 'evaluation-helper.exe')).Hash.ToLowerInvariant();
  scorer_sha256=(Get-FileHash -LiteralPath (Join-Path $code 'r3_score.exe')).Hash.ToLowerInvariant()} |
    ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'input.json') -Encoding UTF8
$process=Start-Process -FilePath $node -ArgumentList @((Join-Path $inputs 'guest-evaluation-proof.mjs'),$SourceRoot,$root) -NoNewWindow -PassThru `
    -RedirectStandardOutput (Join-Path $root 'stdout.log') -RedirectStandardError (Join-Path $root 'stderr.log')
$null=$process.Handle
if (-not $process.WaitForExit(180000)) {
    & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
    throw 'Owned native evaluation exceeded three minutes; inspect retained operation'
}
$process.WaitForExit()
if ($process.ExitCode -ne 0) { throw 'Native evaluation failed; inspect retained private diagnostics' }
$result=Get-Content -LiteralPath (Join-Path $root 'result.json') -Raw | ConvertFrom-Json
if ($result.format -cne 'memory-native-evaluation-proof-v1' -or $result.state -cne 'succeeded' -or
    $result.official_evidence -ne $false -or $result.provider_requests -ne 0) { throw 'Invalid native evaluation receipt' }
