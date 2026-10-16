# SPDX-License-Identifier: AGPL-3.0-or-later
# Synthetic native startup only: no account, public listener or PLC operation.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or $OperationId -eq [guid]::Empty) { throw 'Disposable native lab operation required' }
$root='C:\MemoryLab\NativePds'
$runtime=Join-Path $root 'runtime'
$prepared=Get-Content -LiteralPath (Join-Path $root 'prepared.json') -Raw | ConvertFrom-Json
if ($prepared.format -cne 'memory-windows-pds-proof-result-v1' -or $prepared.native_sqlite_verified -ne $true) { throw 'Successful pinned native PDS preparation required' }
$node='C:\MemoryLab\Tools\node-v24.18.1-win-x64\node.exe'
if ((Get-FileHash -LiteralPath $node).Hash.ToLowerInvariant() -cne $prepared.node_sha256) { throw 'Native Node changed after preparation' }
# Recheck the complete prepared runtime before executing its dependency code.
$actual=@(Get-ChildItem -LiteralPath $runtime -Recurse -Force)
if ($actual | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Native runtime reparse entry rejected' }
$files=@($actual | Where-Object { -not $_.PSIsContainer })
if ($files.Count -ne @($prepared.files).Count) { throw 'Native PDS runtime inventory changed' }
foreach ($entry in $prepared.files) {
    if ([IO.Path]::IsPathRooted($entry.path) -or $entry.path.Contains(':') -or $entry.path.Contains('\') -or
        @($entry.path.Split('/') | Where-Object { $_ -in @('','..','.') }).Count) { throw 'Invalid prepared runtime path' }
    $file=Join-Path $runtime $entry.path.Replace('/','\')
    if ((Get-FileHash -LiteralPath $file).Hash.ToLowerInvariant() -cne $entry.sha256) { throw 'Native PDS runtime bytes changed' }
}
$operation=Join-Path $root ('stdio-'+$OperationId.ToString())
if (Test-Path -LiteralPath $operation) { throw 'Retain the existing PDS attempt; use an explicit new operation' }
New-Item -ItemType Directory -Path $operation | Out-Null
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& "$env:SystemRoot\System32\icacls.exe" $operation /inheritance:r /grant:r "*${sid}:(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'PDS proof state ACL failed' }
$data=Join-Path $operation 'data'; $temporary=Join-Path $operation 'temp'
New-Item -ItemType Directory -Path $data,$temporary | Out-Null
$start=[Diagnostics.ProcessStartInfo]::new()
$start.FileName=$node
$start.Arguments='"'+(Join-Path $runtime 'probe-stdio.mjs')+'" "'+$node+'" "'+(Join-Path $runtime 'pds-server.mjs')+'"'
$start.WorkingDirectory=$operation; $start.UseShellExecute=$false; $start.CreateNoWindow=$true
$start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
$start.EnvironmentVariables.Clear()
foreach ($entry in @{
    SystemRoot=$env:SystemRoot; PATH=(Split-Path -Parent $node)+';'+$env:SystemRoot+'\System32'; TEMP=$temporary; TMP=$temporary
    PDS_HOSTNAME='pds.federation.example.com'; PDS_SERVICE_HANDLE_DOMAINS='.federation.example.com'
    PDS_DATA_DIRECTORY=$data; PDS_BLOBSTORE_DISK_LOCATION=(Join-Path $data 'blobs')
    PDS_JWT_SECRET=([guid]::NewGuid().ToString('N')+[guid]::NewGuid().ToString('N'))
    PDS_ADMIN_PASSWORD=([guid]::NewGuid().ToString('N')+[guid]::NewGuid().ToString('N'))
    PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX=([guid]::NewGuid().ToString('N')+[guid]::NewGuid().ToString('N'))
    PDS_INVITE_REQUIRED='true'; PDS_DEV_MODE='false'; PDS_DISABLE_SSRF_PROTECTION='false'; PDS_CRAWLERS=''
    PDS_DID_PLC_URL='https://unavailable.federation.example.com'; LOG_LEVEL='warn'
}.GetEnumerator()) { $start.EnvironmentVariables[$entry.Key]=$entry.Value }
$process=[Diagnostics.Process]::new(); $process.StartInfo=$start
try {
    if (-not $process.Start()) { throw 'Native PDS proof process could not start' }
    $stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(90000)) {
        & "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
        throw 'Native PDS proof timed out; retain its synthetic state'
    }
    $process.WaitForExit()
    # The probe emits metadata, never child stderr or synthetic credentials.
    if ($stdout.Result.Length -gt 65536 -or $stderr.Result.Length -gt 65536 -or $process.ExitCode -ne 0) { throw 'Native stdio probe failed; no PDS acceptance claim' }
    $observed=$stdout.Result | ConvertFrom-Json
    if ($observed.ready -ne $true -or $observed.describe_server -ne 200 -or $observed.clean_eof_shutdown -ne $true) { throw 'Native stdio proof did not establish readiness, HTTP or shutdown' }
    $receipt=[ordered]@{format='memory-native-pds-stdio-result-v1';operation_id=$OperationId.ToString()
        observed_at=[DateTime]::UtcNow.ToString('o');node_sha256=$prepared.node_sha256
        prepared_sha256=(Get-FileHash (Join-Path $root 'prepared.json')).Hash.ToLowerInvariant()
        ready=$true;describe_server=200;clean_eof_shutdown=$true;tcp_listener_created=$false
        signed_repository_verified=$false;pipe_acl_verified=$false}
    $receipt | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $operation 'result.json') -Encoding UTF8
    $receipt | ConvertTo-Json
} finally { $process.Dispose() }
