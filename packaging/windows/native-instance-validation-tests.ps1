# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable invariants only; this is not native Windows acceptance evidence.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
$instance = [guid]'11111111-1111-4111-8111-111111111111'
$plan = Get-NativeInstancePlan 'homelab' 'windows-v2.1.0-rc.5-build.1' 'memory.exe' 'http://127.0.0.1:55440/healthz' $instance
$first = Get-NativeInputDigest ([ordered]@{ action = 'Install'; recovery_reference = 'synthetic-backup-one'; plan = $plan })
foreach ($changed in @(
    ([ordered]@{ action = 'Upgrade'; recovery_reference = 'synthetic-backup-one'; plan = $plan }),
    ([ordered]@{ action = 'Install'; recovery_reference = 'synthetic-backup-two'; plan = $plan })
)) {
    $rejected = $false
    try { Assert-NativeRetry ([pscustomobject]@{ input_sha256 = $first }) (Get-NativeInputDigest $changed) } catch { $rejected = $true }
    if (-not $rejected) { throw 'Changed action/recovery reference was accepted on retry' }
}
if (-not (Test-NativeReadinessHealth ([pscustomobject]@{ status = 'ok'; instance_id = [guid]::NewGuid().ToString(); area_instance_id = $instance.ToString() }) $instance)) { throw 'Configured area readiness rejected' }
if (Test-NativeReadinessHealth ([pscustomobject]@{ status = 'ok'; instance_id = $instance.ToString() }) $instance) { throw 'Ephemeral runtime identity accepted as configured area identity' }
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('memory-native-staging-test-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path $temporary
try {
    $file = Join-Path $temporary 'synthetic.txt'
    [IO.File]::WriteAllText($file, 'approved')
    $payload = @([pscustomobject]@{ name = 'synthetic.txt'; sha256 = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() })
    Assert-NativePayload $temporary $payload
    [IO.File]::WriteAllText($file, 'wrong bytes')
    $rejected = $false
    try { Assert-NativePayload $temporary $payload } catch { $rejected = $true }
    if (-not $rejected -or -not (Test-Path -LiteralPath $temporary)) { throw 'Invalid staging was accepted or lost' }
} finally { Remove-Item -LiteralPath $temporary -Recurse -Force }
Initialize-NativeReleaseGuard # Compile C# only; no Windows API is invoked here.
Write-Output 'Portable native-instance binding/staging/readiness tests and release guard compilation passed'
