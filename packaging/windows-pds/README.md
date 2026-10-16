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
    "name": "\\\\.\\pipe\\memory-pds-company",
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

The server listener/installer is **not delivered by this directory yet**. The
client alone is not evidence that native PDS deployment or access isolation is
complete. Native upstream Node/PDS startup, storage and pipe ACL installation
must be proven before the candidate can ship.

## Pins and validation

Retain the baseline inspected in `tests/federation-pds/README.md`: PDS distro
0.4.5027, embedded `@atproto/pds` 0.5.27, source revision
`374cf1d4ba782d4391bbb73e4e2d3f320d4846d6`. The Linux OCI digest is provenance;
it is not a Windows-native executable/package hash. Record native dependencies
and hashes separately rather than downloading an unpinned latest package.

Observed on the Linux host for this transport slice:

```sh
cargo test --locked -p mem-federation-runtime --lib
cargo test --locked -p mem-federation-runtime --test http_contract
cargo check --locked -p mem-federation-runtime --tests --target x86_64-pc-windows-gnu
cargo clippy --locked -p mem-federation-runtime --lib --bins --target x86_64-pc-windows-gnu -- -D warnings
```

The first two passed 35 and 7 tests respectively. Cross-check and library/binary
Clippy passed. All-targets Clippy is not a pass: older unrelated test modules
have existing `unwrap_used`/test-layout findings. No native runtime claim follows
from cross-compilation.

Run these in the Windows VM to exercise the actual Windows pipe/ACL APIs:

```powershell
cargo test --locked -p mem-federation-runtime --lib pds_pipe::native::tests
```

The native tests create isolated random-name pipes, preserve non-UTF-8 binary
HTTP response bytes and reject broad ACLs before any bearer bytes are sent.
Follow with the existing signed-repository, credential renewal, duplicate,
reconnect and addressed-expertise regressions against the actual native PDS.
