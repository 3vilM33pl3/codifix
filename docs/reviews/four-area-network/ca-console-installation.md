# CA console installation: inactive first-install slice

Continuation authorized by Olivier on 2026-09-20. Dev Memory task:
`dd4d6431-97e9-4399-ba85-e787dd0203b2`, memory
`ebd87bbe-d411-4158-9401-c7ee7a5cd04f`, thread
`ca-console-installer-and-recovery`, starting at `32763df4`.

## Scope

Build and test an **inactive first-install** console tool for one new named
Memory instance. This is not an upgrade of the CA's running singleton. No
operator key, real credentials, CA service, original database or existing
preview may be changed by development or verification. Tests use disposable
signing keys, synthetic configuration, private directories and isolated containers.

Required behavior:

- Independently authenticate a signed release/preparation and bind the exact
  operator-reviewed configuration hash, instance, architecture and schema claim.
- Use fixed per-instance installation and recovery paths; no CLI override of the
  live filesystem root, no package hooks, arbitrary commands or model execution.
- Refuse existing instance paths, active/pending units, activation markers,
  unsafe permissions, links, changed bytes and unsuitable staging configuration.
- Journal intent before publishing each filesystem effect. Recovery verifies
  actual bytes before reconciling an ambiguous rename; never overwrite or erase
  an unexpected path. Preserve interrupted stages and retained installations.
- Install matching executable/Web/configuration/digest files without enabling,
  starting, stopping or restarting services. No activation marker is produced.
- Allow restart recovery after a committed transaction plan, and retention of
  this inactive first install. No database rollback, schema observation, writer
  fencing, upgrade rollback or live credential rotation is claimed.

The unsigned Debian 12 candidate is still **not** an authenticated release.
Its identity remains in `ca-bookworm-release.md`. The existing release format
requires both architectures from a coherent release; do not invent a second
artifact or sign with a disposable fixture key to bypass that requirement.

## Delivery checklist

- [x] Checkpoint scope and inspect existing release, launcher and sandbox contracts.
- [x] Implement the inactive installer, protected journal and recovery commands.
- [x] Prove signature/config/path refusals, concurrent exclusion and interrupted
      publication recovery using signed synthetic fixtures.
- [x] Wire offline CI checks and document exact console preparation/recovery.
- [x] Record verification evidence and remaining real-release/CA activation gates.

## Implementation and verification — 2026-09-20

The core landed at `d655e457`, the real-CLI rehearsal at `e027b88d`, and package/CI
integration at `703145a1`. The [operator guide](../../../scripts/areas/releases/console-README.md)
documents install, inspect, finish and retain commands, exact trust/configuration
requirements and the independent activation gates. Future Debian builds include
the root-owned mode-0644 tools and require Python 3.11 or newer. No new binary or
Debian candidate was built by this slice. Existing candidate hashes and the
candidate handoff remain unchanged.

The scoped offline validation completed successfully:

| Check | Result |
| --- | --- |
| Console library/signed-fixture tests | 22 passed |
| Prepared-instance tests | 20 passed |
| Release/signature/archive tests | 17 passed |
| Clean instance launcher tests | 3 passed |
| CA preflight tests | 13 passed |
| Debian lifecycle and packaging fixtures | Passed for native/cross layouts and dependency bounds |
| Root-only CLI container rehearsal | 8 passed; 25 exact read-only status queries; no activation markers |
| `cargo fmt --check`, `scripts/check-boundaries.sh`, `git diff --check` | Passed |

The console suite includes actual `SIGKILL` after each installation-tree rename,
fresh-process lock recovery, signature/configuration/path refusals, interrupted
retention and refusal to adopt an identical-byte replacement inode. It does not
simulate hardware power loss. The journal is bounded, fsynced and preserves
incomplete copy generations; this is not a claim about failing storage or network
filesystem semantics.

Local evidence:

- Aggregate check log:
  `/data/tmp/memory-console-validation-20260920.f3Rg3IdK/offline-tests.log`.
  SHA-256: `430a2796b3ecc5847f1e8bb571c64cfcfde821360b17252a38918ea3eb2074bf`.
- Root CLI invocation, source hashes, structured output, result and cleanup:
  `/data/tmp/memory-console-cli-9a9m4b6n/`.
- Exact local container image:
  `sha256:ed8f07b19272db7d8759355733e5564a2745a4059f44b7065b130abc97668fac`.
- Tested `console.py` SHA-256:
  `2fd6c35865e43ae63d11c5d9d01ba9498fc823435d2b2294cb060cf7bfde12fe`.
- Tested CLI rehearsal SHA-256:
  `91234684bc93f7ef3acb4d25962e3d55b1aac1b5df1481af3e0a018b326e046d`.

The CLI ran as root **only inside** a network-none, capability-free,
read-only-root container. It used disposable signing keys and synthetic ELF
identity headers, not a running Memory artifact. A fixed fake `systemctl` accepted
only the exact read-only query. This proves entrypoint/file-placement behavior,
not real-systemd activation, ARM64 runtime compatibility or a native CA sandbox.
No host installation paths, database, provider, real credentials or signing key
were used. The rehearsal container was removed; its private evidence remains.
All 49 preserved candidate handoff files were rechecked successfully against its
unchanged `SHA256SUMS` (digest
`334f51844fb560eab55f91020aa96ba04c4728ec821fe041e844add794650ed8`).
CI is wired to run the console unit suite; this is local execution evidence, not
a claim that remote CI ran. No Rust implementation or application Web behavior
changed, so full workspace/database/Web suites were not repeated for this slice.

The initial closed configuration permits only loopback PostgreSQL, matching the
default named-unit network boundary. This is **not** a working CA-to-Monolith
database configuration. Separately provisioned Trust credentials and a reviewed
remote-database egress/sandbox profile are required for that deployment option.
The tool neither proves database independence nor observes its schema; the exact
schema argument remains an operator assertion.

The dev Memory completion workflow succeeded with one targeted capture
`8b837a4b-2074-4c03-a98f-eab9471f02af` and four implementation memories (curation
run `36954e34-58c7-4e1f-80aa-d16065b6e843`, no replacement proposals). Its ordinary
background jobs were queued, not asserted complete. Task checkpoint/completion
records are the intentional Memory writes for this work; no deployment data
migration or cutover was performed.

## Remaining independent gates

Complete release assembly and operator signing, CA-native ABI/sandbox checks,
offline-data reconciliation, distinct database/credentials, final backup/restore
and delta review, original writer fencing and explicit cutover approval. Pi stays
separate. The first-install tool must not claim those gates merely because its
filesystem transaction completed.
