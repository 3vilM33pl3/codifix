# Pinned Pi control probe and broker-only child (not activated)

This Linux-only library is a credential-free, bounded **control protocol probe**
for Pi Rust v0.5.1. It is not a provider transport, executable installer, service,
OS/network sandbox, or permission to activate Pi. The normal test suite uses a
local synthetic Python fixture. Separate isolated AMD64 diagnostics confirmed
real Pi's credential-free blocked startup and a complete control handshake using
a fixed invalid, non-secret, test-only auth-presence token. Neither made provider
requests or enabled production execution.

## Public boundary

`ProbeConfig::new` accepts only an explicit local executable, an existing
current-user-owned mode-0700 scratch parent, and AMD64/ARM64 architecture. The
executable hash is fixed per architecture; callers cannot override it or add
arguments, environment variables, prompts, credentials, or RPC commands. Path
components cannot be symlinks. Scratch ancestors must be root/current-user owned
and not group/other writable, except sticky directories with a trusted owner.
This protects against other users, not hostile code running as the same UID.

The executable is copied into fresh private scratch, hashing the exact copied
bytes before making the copy executable. Input is a regular nonempty file capped
at 512 MiB. Every launch uses `env_clear`, fresh HOME/XDG/agent/config/session and
working directories, fixed settings disabling retries/failover/compaction, and
fixed CLI flags requesting disabled tools, extensions, skills, context files, prompt
templates, themes, sessions, and migrations. No shell is involved.
The later real-provider diagnostic below establishes that the tool flag is not
sufficient in the pinned upstream build; do not interpret these flags as a
verified empty tool registry.

The only RPC requests are correlated `set_auto_retry(false)`,
`set_auto_compaction(false)`, and `get_state`. State must confirm exactly
`openai/gpt-5.6-sol`, thinking off, disabled retry/compaction, no streaming/compacting,
zero messages/pending messages, and null session file/name. Unexpected,
uncorrelated, replayed, malformed, duplicate safety fields, or trailing frames
fail closed. Known diagnostic metadata is ignored, never copied into reports.

Bounds: 256-byte request, 16 KiB stdout frame, 64 KiB stdout total, 8 KiB stderr,
and one shared preparation/startup/handshake/exit deadline (default 10 seconds,
maximum 30), followed by at most one second of kill-and-reap waiting. A transient
pre-exec `ETXTBSY` alone may retry within that same deadline; no launched command
or provider request is retried. Reports contain typed events/byte counts only,
never Pi output, credentials, prompts, local paths or PIDs. Cleanup uncertainty
is explicit and preserves credential-free scratch instead of removing a live
process's files. Cancellation of the caller's wait leaves the bounded driver
running to clean up; runtime shutdown only has Tokio's kill-on-drop/reaper
fallback, not a confirmed reap report. Descendant-process isolation is not
implemented here.

## Artifact provenance (checked 2026-09-17)

The pin is upstream commit
[`7c018475d54d4e09c907691e47a520f1f09e3288`](https://github.com/Dicklesworthstone/pi_agent_rust/commit/7c018475d54d4e09c907691e47a520f1f09e3288).
The [v0.5.1 release](https://github.com/Dicklesworthstone/pi_agent_rust/releases/tag/v0.5.1),
[SHA256SUMS](https://github.com/Dicklesworthstone/pi_agent_rust/releases/download/v0.5.1/SHA256SUMS),
[AMD64 build manifest](https://github.com/Dicklesworthstone/pi_agent_rust/releases/download/v0.5.1/build-manifest-pi-linux-amd64.json),
and [ARM64 build manifest](https://github.com/Dicklesworthstone/pi_agent_rust/releases/download/v0.5.1/build-manifest-pi-linux-arm64.json)
were fetched over HTTPS. Both manifests identify that exact source revision.
Both archives were independently downloaded and hashed; each exact `pi` member
was streamed to SHA-256 without writing or executing an extracted binary.
Observed values matched both upstream metadata and the compiled pins:

| Artifact | SHA-256 |
| --- | --- |
| `pi-linux-amd64.tar.xz` | `13c39f3e9a45c2f6da0b1069521cda5a4ae4e99c29ec91422231b508dcfe3f0c` |
| AMD64 `pi` | `07c13337ab52695d3d8897e37fac7164ce97184b3c4ecc6ce01b4ea0ceef6677` |
| `pi-linux-arm64.tar.xz` | `89906e3fd11842c1a571f174740a188ebdc6c8a7a65f51f1d6f0b23800ad2e44` |
| ARM64 `pi` | `0190009efe002c3cbca80a5f005472cea0df5901706c9e49dcb6c4c6891e0884` |

This is HTTPS/upstream consistency evidence, **not** independent signature
verification, reproducible-build proof, an operator signing trust root, or
activation approval. No upstream detached signature was verified. The separate
operator-authenticated release/staging workflow must bind these pins before
deployment. The crate performs no downloads or automatic updates.

## Before provider integration

The exact pinned `src/cli.rs`, `src/config.rs`, `src/main.rs` and `src/rpc.rs`
were inspected for flags, settings, response shape and startup behavior. Upstream
MCP discovery is initialized separately from the no-tools/no-extensions flags;
fresh directories avoid ambient configuration but do not establish filesystem
or egress isolation. Missing credentials or unavailable exact model can prevent
real Pi startup: the probe reports failure, never substitutes a model or key.

`provider_execution_gate()` always rejects; successful control state does not
change it. Activation needs a separately enforced sandbox and proxy-only egress,
with no CA/key/step-ca/OpenBao paths exposed to Pi. Only the admission proxy may
hold the approved normal Memory credential; R3 credentials are out of scope.
Every physical provider request, including any retry/failover/compaction path,
needs a durable area/work/fencing-bound reservation before dispatch, exact model
and endpoint/body/output limits, conservative cost ceilings, and durable actual
usage settlement or an uncertain reservation retained after ambiguous outcomes.
That transport must prevent Pi from bypassing admission, and separately persist
private proposals before publishing verified outcomes. Those host responsibilities
are not conferred by the probe or the separate child entrypoint below.

## Broker-only child entrypoint

`memory-pi-sandbox` is a separate restricted executable for the approved
`0.5.1+memory.no-tools.1` patch. It is **not a host launcher**. An independently
enforced network-none, read-only-root, non-root container supplies only the
verified patched binary at `/pi/pi`, this driver, a private `/broker/job.sock`,
and a fresh scratch tmpfs. The child checks loopback-only networking, empty
capabilities, no-new-privileges, a read-only root and the socket's owner/modes to
catch accidental host invocation; these checks do not replace outer isolation.

Bounded stdin carries exact system/prompt bytes and a single-work broker token,
never an OpenAI or database credential. No endpoint, path or additional argument
can be supplied. A one-connection loopback-to-Unix relay is the only provider
route. The real provider remains outside the container in the trusted broker.
Pi retry, fallback, compaction, tools, context discovery, sessions and cache
affinity remain disabled; control state is checked before the single prompt.

The driver uses a maximum 110-second exchange, 256-KiB frame/2-MiB total stdout
limits, 8-KiB discarded stderr, bounded relay traffic and explicit kill/reap.
Reports contain only a proposal-text hash, process outcome and typed failure.
The parent must match that hash against the exact durably stored proposal;
child success/usage is never authority, billing evidence or completion proof.
The legacy `provider_execution_gate()` remains closed: a successful probe cannot
authorize the new broker-only path or arbitrary provider traffic.

The 2026-09-18 ignored runtime test
`transport::tests::broker::sandbox::patched_pi_reaches_durable_broker_only_through_private_socket`
ran the authentic patched AMD64 Pi through this driver, a host Unix broker,
fake provider and disposable PostgreSQL. The happy path matched the saved
proposal hash and all 120 reported synthetic tokens, with one count and one
generation. Revocation during counting sent no generation; malformed usage
retained uncertainty with no accepted proposal. All containers were removed.
No paid calls, real knowledge, live database or credential was used.

Reproduce after building `cargo build -p codifix-supervisor-pi --bin memory-pi-sandbox`
using the shared build target, with `CODIFIX_PI_PATCHED_ARTIFACT` and
`CODIFIX_PI_SANDBOX_DRIVER` set to those explicit verified local paths,
`CODIFIX_TEST_REQUIRE_DB=1` and a disposable test database. Run the named
ignored test in `codifix-supervisor-runtime`. These environment-selected test paths
are not a production installer, sandbox-launch API or activation grant.

## Focused checks

Run from the workspace root (Python 3 is needed only for fake executables):

```sh
CARGO_TARGET_DIR=/data/tmp/memory-four-area-target cargo test -p codifix-supervisor-pi --locked --offline
CARGO_TARGET_DIR=/data/tmp/memory-four-area-target cargo clippy -p codifix-supervisor-pi --all-targets --locked --offline -- -D warnings
cargo fmt -p codifix-supervisor-pi --check
```

Tests assert the child's exact environment/arguments/requests, empty context,
redaction, model/state rejection, malformed and uncorrelated output, byte limits,
one shared deadline, child reaping, cancellation cleanup, artifact/architecture
rejection, and symlink/shared-ancestor rejection. They make no provider calls and
read no real credentials. These are local protocol tests, not live integration
or host isolation certification.

An explicit ignored `sandboxed_real_credential_free_probe` diagnostic can be run
only after an operator independently supplies a network-disabled, read-only-root
container with the pinned binary at `/scratch/pi`, private writable `/scratch`,
and `CODIFIX_PI_SANDBOXED_PROBE=network-none-read-only-root`. The marker is an
intent acknowledgement, not sandbox attestation. On 2026-09-17:

- The existing `rust:1.98.1-bookworm` image could not load the host-built driver,
  which requires `GLIBC_2.39`.
- Official Ubuntu 24.04 ran the driver, but Pi closed without an RPC frame.
  Read-only ELF inspection establishes that the pinned AMD64 Pi itself requires
  `GLIBC_2.43`; older distro compatibility must not be assumed.
- Official Ubuntu 26.04, pinned to
  `sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367`,
  launched the verified AMD64 binary with all fixed flags. Pi returned the
  structured startup class `missing_credential` (159-byte frame); the driver
  explicitly killed/reaped it. Both readiness and provider execution remained
  false. The ignored diagnostic now asserts this blocked outcome.

The successful diagnostic used no credentials, no prompt, `--network=none`,
`--read-only`, `--cap-drop=ALL`, `--security-opt=no-new-privileges`, UID/GID 1000,
a 32-process/512-MiB/one-CPU limit, read-only mounts of the pinned Pi binary and
test driver, and a mode-0700, 160-MiB executable scratch tmpfs. No host config,
workspace, key path, Docker socket, or service state was mounted. Containers
were removed after each run. The raw upstream AMD64 binary was independently
downloaded and hashed again before this diagnostic; it matched the table above.

To reproduce, first build the test driver using the focused commands above,
then replace the two explicit artifact paths below with the verified binary and
the exact test executable reported by `cargo test --no-run`. Do not add credential
environment variables, secret/config mounts, a host network, or host execution:

```sh
docker run --rm --pull=never --network=none --read-only \
  --cap-drop=ALL --security-opt=no-new-privileges --user=1000:1000 \
  --pids-limit=32 --memory=512m --cpus=1 \
  --tmpfs=/scratch:rw,exec,nosuid,nodev,size=160m,mode=0700,uid=1000,gid=1000 \
  --mount=type=bind,src=/absolute/path/to/pinned-pi,dst=/scratch/pi,readonly \
  --mount=type=bind,src=/absolute/path/to/probe-test-driver,dst=/probe-tests,readonly \
  --env=CODIFIX_PI_SANDBOXED_PROBE=network-none-read-only-root \
  ubuntu@sha256:cd21a4f68a617580279d4b091cb18e3af9fa8a87500665f0ae5f7f757d17d367 \
  /probe-tests --exact tests::sandboxed_real_credential_free_probe --ignored --nocapture
```

This proves actual startup framing, fixed CLI acceptance through the auth gate,
credential-free failure and reaping on AMD64. The no-token run cannot reach
`get_state`, because Pi requires local authentication presence at startup.

A second ignored test, `sandboxed_real_synthetic_auth_control_probe`, uses a
private `cfg(test)` option (default false) to set a **hardcoded invalid non-secret
token** for that local presence check. The option and token injection are absent
from production builds, and no arbitrary token input is accepted. It must run
with exactly the same independently enforced no-network/no-secret container
boundary above; replace only the final test name. On 2026-09-17 this test passed
against the authentic pinned AMD64 artifact:

- `set_auto_retry(false)` received its correlated 79-byte acknowledgement.
- `set_auto_compaction(false)` received its correlated 84-byte acknowledgement.
- `get_state` received a correlated 708-byte response confirming the exact local
  selection `openai/gpt-5.6-sol`, thinking off, disabled retry/compaction, no
  streaming/compacting, zero messages/pending messages, and null session file/name.
- Clean protocol EOF and successful process exit were reaped. Stderr was empty.
  `protocol_ready=true`; `provider_execution_enabled=false`.

This proves real **local control compatibility**, not authenticated provider
model availability, generation, accounting, or successful model output. No
prompt, tool call, paid request, normal Memory key or R3 key was supplied. No
ARM64 binary was executed. GLIBC compatibility still needs explicit checking on
each deployment target; neither an older CA distro nor a production sandbox is
certified. Production remains credential-free and therefore blocked at Pi's
auth-presence gate until a separately reviewed proxy-only integration exists.

## Real fake-provider diagnostic: tool isolation blocked

On 2026-09-17 the additional ignored test
`provider_probe::sandboxed_real_fake_provider_exposes_unrequested_tool` ran in
the same pinned Ubuntu 26.04/network-none/non-root/read-only container. Its
in-container fake HTTP server accepted only the fixed invalid fixture token,
exact model, bounded synthetic input, no provider storage, streaming and an
800-output-token limit. No host service, real credential, dataset or paid model
was used. Reproduce with the earlier Docker command and this exact test name.

The test **reproduces a blocked tool-isolation gate**, not a qualified agent:

- `--no-tools` still advertises `xdev` and `submit_plan`.
- Explicit `tools.loadMode` settings turning `manage_skill`, `xdev` and
  `submit_plan` off instead advertise `manage_skill` and `submit_plan`.
- Both successful synthetic responses complete a full RPC prompt/result cycle.
  A synthetic HTTP 429 yields `provider_error` and terminal failure with exactly
  one request, no retry and no successful answer.
- The supplied system prompt has an upstream current-date line appended. The
  diagnostic accepts only that bounded line, not paths or other added context.

The [pinned tool registry](https://github.com/Dicklesworthstone/pi_agent_rust/blob/7c018475d54d4e09c907691e47a520f1f09e3288/src/tools.rs#L5262)
adds `manage_skill` independently of the selected built-ins, while the
[pinned startup](https://github.com/Dicklesworthstone/pi_agent_rust/blob/7c018475d54d4e09c907691e47a520f1f09e3288/src/main.rs#L2020)
adds `submit_plan`. The dispatcher derives from discoverable registrations;
setting a tier to off does not remove these unconditional additions. The
[prompt builder](https://github.com/Dicklesworthstone/pi_agent_rust/blob/7c018475d54d4e09c907691e47a520f1f09e3288/src/app.rs#L221)
adds the date even with an explicit system prompt.

The diagnostic intentionally supplies only a fixed text response, never a tool
call. It proves these declarations exist, **not** that a particular privileged
effect was executed. Production `provider_execution_gate()` remains closed.
Olivier approved a narrowly patched, separately hashed Pi build on 2026-09-17.
The approval was checkpointed before fetching the complete pinned source.
Inspection then found its
[nonstandard licence rider](https://github.com/Dicklesworthstone/pi_agent_rust/blob/7c018475d54d4e09c907691e47a520f1f09e3288/LICENSE),
which expressly restricts OpenAI/Anthropic-related use and includes modification,
testing and distribution. Do not describe this as plain MIT licensing. The
applicability to this Codex-assisted work has not been legally established.
Olivier subsequently confirmed continuation of the internal narrow patch;
that authorization was checkpointed at 20:06 UTC. The complete upstream notice
is retained, with no claim of standard MIT terms or maintainer clearance. This
is scoped to local work, not publication or redistribution. Original
artifacts remain pinned without silent substitution, and patched diagnostics
must record their own exact source, patch and executable hashes.
Even a corrected tool flag will not replace the external sandbox, exact input
consent, per-request admission, response validation or budget checks.

## Separately pinned internal no-tools.1 experiment

The approved two-site RPC-startup patch now builds on AMD64 and passes the
network-none synthetic diagnostic: no tools are advertised with either default
or explicit-off settings; success returns the exact supplied text; HTTP 429
fails without an automatic second request. The original artifact still
reproduces the defect. Its production hash is unchanged and rejects the patched
binary, whose fixed digest exists only inside the diagnostic test module.

See [patch, complete notice, identity and evidence](patches/README.md). The
13 ordinary tests pass; five environment-bound diagnostics remain ignored by
default, including the patched test that was explicitly exercised separately.
This is not a live agent, general FTUI/SDK certification, an ARM64/CA runtime
result, an admission proxy or a production provider grant.

## Exact-input supervisor admission diagnostic

The ignored `provider_probe::sandboxed_patched_pi_matches_supervisor_admission`
test connects the actual separately hashed AMD64 binary's first request to
`codifix-supervisor-runtime::pi_admission::PiAdmission`, a test-only dependency here.
It supplies the exact structured question/evidence prompt, fixed supervisor
instructions and `PI_CACHE_RETENTION=none`. The validator rejects all unknown
fields, context additions and profile changes, including session-affinity keys.
Only the canonical date suffix added by pinned Pi is tolerated; the runtime
discards that date rather than sending it to a provider.

On 2026-09-17 both synthetic success and HTTP 429 passed in the same pinned
network-none/read-only/non-root Ubuntu container described above, with one local
HTTP request per case and no real credentials. The success returned the exact
structured fixture proposal through RPC; the 429 failed without retry. Reproduce
by building the test driver and selecting the new exact test name in the Docker
command above. No binary or production hash changed in this test.

The runtime's separate required-PostgreSQL tests feed accepted Pi-shaped requests
through real reserve/begin/settle/private-proposal persistence with fake providers.
They prove one generation under concurrent replay, exact frozen provider payloads,
scoped recovery and revocation/uncertain-usage handling. These are **separate
composable tests**, not one Pi-to-PostgreSQL live integration. This crate still
exposes no production prompt API, provider credential, SQL access or listening
server. HTTP authentication, proxy-only egress, current disclosure consent and
safe result streaming remain integration work. Pi's off-thinking/streaming wire
profile is deliberately translated to Memory's fixed low-reasoning/non-streaming
structured proposal profile, not passed through to OpenAI unchanged. This is a
restricted proposal path, not general autonomous Pi tool execution.
