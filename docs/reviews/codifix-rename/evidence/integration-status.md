# Integration checkpoint — 17 October 2026

Historical checkpoint. Current publication, runtime and client results are in
the [live cutover ledger](../live-cutover.md); the pending statements below
describe this earlier observation, not new work requests.

Candidate source is 3.0.0-rc.1. This is implementation/rehearsal evidence, not a
published release or a completed fleet cutover. See the separate federation
snapshot evidence and [CA console checks](../ca-console-checks.md).

## Verified source gates

- 32 workspace package versions plus Web/docs/skills/plugin candidate versions
  agree. Cargo third-party packages and npm dependency entries are unchanged.
- MCP: 16 tests passed, including the `codifix_memory_history` discovery name.
- Current schema 85 and historical import 65: 15 tests passed against explicitly
  disposable PostgreSQL. Existing migrations remain byte-for-byte unchanged.
- Web: 293 tests passed and production build passed.
- CA read-only preflight: 13 stubbed safety/metadata tests passed. It explicitly
  inventories both legacy Memory and candidate Codifix units and versions.
- Installer: three mocked execution tests prove legacy/latest or wrong-architecture
  assets cause refusal before download/install; matching Codifix selection works.
- Windows source tests: 71 run, three native skips, no failures.
- R3 source tests for renamed environment flags: 149 total, 76 passed, 73 native
  or explicit optional skips. No paid evaluation was run.
- Historical staging tests now bind the preserved schema-61 prefix rather than
  incorrectly assuming that no later migrations exist: 15 passed, one optional skip.
- macOS onboarding database defaults: operator 29 and first-run 13 tests passed.
- The namespace change changed CAR block hash order. The resource-bomb test now
  selects the smallest valid frame to isolate the block-count bound; all 11
  verifier tests passed. No security limit was relaxed.

Consolidated all-target Rust testing used the release workflow's four-thread
limit and an explicitly required disposable database. It completed 102 targets:
1,516 tests passed, 14 were explicitly ignored, and one MCP authorization fixture
still used the old tool name. That fixture was corrected. The final affected
CLI/service/MCP/platform all-target rerun passed 665 tests across 9 targets,
with 3 explicit ignores and no failures. The earlier unconstrained run
exhausted the disposable PostgreSQL connection limit; no production database was
involved. No remaining observed local Rust test failure is open; hosted native gates are tracked separately below.

Docs content (99 source files), local links, 16 image references, production docs
build, architecture boundaries and five release asset-contract tests passed.
The complete area-script suite passed 116 tests with six optional skips.
All 84 previously applied migration files match the original baseline bytes.
The candidate contract confirms all 32 workspace packages and root versions.

The Mac administrator transition has additional isolated evidence: 22 baseline
and 11 guarded-transaction checks passed. A temporary NOLOGIN role exists only
inside the transaction and is dropped before commit; no permanent login is added.
Astra reviewed the supported session-authorization design. Actual cold cutover
and new runtime bindings remain pending. Native Windows and
Mac Intel/ARM installer/signing acceptance, real-client credentials, fresh
federation enrollment, final coordinated backup/restore and live cutover remain
open. Previously recorded Mac app unit coverage is 55 tests with four explicit
live-service skips; its signed UI build succeeded but automation could not start,
so no UI test execution is claimed.

## Operational checkpoint

Only the old Monolith watcher manager and this repository's Workshop watcher
were stopped: they were recreating legacy bootstrap files in the renamed source.
Both were verified inactive. Definitions, before-state metadata and generated
files were preserved privately under the rename recovery directory. Keep them
paused until correctly configured Codifix watchers replace them. Other live
applications/databases and laptop services were not changed by this checkpoint.

All four Monolith database seeds and two Mac database seeds were restored into
isolated PostgreSQL for rehearsal. These are per-database snapshots taken while
writers were active, not a final coordinated recovery set or permission-restoration
claim. Original dumps remain unchanged. Mac credentials were inspected as metadata;
no Keychain transfer was applied. DNS, GitHub repository identity and published
release status remain unchanged.

CA SSH is deliberately disabled and must stay disabled. The console inventory
has been received and its JSON checksum verified. At 21:25 UTC CA HTTPS returned
200/ok and canonical OpenBao HTTPS returned 200, initialized/unsealed/active.
The earlier TLS failure was on a different hostname. Process/database binding,
recovery coverage and restore validation remain pending. See
[CA console checks](../ca-console-checks.md); any CA change requires Olivier's
approval. Follow-up observations are coordinated through Notion page bodies.


## First hosted candidate attempt

The branch at `95130a0cd029545efcb452bdd040ca0e3b20730a` was pushed and
[run 39065798756](https://github.com/3vilM33pl3/memory/actions/runs/39065798756)
was dispatched for `3.0.0-rc.1` with `publish=false`. Contract and source archive
passed. Windows stopped before database startup because the release guard expected
`codifix-windows-postgres-proof-v1`, while the shared manifest declared the old
format. Commit `5b46abc5` aligns that manifest and both guest consumers, including
the derived proof-result format. PostgreSQL/pgvector versions, URLs and upstream
SHA-256 pins are unchanged. A regression checks the release guard against the
actual shared manifest. Windows source validation now ran 72 tests with three
native skips and no failures. Hosted Windows acceptance remains pending a new
candidate run; the failed run cannot establish package acceptance.

The private Monolith ledger records exact database/role/container/path mappings
and bounded cold preparation commands. Its mappings still need instance-specific
rehearsal and final client/privilege checks. Mac immutable runtime-journal rebinding
passed Astra source review and 12 focused tests; native rebinding remains pending. Neither ledger preparation nor the six populated data
rehearsals establishes the full operational cutover. No live cutover occurred.

## Corrected candidate preparation

The first hosted run also passed required-database/source/security validation,
Linux arm64 packaging and Apple-silicon standard/update package acceptance.
Mac Intel and Linux amd64 were still building at this checkpoint. The verified
Apple-silicon artifacts bind to the original `95130a0c` source, are unsigned and
not notarized, and contain no bundled runtime. They do not establish acceptance
of later source changes.

The [fresh native Mac runtime](native-macos-runtime.md) passed staged validation.
The [immutable runtime helper](macos-runtime-rebind.md) passed its 12 tests and
Astra review. Review then found automatic loop/auth bootstrap writes after
migration verification. Commit `15e1f828` returns after successful verification
when both read-only and verify-only are selected. Four real-database schema
checks passed, including SELECT-only startup with unchanged loop/auth/audit/
migration snapshots, rejection of invalid history, and retained writable
bootstrap. Astra's follow-up review found no further concrete automatic writer
or egress under the helper's cold configuration; native process observation is
still required.

[Current TUI captures](tui-captures.md) use disposable synthetic data. Active
user docs and embedded help were refreshed, while historical source evidence
and exact old objects in cutover instructions remain preserved. The final
candidate run must include these source corrections; no live cutover occurred.

Both Linux native lifecycle jobs subsequently reached successful fresh dpkg
installation, then failed the version assertion. The test used an unescaped
Bash tilde pattern, which expands to HOME and leaves `3.0.0~rc.1` unchanged.
The corrected escaped pattern was exercised directly from the script for
stable, rc.1 and six-digit RC versions; all three pass, and shell syntax passes.
This is a test harness correction; native lifecycle acceptance still requires
rerunning it. Run `39068466765` at `3701fd10` was cancelled while pending with
zero jobs so the replacement candidate can include this correction.

## Current candidate and populated functional reads

Corrected unpublished candidate [run 39068603586](https://github.com/3vilM33pl3/memory/actions/runs/39068603586)
is pinned to `afbde0b8d7bccaef05d6cdacc8e3c5bd9ed80eb3`, version 3.0.0-rc.1,
`publish=false`. It is now running; the first run completed with the recorded
Windows/Linux failures and both Mac architectures' package acceptance passing.
No corrected-candidate acceptance is claimed yet. Later commits add docs,
private-rehearsal evidence and a standalone CA observational helper; they do not
alter that candidate's Rust/package code.

[Workshop HTTP reads](populated-workshop-reads.md) and
[restored Mac cache HTTP reads](populated-mac-cache-reads.md) now pass against
separate populated disposable clones. Successful reads preserve all 161 table
fingerprints and normalized schema, including original provenance/version and
remote-origin citation identities. A separate intentional anonymous cache denial
adds exactly one security audit row while preserving every original row; this
is recorded separately, not deleted or hidden. Vectors/attachment bytes remain
unexercised. These are local service tests, not live Mac installation/enrolment.
