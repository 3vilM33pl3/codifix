# Native Windows private PDS transport

The gateway can dispatch its existing PDS HTTP requests over a local Windows
named pipe. This is a transport adapter, not a new ATProto profile. The logical
HTTPS origin, enrolled repository DID, signing key, credential generation,
immutable record IDs and signed-CAR verification remain unchanged.

Add an operator-owned field to the gateway JSON configuration:

```json
{
  "pds": "https://pds.company.example",
  "pds_named_pipe": {
    "name": "\\\\.\\pipe\\codifix-pds-company",
    "server_sid": "S-1-5-21-100-200-300-1001",
    "client_sid": "S-1-5-21-100-200-300-1002"
  }
}
```

The example SIDs are placeholders, not identities to install. Provision separate
PDS and gateway user/virtual-service identities and obtain their actual SIDs.
This field is rejected on non-Windows hosts. Omitting it preserves HTTPS. The
configured HTTPS address is the stable credential/repository identity; a pipe
failure never causes a PDS request to be retried over TCP.

## Server requirements

The native PDS server adapter must create the pipe before accepting clients with:

- A byte-mode, local-only pipe (`PIPE_REJECT_REMOTE_CLIENTS`) and a uniquely owned
  first instance (`FILE_FLAG_FIRST_PIPE_INSTANCE`).
- Owner equal to `server_sid`, and a protected DACL granting only the PDS and
  gateway identities, SYSTEM and administrators access. Do not create a broadly
  accessible pipe and tighten its permissions after accepting connections.
- The same HTTP routing/authentication and unmodified request/response bodies as
  the pinned upstream server. No public TCP bypass into its repositories.

The client validates the connected handle's server process token and owner SID,
and inspects every effective allow ACE before sending authentication bytes.
Unknown access-control ACE forms and broad principals are rejected. It disables
impersonation beyond identification, rejects remote/ambiguous pipe paths, bounds
the entire exchange to 20 seconds and caps bodies at the signed-CAR limit.

`codifix-pds-pipe.exe` supplies the native listener. It creates the protected ACL
before the first connection, grants the gateway data-write rather than
pipe-instance-creation rights, and launches Node using private length-framed
stdin/stdout. It never opens a PDS TCP listener. Requests execute serially, with
bounded frames and deadlines. Ambiguous child failure stops the broker rather
than replaying a write. Existing signed read-back reconciles publication outcomes.

The Node entry point calls pinned `PDS.fromEnv()` and starts its sequencer, then
passes unmodified HTTP bodies through Node's real HTTP parser into `pds.app` via
an in-memory duplex connection. It does not call `PDS.start()` or `listen()`.
This matches the inspected initialization preceding the pinned upstream listen;
the same 90-second keep-alive setting is applied and `pds.destroy()` runs on EOF.

Supply a private operator JSON file to `codifix-pds-pipe.exe`:

```json
{
  "schema_version": 1,
  "pipe": {
    "name": "\\\\.\\pipe\\codifix-pds-company",
    "server_sid": "S-1-5-21-100-200-300-1001",
    "client_sid": "S-1-5-21-100-200-300-1002"
  },
  "node": "C:\\MemoryPrivate\\runtime\\node.exe",
  "node_sha256": "EXACT_VERIFIED_NATIVE_NODE_SHA256",
  "entry": "C:\\MemoryPrivate\\pds\\pds-server.mjs",
  "entry_sha256": "EXACT_REVIEWED_ENTRY_SHA256",
  "helper_sha256": "EXACT_REVIEWED_HTTP_STDIO_SHA256"
}
```

These example paths/SIDs/hashes are not ready-to-run credentials or installers.
The PDS service identity must own and be allowed to read the protected inputs.
The broker verifies its own identity and hashes of Node, entry and helper before
spawning. It forwards only `PDS_*`, essential Windows process variables,
`NODE_EXTRA_CA_CERTS` and `LOG_LEVEL`, not provider credentials or `NODE_OPTIONS`.
Keep `PDS_DEV_MODE=false`, SSRF protection enabled and the existing private-PDS
configuration. Startup requires exact Node 24.18.1 and PDS 0.5.27.

The **native dependency tree/installer and Windows runtime validation remain
undelivered**. The complete dependency tree still needs an immutable native
manifest and appropriate private ACLs. Do not claim complete PDS deployment or
access isolation merely because its broker cross-compiles.

## Pins and validation

Retain the baseline inspected in `tests/federation-pds/README.md`: PDS distro
0.4.5027, embedded `@atproto/pds` 0.5.27, source revision
`374cf1d4ba782d4391bbb73e4e2d3f320d4846d6`. The Linux OCI digest is provenance;
it is not a Windows-native executable/package hash. Record native dependencies
and hashes separately rather than downloading an unpinned latest package.

Observed on the Linux host for this transport slice:

```sh
cargo test --locked -p codifix-federation-runtime --lib
cargo test --locked -p codifix-federation-runtime --test http_contract
cargo check --locked -p codifix-federation-runtime --tests --target x86_64-pc-windows-gnu
cargo clippy --locked -p codifix-federation-runtime --lib --bins --target x86_64-pc-windows-gnu -- -D warnings
```

The first two passed 37 and 7 tests respectively. Cross-check and library/binary
Clippy passed. All-targets Clippy is not a pass: older unrelated test modules
have existing `unwrap_used`/test-layout findings. No native runtime claim follows
from cross-compilation.

Run these in the Windows VM to exercise the actual Windows pipe/ACL APIs:

```powershell
cargo test --locked -p codifix-federation-runtime --lib pds_pipe::native::tests
```

The native tests create isolated random-name pipes, preserve non-UTF-8 binary
HTTP response bytes and reject broad ACLs before any bearer bytes are sent.
Follow with the existing signed-repository, credential renewal, duplicate,
reconnect and addressed-expertise regressions against the actual native PDS.

`node --test packaging/windows-pds/http-stdio.test.mjs` passed three fake-app
tests for binary request/response preservation, oversized/interrupted frames,
oversized outputs and execution deadlines. A separate smoke invoked this exact
entry/helper against the pinned Linux OCI image, with network disabled, tmpfs
state and synthetic credentials: READY, `describeServer` HTTP 200 and clean EOF
shutdown passed. This tests the actual upstream adapter without a TCP listener;
it is **not** native Windows database/SQLite, pipe ACL or signed-repository proof.
`probe-stdio.mjs` accepts an explicit disposable command for repeating that smoke
and prints metadata only. Never point it at live deployments.
