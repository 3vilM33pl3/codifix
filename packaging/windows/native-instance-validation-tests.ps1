# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable invariants only; this is not native Windows acceptance evidence.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
$instance = [guid]'11111111-1111-4111-8111-111111111111'
$plan = Get-NativeInstancePlan 'homelab' 'windows-v3.0.0-rc.5-build.1' 'codifix.exe' 'http://127.0.0.1:55440/healthz' $instance
$legacyPlan = [ordered]@{
    service_name = 'Codifix-homelab'; account = 'NT SERVICE\Codifix-homelab'; name = 'homelab'
    release_identity = 'windows-v3.0.0-rc.5-build.1'; executable_name = 'codifix.exe'
    health_uri = 'http://127.0.0.1:55440/healthz'; expected_instance_id = $instance.ToString()
}
if ((Get-NativeInputDigest $plan) -ne (Get-NativeInputDigest $legacyPlan)) { throw 'Existing Codifix plan hash compatibility changed' }
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
$space = [guid]'22222222-2222-4222-8222-222222222222'
$audience = [guid]'33333333-3333-4333-8333-333333333333'
$gateway = Get-NativeInstancePlan 'gateway' 'windows-v3.0.0-rc.5-build.1' 'codifix-federation-gateway.exe' '' $instance $space $audience
if ($gateway.expected_space_id -ne $space.ToString() -or $gateway.status_audience_id -ne $audience.ToString() -or $gateway.health_uri) { throw 'Gateway bindings lost or bogus health URL added' }
$argsMemory = @(Get-NativeServiceArguments 'codifix.exe' 'C:\private path\codifix.toml')
$argsGateway = @(Get-NativeServiceArguments 'codifix-federation-gateway.exe' 'C:\private path\gateway.json')
if (($argsMemory -join '|') -cne '--config|C:\private path\codifix.toml|service|run') { throw 'Codifix service arguments changed' }
if ($argsGateway.Count -ne 1 -or $argsGateway[0] -cne 'C:\private path\gateway.json') { throw 'Gateway must receive exactly one positional configuration' }
foreach ($bad in @(
    @('http://127.0.0.1:9999/healthz', $space, $audience),
    @('', [guid]::Empty, $audience), @('', $space, [guid]::Empty)
)) {
    $rejected = $false
    try { $null = Get-NativeInstancePlan 'gateway' 'windows-v3.0.0-rc.5-build.1' 'codifix-federation-gateway.exe' $bad[0] $instance $bad[1] $bad[2] } catch { $rejected = $true }
    if (-not $rejected) { throw 'Incomplete or false gateway readiness accepted' }
}
$config = [pscustomobject]@{ space = $space.ToString(); authority = $instance.ToString(); credentials = @([pscustomobject]@{ instance = $audience.ToString() }) }
Assert-NativeGatewayConfig $config $space $instance $audience
$rejected = $false
try { Assert-NativeGatewayConfig $config $space $instance ([guid]::NewGuid()) } catch { $rejected = $true }
if (-not $rejected) { throw 'Unenrolled probe audience accepted' }
$ready = [pscustomobject]@{ schema_version = 1; space = $space.ToString(); authority = $instance.ToString(); audience = $audience.ToString(); observed_at = [DateTimeOffset]::UtcNow.ToString('o'); readiness_scope = 'signed_identity_current_pull_and_writer_fence_only' }
if (-not (Test-NativeGatewayReadiness $ready $space $instance $audience)) { throw 'Verified gateway probe bindings rejected' }
foreach ($change in @(
    @{ space = [guid]::NewGuid().ToString() }, @{ authority = [guid]::NewGuid().ToString() },
    @{ audience = [guid]::NewGuid().ToString() }, @{ readiness_scope = 'scm_running' },
    @{ observed_at = [DateTimeOffset]::UtcNow.AddMinutes(-2).ToString('o') }
)) {
    $bad = $ready | ConvertTo-Json | ConvertFrom-Json
    foreach ($key in $change.Keys) { $bad.$key = $change[$key] }
    if (Test-NativeGatewayReadiness $bad $space $instance $audience) { throw 'Wrong or stale gateway status binding accepted' }
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('codifix-native-staging-test-' + [guid]::NewGuid())
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
