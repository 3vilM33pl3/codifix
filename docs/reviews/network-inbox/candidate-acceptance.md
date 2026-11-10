# Network/Inbox candidate acceptance — 10 November 2026

Candidate versions: **core 3.1.0-rc.1**, **desktop 0.2.0-rc.1**. This report
supplements the [original implementation acceptance](acceptance.md); it does
not replace that historical evidence. Execution is tracked in
the private project tracker and the
[approved checklist](candidate-plan.md).

Status: source and focused integration checks passed; final installer lifecycle
and artifact assembly are in progress. Do not treat this interim report as
completed package acceptance.

## Scope and isolation

All sessions, model responses, grants and Linear records used for the complete
workflow are synthetic. PostgreSQL 17 with pgvector runs in the task-owned
`codifix-inbox-candidate-db-20261110` container on loopback; the test creates an
isolated database, applies the full compatible migration chain through `0097`,
and closes it afterwards. No live account, production service/database, CA,
production grant, publication or installed desktop was changed.

The native fixture drives the same `Transport::api_request` implementation as
the Tauri command, including its route allowlist. The fixture's session input
exists only under `cfg(test)`. This proves native command/HTTP integration; it
does not prove a real keyring, browser login, identity provider or graphical
desktop session.

## Verified source checks

| Check | Observed result |
| --- | --- |
| Complete native command → human service → Inbox ticket → worker → model proposal → prepare/apply → Linear readback | 1 passed, no ignored test; `INBOX_CANDIDATE_VERIFIED` at merged `cd80dc7d` |
| Six focused agency/runtime/projects/OpenBao/Inbox/identity crates | 137 passed; 2 explicit opt-in probes ignored |
| Service Network/Inbox routes and broker boundary | 3 passed |
| Existing desktop-session auth, route policy and OpenAPI inventory | 11 passed |
| Identity broker fixtures and exact known-advisory audit policy | 25 passed; audit reports 16 affected dependency entries |
| Desktop renderer workflows | 39 passed |
| Native desktop tests, serial execution | 18 passed; 3 explicit probes ignored in ordinary run |
| Isolated Linux TLS probe | Trusted CA accepted; untrusted CA and wrong hostname rejected |
| Strict Clippy: service all targets and desktop all targets | Passed with warnings denied |
| Workspace and native formatting | Passed |
| Release asset contracts | 13 passed, including full 3.1.0-rc.1 allocation |
| Core/desktop/plugin/skill metadata and Debian ordering | Passed; previous stable < candidate < next stable |
| Package staging/lifecycle contract, Bookworm build/release/hosted guard contracts | Passed |
| Release upgrade/signature tests | 18 passed |
| Real Chromium renderer fixture | Navigation, send/readback, reviewed invitation, pending cancellation and directory passed |
| Extracted desktop candidate startup, isolated Xvfb/D-Bus/XDG paths | Stayed alive for the ten-second timeout; portal warnings remain |

The complete workflow checks that preparation performs **zero Linear writes**.
Wrong plan UUID, stale digest, removed project access and revoked invocation
grants deny admission. A lost mutation acknowledgement produces an uncertain,
blocked effect; automatic worker polling does not repeat it. An explicit
authorized follow-up reconciles the original effect through readback, with one
mutation per object and visible task results. Existing two-instance, ownership,
disclosure, eight-turn, spending and cancellation tests remain green.

The focused Rust run deliberately skips the real local OpenBao executable probe
and the operator-browser probe. The ordinary native run skips host-keyring,
isolated TLS and the subprocess bridge probe: TLS and the bridge were exercised
separately above. An initial parallel native run reproduced the already
documented ephemeral-port reuse race in the cancellation fixture; the required
serial run passed, and CI now uses serial native tests. No authentication
behavior was weakened to accommodate this fixture.

## Broker finding and artifact boundary

JSON complexity checks now cover valid JSON irrespective of MIME type and use
Fetch-compatible UTF-8/BOM decoding before constructing a `Response`; clones
cannot expose unchecked bytes. Plain-text handle resolution remains supported.
Astra found the BOM edge case during review and verified its correction.

The optional archive contains immutable tracked source, locked production
dependencies, distributed licenses/notices, audit evidence, dependency source
and integrity provenance, checksums, launcher and inactive examples. Source is
materialized from Git rather than recursively packaging untracked worktree
files. Archive verification passed in a directory containing spaces and `#`,
with a different working directory. Dependency loading succeeds on Node 24.15;
Node below 22 and startup without explicit isolated-test opt-in are refused.

SDK **0.5.13** remains pinned. **GHSA-mrhx-c7xc-m3w5 remains unresolved** through
Zod 3.25.76 (16 high dependency entries). The audit gate reports this exception
and rejects other findings, changed pins or invalid audit responses. JSON
hardening is not a fix for the advisory. The broker remains disabled and
non-production; no real AT login was performed.

## Distribution and CI evidence

Candidate artifacts and retained logs will be assembled under
`target/network-inbox-candidate/`, with `candidate-manifest.json` recording each
artifact's exact source revision, versions and SHA-256, and `SHA256SUMS` covering
the delivered files. Core and desktop build environments and source revisions
are recorded separately. The candidate is unsigned and unpublished.

CI now selects database checks for the agency, Inbox, identity and native
changes, explicitly runs the broker fixtures/audit/archive checks, exercises the
full native workflow, and checks package contracts and disposable installation.
The equivalent focused commands were run locally; no hosted GitHub Actions run
or published release is claimed by this report.

## Reproduction and activation limits

Use the [isolated candidate runbook](../../user/network-inbox-candidate.md).
Installation checks use a prepared disposable image by exact image ID and
readonly artifact mounts, with networking disabled. The original hosted-runner
guard is preserved; it is never impersonated on the host.

Production deployment, public publication, real AT login, live Linear writes,
new standing grants and CA recovery remain outside this candidate's acceptance.
Graphical portals, notifications, accessibility, real identity-provider login
and interactive operator acceptance require their own recorded tests. The
default Network/Inbox feature flag remains off.
