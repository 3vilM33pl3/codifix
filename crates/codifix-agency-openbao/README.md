# OpenBao ceremony pilot

An independent deterministic controller observes an enrolled OpenBao target and
asks a standing quorum of independently configured custodians to unseal it. Each
custodian reads only its own private share and sends it directly to OpenBao. The
controller, attestor, Codifix database, Workshop, model provider and cloud are
absent from the share path. This pilot has no LLM or arbitrary command runner.

**This is an isolated-test implementation, not a deployed production service.**
It preserves the enrolled Shamir seal, share count and threshold. Auto-unseal,
recovery shares, initialization, rekey, reset, seal migration and target discovery
are deliberately outside the executable. The actual native-vault test below
initializes only its own temporary synthetic vault.

## Roles and trust

| Role | Has | Network operations |
| --- | --- | --- |
| Controller | enrolled public identity, per-custodian scoped tokens, attestor reader token, replay state | authenticated identity GET; native seal-status GET; ceremony POST to each selected custodian |
| Custodian | independently enrolled identity/roster, **one** local share, its scoped controller token, attestor reader token, replay state | authenticated identity GET; native seal-status GET; direct native unseal POST |
| Attestor | enrolled expiring public identity, reader token, native process binding, its own TLS server key | serves read-only authenticated identity; never receives or proxies a share |
| Operator | enrollment/provisioning authority | reviews target/process, generations, failure domains, maintenance and recovery |

Run custodians under different OS identities on independently administered hosts
or suitable separately secured devices. A 0700 directory owned by one role and
0400/0600 owner-only files are required. Each role checks its own file ownership,
mode, regular-file type, link count and symlink-free parent path. The controller
has no share-file configuration and cannot request a share over any API.
**Separate process names or multiple directories under one shared account are
not independent custody.** Root or a host administrator remains trusted; this
crate does not manufacture physical independence or prevent administrator access.

The independent roster on every custodian rejects duplicate custodian IDs,
share fingerprints and failure-domain labels. `share_id` is lowercase SHA-256 of
the trimmed share, calculated locally during independent provisioning. A
custodian verifies its own share against that fingerprint before submission.
Operator labels must describe real failure boundaries; the software cannot
prove that two machines are independently administered. Threshold is the native
threshold; roster size may be between threshold and native share count. Selection
is the first threshold members of the reviewed roster, never an automatic
fallback to unreviewed peers.

Production endpoints require HTTPS with hostname verification and a dedicated
enrolled trust anchor; system/public roots, HTTP proxies and redirects are
disabled. Pin contents participate in the persisted target binding. CA signing
keys stay outside these roles. Use a dedicated per-target/per-role CA or tightly
controlled enrolled certificate; a broad organizational CA weakens identity
isolation. Listener TLS keys are separate from controller capability tokens.
Controller tokens must be distinct per custodian and distinct from the attestor
reader capability. Bearer capabilities are high-entropy preprovisioned secrets,
not OpenBao root or application tokens. No token provisioning API exists.

`fixture_localhost_http: true` explicitly permits only literal `127.0.0.1` or
`::1` HTTP origins/listeners for synthetic isolated tests. Controller target,
identity and custodian peers must all use the same fixture/production mode. This
switch is never appropriate for real shares.

## Stable installation and fresh boot evidence

Native sealed [`/v1/sys/seal-status`](https://openbao.org/api-docs/system/seal-status/)
does not independently prove the installation's root/unseal-key generation. The
included target-adjacent attestor serves fresh evidence at
`/codifix-agency/v1/identity?challenge=UUID`. Each reader supplies an unpredictable
challenge, verifies its echo and a maximum 30-second response lifetime, then
checks the complete stable enrolled identity: installation ID, key generation,
reviewed native binding, Shamir seal type, `n`, `t` and absolute authorization
lease. Actual native seal-status is independently checked as well.

**Ordinary restart or reboot of this known installation is autonomous.** The
attestor reads the live PID from the protected file maintained automatically by
OpenBao's [`pid_file` configuration](https://openbao.org/docs/configuration/).
It obtains fresh Linux PID/start-time/machine-boot evidence and verifies all
stable bindings before issuing a response:

- the actual loaded executable path and content digest, including matching
  on-disk executable identity;
- a digest of the exact reviewed invocation and a digest of the independently
  enrolled native configuration file;
- the protected local storage directory's device/inode and an installation
  record inside that directory, matching enrolled installation and generation;
- an explicit `maintenance: false` record and an unexpired authorization lease.

These stable fingerprints remain in controller and custodian replay enrollment.
New PIDs and machine boot markers do not change enrollment or require approval.
The attestor must share the native PID namespace and run under an account that
can safely read the native configuration, private PID file and installation
record. It holds no custody share. Use a reviewed, controlled native service
launch environment; this pilot fingerprints argv and configuration, not the
entire process environment. To avoid repeatedly hashing a large image, the
attestor caches a successful digest only for that process marker and loaded
executable inode/metadata. A restart, exec or replacement invalidates that cache.
The native config, storage identity, installation/generation and maintenance
record are checked on every request.

**The generation record remains an operator attestation, not cryptographic proof
that a privileged administrator has not rekeyed a running vault.** Before rekey,
restore, binary/config/storage changes or generation-affecting maintenance, set
`maintenance: true`, fence controller and custodians, perform maintenance, then
review and re-enroll changed bindings before clearing the fence. Do not renew an
authorization lease without checking installation and generation. The pilot
cannot be enabled where this maintenance fence and independent evidence cannot
be assured. Native unseal has no atomic conditional identity/nonce operation;
prevent rekey/reset/installation changes during a live ceremony. Routine restart
of an unchanged installation needs no maintenance edit or re-enrollment.

Unknown executable/config/invocation/storage, changed installation or generation,
maintenance signals, changed CA, expired authorization, unsupported seal/threshold,
missing replay state, native partial progress or interrupted ceremonies stop
submission and emit a secret-free `escalation` observation. Storage device/inode
is intentionally conservative: a different mount topology, replaced storage or
restored directory requires review even if paths look unchanged. Missing PID or
an unavailable native process never yields an attestation. Arrange service start
order or the independent runtime's readiness checks so the native PID/API and
attestor are ready before dispatching the standing routine.

A failed started ceremony never loops through fresh challenges. Manual recovery
must inspect native status and custodian replay records; there is no automatic
state-delete, nonce-reset or retry override. Missing replay state never silently
initializes. Restoring stale disk snapshots can roll back local replay protection;
retain monotonic replay state through reviewed backup/recovery and do not restore
it as an unattended routine.

## Bootstrap and executable

Build with `cargo build -p codifix-agency-openbao --locked`. There is no service
installation, enrollment, live deployment or production access in this change.

The JSON templates in `examples/` contain invalid enrollment placeholders and
an expired lease. They must be independently reviewed and replaced. Configuration,
CA certificates, listener TLS material, tokens, shares and replay state all live
in private role-owned directories outside the sealed vault. Never put a share or
token in an argument, shell history, environment variable, prompt, message,
observation or diagnostic. Provision files through your secure offline process;
there is no controller-side share-import helper.

1. Preserve the current seal and record native `n`/`t` without initialization or
   migration. Independently provision each existing share to its custodian.
2. Establish dedicated TLS identities and distinct scoped controller tokens.
   Supply only the custodian's own share and controller capability to that host.
   Provision the identity-reader token separately; it cannot authorize a ceremony.
3. Configure native OpenBao `pid_file` in a private native-owned directory and
   use `UMask=0077` so each ordinary restart writes its own protected current PID.
   Provision the installation/generation/maintenance record inside the private
   storage directory, with no custody share there. Inspect the specifically
   selected actual process with `inspect-native`, and independently review its
   binary, invocation, config, local storage, installation and current generation.
   The inspector emits paths and fingerprints; it never prints argv/config contents.
4. Enroll that stable binding and installation/generation with a bounded lease.
   Copy the reviewed public target/roster into each role's own protected config.
   Start the read-only attestor and check its authenticated, fresh boot evidence
   through its enrolled TLS anchor. Every role independently holds the matching
   stable identity. `examples/installation.json` starts with maintenance enabled;
   clear it only after the enrollment review.
5. Initialize each empty local replay state once with `init-custodian CONFIG` or
   `init-controller CONFIG`. Existing files are never overwritten. These commands
   initialize local state only; they cannot initialize an OpenBao vault.
6. Start the custodian services; run `status CONTROLLER_CONFIG`, then
   `controller-once CONTROLLER_CONFIG` for the reviewed standing quorum. Use
   `controller-watch CONTROLLER_CONFIG --interval-seconds 15` for continuous
   observation. Any escalation exits the watcher with failure for human handling.

Commands:

```text
codifix-agency-openbao inspect-process PID
codifix-agency-openbao inspect-native PID CONFIG_FILE STORAGE_DIRECTORY INSTALLATION_FILE
codifix-agency-openbao attestor-serve ATTESTOR_CONFIG LISTENER_CONFIG
codifix-agency-openbao init-custodian CUSTODIAN_CONFIG
codifix-agency-openbao custodian-serve CUSTODIAN_CONFIG LISTENER_CONFIG
codifix-agency-openbao init-controller CONTROLLER_CONFIG
codifix-agency-openbao status CONTROLLER_CONFIG
codifix-agency-openbao controller-once CONTROLLER_CONFIG
codifix-agency-openbao controller-watch CONTROLLER_CONFIG --interval-seconds 15
```

The sample systemd unit is a reviewable starting point, not an installer. Provide
separate accounts and paths, disable core dumps, keep shares out of persistent
logs, and avoid automatic restarts after a fail-closed watcher exit. Observe and
escalate that exit through the independent agency runtime. Runtime/library output
contains only safe identity/status, ceremony IDs, sequences, custodian IDs and
reason codes. Errors never include response bodies, URLs or secret-file contents.
Share buffers and request-body ownership use zeroization, but this is not a
hardware enclave and does not guarantee erasure of every TLS/kernel/library copy.

## Runtime integration

The standalone executable emits newline-delimited JSON `Observation` records on
stdout. Constant, secret-free error descriptions go to stderr; nonzero exit
requires escalation. `controller-watch` polls locally without a database or
model. A caller can instead use:

```rust,ignore
transport::controller_tick(&config, |event: Observation| {
    // Persist only this typed, share-free event in your task runtime.
}).await?;
```

`ControllerConfig` contains the target, reviewed roster, peer HTTPS endpoints,
per-peer token **paths**, and its protected local state path. `verified_status`
only observes. Library callers must authenticate access before directly exposing
`custodian_submit`; `custodian_router` already enforces the scoped token.

A ceremony has a fresh random challenge, monotonically increasing sequence and
maximum 120-second lifetime. The controller fsyncs active state before contacting
custodians. Each custodian fsyncs replay consumption before reading its share,
checks selected membership, expected native progress/nonce, fresh attestation and
its own fingerprint, then submits once. File locks serialize local instances.
Responses contain only safe acknowledgement fields. A lost response or crash can
leave consumed state without a successful unseal; this intentionally requires
human recovery instead of risking an autonomous replay.

## Validation

```bash
cargo fmt -p codifix-agency-openbao --check
cargo test -p codifix-agency-openbao --all-targets --locked
cargo clippy -p codifix-agency-openbao --all-targets --locked -- -D warnings
# Explicit opt-in; requires `bao` on PATH, creates only a disposable local vault:
cargo test -p codifix-agency-openbao --test ceremony actual_openbao_shamir_two_of_three --locked -- --ignored
```

Tests exercise actual HTTP custodian and attestor routers, native nonce/progress,
replayed/expired/cross-target ceremonies, generation/config/storage/process/seal drift, scoped
authentication, partial failure, missing and unsafe files, exclusive state locks,
duplicate custody metadata and fingerprint mismatch. A generated TLS fixture
proves enrolled trust succeeds, an unrelated certificate fails, and redirects
are rejected. The opt-in native test starts a separate file-storage OpenBao,
generates synthetic 2-of-3 bootstrap keys, distributes only those synthetic test
shares and unseals through separate custodian executable processes with actual
`status` and `controller-once` executable invocations. It then stops and restarts
the native vault sealed, verifies a fresh native process marker, and automatically
unseals a second time using the unchanged enrollment and replay state. Fixture bootstrap holds all
**synthetic** shares solely to construct the isolated test; production binaries
have no such bootstrap/init path. No real CA, production URL, vault credentials,
Codifix database, Workshop or cloud account is used. Linux integration has been
exercised; other OSes and real distributed failure-domain isolation are not
validated by these single-host tests.
