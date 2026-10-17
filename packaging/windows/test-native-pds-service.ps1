# SPDX-License-Identifier: AGPL-3.0-or-later
# Portable service-role policy only. No SCM, credential or PDS execution.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native-instance-functions.ps1')
. (Join-Path $PSScriptRoot 'native-prerequisites-functions.ps1')
function Reject-Pds([scriptblock]$Action){$failed=$false;try{$null=&$Action}catch{$failed=$true};if(-not $failed){throw 'Invalid native PDS service policy accepted'}}
function Clone-Pds($Value){$Value | ConvertTo-Json -Depth 20 | ConvertFrom-Json}
$instance=[guid]'11111111-1111-4111-8111-111111111111'
$plan=Get-NativeInstancePlan 'company-pds' 'windows-v3.0.0-rc.5-build.1' 'codifix-pds-pipe.exe' '' $instance
if($plan.account -cne 'NT SERVICE\Codifix-company-pds' -or $plan.readiness_mode -cne 'native_pipe_public_xrpc_v1'){throw 'PDS service role mismatch'}
Reject-Pds {Get-NativeInstancePlan 'company-pds' 'windows-v3.0.0-rc.5-build.1' 'codifix-pds-pipe.exe' 'http://127.0.0.1/healthz' $instance}
Reject-Pds {Get-NativeInstancePlan 'company-pds' 'windows-v3.0.0-rc.5-build.1' 'codifix-pds-pipe.exe' '' $instance $instance}
if(@(Get-NativeServiceArguments 'codifix-pds-pipe.exe' 'private-config.json') -join ',' -cne 'private-config.json'){throw 'PDS service command gained ambient/secret arguments'}
$root=Join-Path ([IO.Path]::GetTempPath()) 'synthetic-native-pds-policy'
$state=Join-Path $root 'state';$configuration=Join-Path $root 'configuration';$runtimeRoot=Join-Path $root 'runtime';$hash='a'*64
$runtime=[pscustomobject]@{runtime_kind='codifix-native-pds-runtime';node_executable=(Join-Path $runtimeRoot 'node/node.exe');pds_entrypoint=(Join-Path $runtimeRoot 'pds/pds-server.mjs')}
$patch=[pscustomobject]@{layout='windows-sha256-did-paths-v1';patch_sha256='79c8c8378fdab250457e663d2d48518cb28dbabf5a81b3ae832aa70a7b860441';files=@(
    [pscustomobject]@{name='actor-store/actor-store.js';original_sha256='33ba4bcf0b0afb06868b80a4cb0025388a2a7e8006338ce80a70785d2079af4f';patched_sha256='c192431cb2de642bbe5c8256fbe30f0de6fe5dc23f3b20b8baec71e969909965'},
    [pscustomobject]@{name='disk-blobstore.js';original_sha256='cc18ec422b23ba3bc06baea321146e512ce9cb14608ec90ccfe5e9bc71359811';patched_sha256='9da2aa49a53b58826ae1b24fd7ba8d4fbbe7c9aaf9c7ee8a24c91c792b736f5a'})}
$runtimePlan=[pscustomobject]@{path_patch=$patch;payload_files=@('node/node.exe','pds/pds-server.mjs','pds/http-stdio.mjs' | ForEach-Object {[pscustomobject]@{name=$_;sha256=$hash}})}
$config=[pscustomobject]@{schema_version=1;pipe=[pscustomobject]@{name='\\.\pipe\codifix-pds-company-pds';server_sid='S-1-5-80-1-2-3-4-5';client_sid='S-1-5-80-6-7-8-9-10'}
    node=$runtime.node_executable;entry=$runtime.pds_entrypoint;node_sha256=$hash;entry_sha256=$hash;helper_sha256=$hash
    environment=[pscustomobject]@{path=(Join-Path $configuration 'pds-environment.json');sha256=$hash}}
Assert-NativePdsServiceConfig $config $runtime $runtimePlan 'company-pds' $state $configuration $hash
foreach($change in @({param($m)$m.node='foreign-node.exe'},{param($m)$m.node_sha256=('b'*64)},
    {param($m)$m.pipe.client_sid=$m.pipe.server_sid},{param($m)$m.pipe.name='\\remote\pipe\codifix-pds-company-pds'},
    {param($m)$m.environment.path='foreign-secret.json'},{param($m)$m.environment.sha256=('b'*64)})){
    $copy=Clone-Pds $config;&$change $copy;Reject-Pds {Assert-NativePdsServiceConfig $copy $runtime $runtimePlan 'company-pds' $state $configuration $hash}
}
$environment=[pscustomobject]@{PDS_HOSTNAME='pds.synthetic.example';PDS_DATA_DIRECTORY=(Join-Path $state 'data');PDS_BLOBSTORE_DISK_LOCATION=(Join-Path $state 'data\blobs')
    PDS_DEV_MODE='false';PDS_DISABLE_SSRF_PROTECTION='false';PDS_INVITE_REQUIRED='true';PDS_JWT_SECRET='synthetic-secret';PDS_ADMIN_PASSWORD='synthetic-secret';PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX=('a'*64)}
Assert-NativePdsEnvironment $environment $state
foreach($change in @({param($m)$m.PDS_DATA_DIRECTORY='foreign-state'},
    {param($m)$m.PDS_DEV_MODE='true'},{param($m)$m.PDS_DISABLE_SSRF_PROTECTION='true'},
    {param($m)$m.PDS_INVITE_REQUIRED='false'},{param($m)$m | Add-Member -NotePropertyName OPENAI_API_KEY -NotePropertyValue 'never'},
    {param($m)$m.PDS_ADMIN_PASSWORD='x'*8193})){
    $copy=Clone-Pds $environment;&$change $copy;Reject-Pds {Assert-NativePdsEnvironment $copy $state}
}
$status=[pscustomobject]@{schema_version=1;server_sid=$config.pipe.server_sid;client_sid=$config.pipe.client_sid;pipe_name=$config.pipe.name;readiness_scope='verified_pipe_peer_and_public_xrpc_only';observed_at=[DateTimeOffset]::UtcNow.ToString('o')}
if(-not(Test-NativePdsReadiness $status $config.pipe.server_sid $config.pipe.client_sid $config.pipe.name)){throw 'Bounded PDS readiness rejected'}
foreach($change in @({param($m)$m.server_sid='S-1-5-80-9-9-9-9-9'},
    {param($m)$m.observed_at=[DateTimeOffset]::UtcNow.AddMinutes(-2).ToString('o')},
    {param($m)$m.readiness_scope='unverified-process-only'})){
    $copy=Clone-Pds $status;&$change $copy
    if(Test-NativePdsReadiness $copy $config.pipe.server_sid $config.pipe.client_sid $config.pipe.name){throw 'Invalid PDS health accepted'}
}
if(($plan | ConvertTo-Json -Depth 8).Contains('synthetic-secret')){throw 'Private environment reached public plan'}
Write-Output 'Native PDS role/runtime/environment/readiness policy tests passed (portable only; SCM acceptance pending).'
