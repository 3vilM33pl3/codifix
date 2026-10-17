# Native private PDS service

`native-instance.ps1` can install `memory-pds-pipe.exe` through the existing
`memory-windows-service.exe` SCM wrapper. Node is its unprivileged child; no PDS
TCP listener, Docker, WSL or registry environment secrets are introduced.
Portable policy tests and Windows cross-compilation are not native installation
acceptance. Account creation, signed repositories and federation remain separate
checks from the public-XRPC readiness probe.

## Prepare the exact inputs

Use an elevated Windows PowerShell session and the checksummed candidate ZIP and
manifest. First install a `memory-native-pds-runtime` prerequisite bundle using
`packaging/windows/native-prerequisites.ps1`; retain its completed operation UUID,
manifest and digest. The service requires the separately recorded
`windows-sha256-did-paths-v1` patch. An unpatched runtime is not accepted. Keep
actual Node/PDS/native-library notices with that bundle.

For instance `company-pds`, the fixed paths are:

- State: `%ProgramData%\MemoryLayer\instances\company-pds`
- Configuration: `%ProgramData%\MemoryLayer\configurations\company-pds`
- Pipe: `\\.\pipe\memory-pds-company-pds`
- SCM account: `NT SERVICE\MemoryLayer-company-pds`

Obtain the service SID with `sc.exe showsid MemoryLayer-company-pds`, and the
distinct gateway SID with `sc.exe showsid MemoryLayer-company-gateway`. These
are deterministic named-service identities, not user-provided area labels.
Installation verifies the actual configured SCM account before starting it.

Create a private operator-owned environment JSON outside the package. Permit
only your operator, SYSTEM and Administrators to read it. Use fresh credentials
locally; never put their values in command arguments or logs. All values are
strings. Required settings include:

```text
PDS_HOSTNAME = the existing private-profile hostname
PDS_DATA_DIRECTORY = exact state path above + \data
PDS_BLOBSTORE_DISK_LOCATION = exact state path above + \data\blobs
PDS_DEV_MODE = false
PDS_DISABLE_SSRF_PROTECTION = false
PDS_INVITE_REQUIRED = true
PDS_JWT_SECRET = locally generated secret
PDS_ADMIN_PASSWORD = locally generated secret
PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX = existing approved key
```

Additional bounded `PDS_*` strings are allowed; unrelated provider variables are
not. Compute SHA-256 over the exact JSON bytes. The installed copy is
`%ProgramData%\MemoryLayer\configurations\company-pds\pds-environment.json`,
readable by this PDS account but not the gateway or participant accounts.

Prepare broker JSON with `schema_version: 1`, `pipe` containing the exact name,
server SID and client/gateway SID, and these fields from the installed runtime
binding and its manifest:

```text
node = runtime binding node_executable
entry = runtime binding pds_entrypoint
node_sha256 = payload node/node.exe hash
entry_sha256 = payload pds/pds-server.mjs hash
helper_sha256 = payload pds/http-stdio.mjs hash
environment.path = installed environment path above
environment.sha256 = exact environment JSON hash
```

## Preview, install and recover

Use a new operation UUID and retain it with the exact input files. This example
uses variables for locally reviewed paths/hashes, never secret values:

```powershell
$inputs = @{
    Name = 'company-pds'; OperationId = $operationId
    PackagePath = $candidateZip; ManifestPath = $candidateManifest
    ManifestSha256 = $candidateManifestHash
    ConfigPath = $brokerConfig; ExecutableName = 'memory-pds-pipe.exe'
    ExpectedInstanceId = $installationId
    NativeRuntimeManifestPath = $pdsManifest
    NativeRuntimeManifestSha256 = $pdsManifestHash
    NativeRuntimeOperationId = $completedRuntimeOperation
    PdsEnvironmentPath = $privateEnvironmentJson
    PdsEnvironmentSha256 = $environmentHash
    RecoveryReference = $retainedBackupReference
}
.\native-instance.ps1 -Action Preview @inputs
# Review the output; use precisely its input_sha256:
.\native-instance.ps1 -Action Install @inputs `
    -ExpectedInputSha256 $reviewedInputHash -DatabaseCompatibilityConfirmed
```

The installation UUID identifies the managed installation, not a verified PDS
DID. `DatabaseCompatibilityConfirmed` also applies to retained PDS state; the
operator performs no database backup or rollback. The selected runtime is
read-execute only for PDS, and only this service state is writable.

Retry an interrupted operation with the same UUID and exact inputs. Journals
remain under `%ProgramData%\MemoryLayer\operations`; do not delete them or
restore old PDS bytes over accepted writes. Existing environment bytes must
match; this slice does not silently replace credentials or adopt foreign SCM
services. For upgrades, use `Preview -PreviewAction Upgrade` then `Upgrade` with
a strictly newer installer build and the newly reviewed digest.

An elevated operator can run:

```powershell
memory-pds-pipe.exe check-status $installedBrokerConfig `
    --service-name MemoryLayer-company-pds
```

It checks the protected config, exact pipe peer SID/DACL and bounded
`describeServer` response. Output is fresh sanitized metadata only, explicitly
`verified_pipe_peer_and_public_xrpc_only`; it does not expose credentials or
claim repository/publication readiness. Ordinary participants cannot use this
administrator-only config reader. The gateway continues using its own exact
pipe policy and enrolled PDS credentials.
