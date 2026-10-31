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

## Identity evidence and limits

Native sealed [`/v1/sys/seal-status`](https://openbao.org/api-docs/system/seal-status/) exposes seal parameters, progress and nonce;
it does **not** independently prove the installation's root/unseal-key generation.
The included target-adjacent attestor therefore serves an operator-maintained
expiring identity at `/codifix-agency/v1/identity`. Controller and custodians pin
that helper's separate HTTPS identity and compare the complete reviewed record:
installation ID, key generation, native process boot marker, seal type, `n`, `t`
and absolute lease expiry. They also independently compare actual native
seal-status fields and reject uninitialized, migrating, recovery or non-Shamir
seals. There is no silent downgrade if evidence is missing.

On Linux the helper verifies the native OpenBao PID, `/proc` start time and
machine boot ID for every identity read. `inspect-process PID` emits the safe
marker for enrollment. A restarted/exited native process or host reboot requires
human review and re-enrollment. The attestor must share the native process's PID
namespace and must be enrolled against the actual OpenBao process, not its
service manager. Attestor restart alone does not change an unchanged native
process marker.

**The operator's generation record is an attestation, not cryptographic proof
that an administrator has not rekeyed the running vault.** Before any rekey,
restore, upgrade, restart or generation-affecting maintenance, withdraw the
identity lease, stop/fence controller and custodians, finish maintenance, then
independently re-enroll all roles. Do not renew a lease without checking the
installation and generation. Native unseal does not support conditional
identity/nonce submissions, so the operator must prevent target/rekey/reset
changes during a live ceremony. A privileged actor bypassing this maintenance
fence is outside this pilot's trust model. The pilot must not be enabled where
that operational fence and independent evidence cannot be assured.

Unexpected identity, key generation, CA, process marker, expiry, seal/threshold,
missing state, native partial progress or interrupted ceremony stops work and
emits a secret-free `escalation` observation. A failed standing routine never
loops through fresh challenges. Manual recovery must inspect native status and
custodian replay records; there is intentionally no automatic state-delete,
nonce-reset or retry override. Missing replay state never silently initializes.
Restoring stale disk snapshots can roll back local replay protection; retain
monotonic replay state through operator-controlled backup/recovery and do not
restore it as an unattended routine.

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
3. Inspect the actual OpenBao process and enroll its marker, installation identity
   and current key generation with a bounded lease. Copy the reviewed public
   target/roster into each role's own protected configuration.
4. Start the read-only attestor with its listener file. Check its authenticated
   identity through the enrolled TLS anchor. Every role independently holds the
   matching expected identity and process marker.
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
replayed/expired/cross-target ceremonies, generation/process/seal drift, scoped
authentication, partial failure, missing and unsafe files, exclusive state locks,
duplicate custody metadata and fingerprint mismatch. A generated TLS fixture
proves enrolled trust succeeds, an unrelated certificate fails, and redirects
are rejected. The opt-in native test starts a separate file-storage OpenBao,
generates synthetic 2-of-3 bootstrap keys, distributes only those synthetic test
shares and unseals through separate custodian executable processes with actual
`status` and `controller-once` executable invocations. Fixture bootstrap holds all
**synthetic** shares solely to construct the isolated test; production binaries
have no such bootstrap/init path. No real CA, production URL, vault credentials,
Codifix database, Workshop or cloud account is used. Linux integration has been
exercised; other OSes and real distributed failure-domain isolation are not
validated by these single-host tests.
