# Four-area internal Memory network

Status: Workshop and a separate two-record Homelab preview exchange explicitly
reviewed knowledge through the private company gateway as of 2026-09-19. The
full four-area network is not deployed; no new signed release was published for
these previews. Current work is the approved [dependability and four-area
staging increment](dependable-staging.md), completed on 2026-09-20 without writer
cutover or CA activation.

Workshop evidence: [Workshop-first live activation](workshop-live.md). The
bounded operator worker completed one real Pi/Sol proposal over three reviewed
records in an independent database. Web/CLI/TUI inspection and local Web querying
work. Existing services and the original dataset remain unchanged. Start with the
[demo guide](../../user/workshop-preview.md); remaining four-area gates stay open.

Current work: [Homelab / Workshop exchange](homelab-exchange.md). Homelab's
independent database, authentication, encrypted backups and local Web/CLI/TUI
paths are verified. Use its [demo guide](../../user/homelab-preview.md). Private
gateway provisioning and reviewed deterministic exchange have evidence in the
linked ledger. The newer [dependability evidence](dependable-staging.md) records
encrypted source/preview recovery, credential renewal and restart checks.
[Four-area source accounting, inactive imports and application-clone
verification](staging-evidence.md) are complete. Original writers have not moved;
Trust and General remain inactive.

Completed off-CA preparation: [CA-compatible Debian 12 ARM64 candidates](ca-bookworm-release.md).
Olivier confirmed the existing CA Memory service uses the shared Monolith
database; its console inventory reports glibc 2.36. Separately identified unsigned
Memory/Pi candidates and disposable compatibility evidence are preserved; no CA
installation, writer cutover or Pi activation has occurred.

The subsequent [inactive CA console first-install slice](ca-console-installation.md)
is implemented and verified, including process-death recovery and a root-only CLI
rehearsal in an isolated container. Its [operator guide](../../../scripts/areas/releases/console-README.md)
does not authorize activation or upgrade the running singleton. These tools are
not retroactively included in the preserved candidate. A coherent signed release,
native CA checks, independent database/network preparation, offline reconciliation
and approved writer cutover remain separate gates.

The [coherent offline release preparation](coherent-release-preparation.md) now
provides newly verified AMD64 and ARM64 packages from one frozen source commit,
including the corrected independent named-instance paths and console tools.
Both runtime closures and isolated smoke checks pass; ARM64 runtime evidence is
explicitly emulated. That original review remains unsigned and unchanged. The
subsequently approved [signed handoff](signed-release-handoff.md) authenticates
both architectures offline, with eight tamper/identity/schema rejection checks.
It is not a deployed or publicly published release. Independent signer trust,
native CA checks and a **CA-local** Trust database remain prerequisites; no remote
database topology or original-writer cutover is implied.

The [dedicated release-signing key](release-signing-setup.md) is now provisioned
with direct operator passphrase entry, a public-only Debian 12 verification test
and a verified encrypted backup, now also copied and checksum-verified on the
operator's USB drive. Safely eject that drive for offline storage. Signing the
exact reviewed release was separately approved and is now verified in a new
local handoff; no key was installed on the CA. Keep recovery media separate from
the deployment handoff.

The [CA transfer USB](ca-transfer-usb.md) is now reformatted as one `MEMORY_CA`
FAT32 volume, with the complete signed public handoff copied and all checksums
verified after read-only remount. It is safely unmounted for physical transfer.
This does not install software, keys or data on the CA.

The [CA-agent handoff](../../user/ca-agent-handoff/README.md) now groups the full
workflow into one runbook, offline verification helper and public support download
on the shared Notion coordination page. The CA agent can reuse its existing USB
release; Olivier need not relay each command. [Handoff evidence](ca-agent-handoff.md)
records the exact package and tests. Native CA acknowledgement/checks, a minimized
Trust-only transfer/import method, fresh source delta and explicitly approved
host preparation/cutover were the next gates. The subsequent
[Trust-only local restore and fresh delta](trust-transfer.md) now has verified
local evidence and a [grouped inactive CA proposal](../../user/ca-agent-handoff/TRUST-INACTIVE-PROPOSAL.md).
The CA now reports the named Trust release installed but disabled/inactive, with
the isolated Trust database stopped/manual and no activation marker. Its native
version, AArch64 ABI, runtime-library resolution, installed hashes and static
unit verification passed. The preflight stopped because the generic unit does
not yet hide every locally inventoried OpenBao-config, backup, installation-input,
preparation/evidence and container-socket path; no activation or unit mutation
occurred. A newer 26 September Monolith snapshot, exact restore and renewed
18-history Trust manifest are verified; nothing was imported into the CA. An
offline four-capability Trust enrollment preview also exists, but signed
gateway-history inspection currently fails closed, no Trust credential or
collection exists, and no record was published. Sandbox hardening, writer
cutover, current Trust data delivery, local activation and federation activation
remain separate operations. This is not a deployed Trust instance.

The [Workshop interaction increment](workshop-interaction.md) now exposes the
saved answer and exact citations in Web/CLI/TUI, with permanent digest-bound
human review. The independent preview is upgraded through migration 0060, with
explicit question preparation/confirmation and a separate bounded queue worker.
Preparing is free; confirming may invoke the configured model under the existing
grant and budget. Review does not execute a proposal or authorize another call.
An explicitly approved immutable replacement grant now expires at
`2026-09-26T22:42:47.500Z`. It retains the exact three reviewed source versions,
`gpt-5.6-sol`, 20 calls and $2.50 per UTC day without resetting prior usage.
New paid questions still require explicit confirmation. This is not automatic
consent renewal or permission to disclose the staged dataset.

## Historical implementation notes

The entries below describe progress at their recorded stage, not the current
Workshop activation status. The linked live evidence supersedes earlier gates.

Packaging follow-up: the Debian builder includes the opt-in gateway and uses a
host-native lean CLI for cross-build completions, never the ARM64 target binary.
Synthetic AMD64/ARM64 packaging tests pass, including immutable output paths,
explicit build-volume output and Debian prerelease ordering (`2.1.0~rc.1` before
`2.1.0`). The three clean-launcher tests also pass. These are build-path tests,
not signing, installation or CA runtime evidence. The corrected real AMD64 and
ARM64 packages subsequently completed at `57dd6a77`; exact hashes, dependencies,
permission checks and bounded extracted-AMD64 checks are recorded in the
[candidate ledger](rc1-candidate.md). ARM64 is statically inspected only.

Pi follow-up: a real AMD64 RPC/fake-provider diagnostic in the pinned,
network-disabled sandbox reproduces an **open tool-isolation defect**.
`--no-tools` advertises `xdev`/`submit_plan`; explicit off tiers still advertise
`manage_skill`/`submit_plan`. Synthetic success and HTTP 429 each make one local
request; failure is explicit without retry. The ordinary 11 Pi tests and strict
Clippy pass, but this diagnostic is not a passed activation gate. A separately
hashed patch was approved and checkpointed at 13:36 UTC (plan memory
`03320fec-9b3f-4fed-afb2-21c5eb4a2c93`). Before modification, the complete pinned
source exposed a nonstandard licence rider restricting OpenAI/Anthropic-related
use. Work paused for clarification; Olivier subsequently confirmed continuation
of the internal patch, checkpointed at 20:06 UTC. Preserve the complete notice
without claiming standard MIT terms or legal clearance. Original pins remain
unchanged; separately identified local patch/testing may proceed, with no Pi
publication, paid calls or live activation. No real key or paid call was used.
See [reproduction and evidence](../../../crates/mem-supervisor-pi/README.md#real-fake-provider-diagnostic-tool-isolation-blocked).
The narrow internal `0.5.1+memory.no-tools.1` patch subsequently built on AMD64
and passed the network-none synthetic diagnostic: both settings profiles
advertise no tools; success/HTTP 429 each make exactly one local request. The
original production pin rejects this separately hashed artifact. Source, patch,
licence, compiler and binary identity are in the
[patch evidence](../../../crates/mem-supervisor-pi/patches/README.md).
Thirteen ordinary tests, strict adapter Clippy, formatting and boundaries pass.
ARM64 also compiled successfully and has its own preserved binary hash and
static ELF evidence; neither that platform nor the CA has a patched runtime
result. Both patched binaries still require glibc 2.43. Provider
activation/admission and disclosure gates remain
closed. Nothing was installed into an existing service.

Supervisor-admission follow-up, 2026-09-17: the dev checkpoint at 20:55 UTC
preceded an exact-input Pi ingress validator and its binding to the existing
durable model transport. Changed question/evidence, unreviewed context, tools,
unknown or duplicate fields, storage and model overrides are rejected before
counter calls or journal mutation. The immutable Memory request is sent upstream,
never Pi's original bytes; its date/session metadata cannot leak into that request.
The fixed profile translation and remaining caller-consent obligations are in the
[runtime contract](../../../crates/mem-supervisor-runtime/README.md#pi-request-admission-not-a-running-proxy).
All **37 runtime tests** pass with required disposable PostgreSQL, including
concurrent replay yielding one generation, durable private recovery, revocation
and uncertain billing. Thirteen ordinary Pi tests pass, with five environment-only
diagnostics explicitly ignored by default. The new real patched-AMD64 admission
diagnostic was run separately in the pinned network-none sandbox: success and
HTTP 429 each made exactly one synthetic request, with no tools or real key.
Focused strict Clippy, formatting and boundaries pass. This validates separately
the Pi wire and database/provider path; it is not yet an authenticated running
proxy, current source/destination consent implementation, live agent, ARM64
runtime or four-area deployment. Provider activation remains closed.
The combined `mem-supervisor`, `mem-supervisor-store`,
`mem-supervisor-runtime` and `mem-supervisor-pi` all-targets regression rerun
passes **101 tests**, with required disposable PostgreSQL and five explicitly
ignored environment-bound diagnostics. Both patched-AMD64 diagnostics were
also exercised separately: the original three-case no-tools regression and the
new two-case exact-input admission check. No live migrations, provider calls,
credential changes, imports or service restarts were performed.

The native rc.1 binaries and Web UI compiled at source `5827aeb`, but packaging
failed because a private umask left the Debian control directory mode 0700.
Commit `7c988fab` normalizes only the public package payload, leaving its staging
parent private. Both architecture fixture builds now test umask 077. No failed
package was installed. Both actual corrected packages now assemble successfully.

Fresh-target preparation now has an explicit `schema-plan` and
`initialize-schema` operator path. A reviewed compiled migration digest and
protected local target descriptor are required; an existing schema or other
connected client is refused. All migrations/history commit together, without
application administration, loop definitions, credentials, jobs or supervisor
binding. The 76 focused area/planner/store/operator tests pass with required
disposable PostgreSQL, including a late migration-55 fault rolling back every
schema/history/extension write and a real CLI-created target accepting exact
version/tombstone/provenance import and reconciliation. Strict operator Clippy,
formatting and boundaries pass. No real target was initialized or imported; CA
operator/environment gates are unchanged.

Native `memory --version` passed inside the network-none, read-only-root,
non-root container and reports `2.1.0-rc.1`; it did not contact any database.
Static ELF/dependency inspection found missing runtime dependencies in the old
Debian manifest. Packaging now requires computed, versioned libc/C++/GCC bounds
for all four executables and rejects missing/unresolved metadata. The observed
native requirements are libc6 >= 2.39 and libstdc++6 >= 14; the cross sysroot
metadata conservatively requires libc6:arm64 >= 2.43. These are not CA runtime
approval. PostgreSQL provisioning is no longer an implicit hard package
dependency. Synthetic native/cross and dependency-failure tests pass; actual
rebuilt artifacts confirm the same runtime bounds and remain unsigned/uninstalled.

Offline federation regression rerun passes 76 Rust tests across `mem-federation`
and `mem-atproto`, including shared Rust/Lean traces, expertise policy, replay,
serialization and MST checks. Pinned Lean 4.33.1 `lake build && lake test` also
passes, with existing proof-linter warnings retained. This is scoped model and
adapter evidence, not PDS/network/cryptography/deployed-authentication proof.
Read-only health checks still show the existing production/development instances
on 2.0.0 / 2.0.0-dev, databases up and no pending offline items; no deployment
restart or configuration change was made.

The Web build reported seven dependency audit findings (three high, four
moderate), all in development/build tooling; production-only audit was clear.
The affected development lock entries and Vitest minimum were updated, without
changing any production dependency version. A clean temporary checkout install
(one transient registry reset, then successful bounded retry) reports zero npm
audit findings, passes all 109 Web tests and builds successfully. Existing Web
assets/services were not replaced by this test. Audit results are point-in-time
dependency evidence, not a whole-product security certification.

The [approved execution contract](contract.md) is the source of scope and gates.
Trust runs on the console-administered CA; Homelab, Workshop and General run on
Monolith, each with independent application/database boundaries. Supervisors use
`gpt-5.6-sol` under a $10/day fleet ceiling, never the R3 key or allowance.

## Execution evidence

- Approved-plan workflow succeeded on 2026-09-17 at 08:24:15 UTC on
  `codex/federation-v1`, starting from `2da2005`.
- Plan task: `76b69037-b150-4df7-9ebd-d36f0a5c4ee4`; raw capture:
  `a764afa5-2532-48c9-8fd1-5cfcef9eb985`. The checkpoint has 52 checklist items.
- R3 is paused, not complete. Frozen evaluation artifacts, original ballots,
  AI diagnoses and remaining-budget evidence are retained. No R3 launcher was
  found running in the process inventory at execution start.
- The initial source inspection found 105 projects, 3,367 memory versions and
  2,244 captures. These are a live baseline, not a migration snapshot checksum.
- Current service identities on Monolith are distinct, but that is not proof
  their data stores are distinct. Resolve actual database bindings before cutover.
- Read-only connection inventory at 08:50 UTC found 13 local connections and two
  from the CA address (`10.22.6.2`) to the source `memory` database. Application
  names were blank, so the precise remote process is not inferred. A final
  Monolith-only writer stop is insufficient until the CA binding is reconciled.
- Both system and user `memory-layer.service` are active. The user watcher
  manager, Homelab, SCTP, SCTP-conformance, Soundmind, tb-olivier-rnd and a transient
  dish2-tools watcher were active. None were stopped by this implementation.
  The global DuckDB offline store has a live owner and WAL; a project-local dev
  DuckDB also exists. Neither is claimed as a consistent captured offline store.
  The final cutover must fence these writers and capture/reconcile pending work.
- Fresh online baseline encrypted directly to the operator's existing GPG key,
  with selected operator-readable configuration files separately encrypted.
  Protected local directory: `/data/tmp/memory-four-area-baseline-20260917T0831`;
  independently retained NAS copy: `/media/bunnychow/backups/memory-four-area-20260917`.
  The complete database ciphertext SHA-256 is
  `4600e7062ca5dc3a5e4f4850fabb4b9d73b79cb56dc1b18e30a86ca04f832b1c`.
- Isolated database restore passed: **104 tables and 2,331,495 rows** match the
  archive's per-table row-multiset hashes, with zero invalid indexes or
  unvalidated constraints. The sandbox has no network or published ports and
  runs no Memory application, workers, federation or providers. Evidence lives
  in `restore-evidence.json` beside both backup copies. Historical evaluation
  namespace `mab_full_20260516_150014` is included, not silently omitted.
- Initial row hashes differed because source timestamps rendered in
  `Europe/London` and the sandbox defaulted to UTC. Matching the archive timezone
  produced exact hashes without ignoring or normalizing any fields. New captures
  explicitly pin and record UTC. This is database restore evidence, **not**
  application startup, complete host recovery or final writer-stop evidence.
- Root filesystem free space was approximately 3 GB at execution start. New
  build caches and sensitive recovery artifacts belong on `/data`, not the repo.
- Complete protected partition inventory collected from the verified restore:
  104 tables, 2,331,495 rows, 105 public projects and two schemas. Every table's
  before/stream/after row counts agree in one read-only snapshot. The 1,018,087,569
  byte file has SHA-256
  `0b13c72d8075ae76f5f88920943fddf513605f005255eb0251677020c6622905`;
  `partition-inventory.json.collector.json` records its exact serialization and
  provenance. Both are mode 0600 in the protected baseline directory.
- The first real planner validation took 123.86 seconds and peaked at
  1,811,012 KiB RSS. It reported 100 absent-project references across historical
  `loop_produced_memory` (21), `memory_access_events` (12), `memory_score_audit`
  (32) and `procedural_utility_audit` (35). Read-only database checks confirm the
  references are present in the source history and have no declared foreign keys.
  They must remain visible with their original IDs in the protected archive;
  they must not be reinterpreted as current project ownership or erased.
- After adding strict typed retention notices, the unchanged real inventory
  **passes structural validation with zero blockers and 100 notices**. Active
  ownership and actual FK/logical dependencies still fail closed. Omitting or
  altering the notices in a reviewed manifest is rejected. Normalized inventory
  digest: `ca70bd2ed6eecaeead4d680832ce9b3a63e4a5ae285b1d6e55d644c3d394134f`.
  The private `partition-inventory-validation-20260917T091005Z.json` and metrics
  record 129.12 seconds and 1,810,140 KiB peak RSS in the debug build. This is not
  content classification, approval of an area assignment or import readiness.
- Private unresolved draft generated successfully:
  `partition-unresolved-draft-20260917T091005Z.json` (89,680,231 bytes, mode 0600).
  It contains 104 table policies, 3,107 unresolved canonical lineages, 449,441
  unresolved supporting-record decisions, zero project bindings and the same
  100 retention notices. Debug execution took 134.08 seconds and peaked at
  1,877,068 KiB RSS. **No ownership decision, target import or live cutover is
  implied by generating this draft.** Content/provenance review and safe mixed-
  source transformations remain required before an importer can be authorized.
- Closing health checks returned HTTP 200/database-up for the original Monolith
  instances on ports 4040 (`2.0.0`) and 4250 (`2.0.0-dev`), with their original
  identities. No Memory/CA service was stopped, restarted or reconfigured.

## In progress

- Offline fail-closed schema/partition inventory contracts include the exact
  optional eleven-table historical evaluation catalog, never a schema wildcard.
  Baseline migration 0052 and the opt-in supervisor schema are explicitly
  distinguished; runtime authority stays retired and private outcomes archived.
  The protected operator command is now implemented and tested below; no live
  import yet. Unknown tables and unresolved ownership still block planning.
- Debian install hooks no longer rotate credentials or restart unrelated
  services; opt-in named-instance units have separate protected configuration
  and pinned verified executables. Lifecycle regression tests pass. Actual-host
  sandbox, signature verification, installation and activation gates remain open.
- Pure supervisor action/policy/budget contracts plus bounded private proposals:
  23 tests pass. These are not a running Pi agent, persistent scheduler, API, or
  deployed maintenance authority.
- CA console preflight: twelve stub tests pass. A real non-root run on Monolith
  correctly reports database inspection as permission-denied and binding as
  unknown; this is not a CA observation. The CA console report remains required.
  The [console handoff](../../../deploy/areas/CA-CONSOLE-HANDOFF.md) now gives
  exact read-only preflight and authenticated-staging instructions with stop/
  recovery conditions. It reports libc, namespace/cgroup metadata and anonymous
  DB-client counts without reading configs, secrets or source rows. These are
  observations, not proof of database binding or sandbox enforcement.
- Release manifest/signature/staging tool: 17 offline tests pass, including
  disposable signing-subkey fixtures and staging-directory substitution tests.
  An independent reviewer reproduced a cleanup race in the initial code; the
  private-parent and directory/file-identity fix passed both regression tests
  and independent re-review before commit. No real release has been signed,
  published, installed or granted automatic-install authority. The initial
  four-area deployment is approval-required, not a maintenance update.
- Olivier selected **reuse of the normal Memory OpenAI key** on 2026-09-17.
  Its presence was verified in the existing mode-0600 Memory environment file,
  without displaying, copying or sending it. No persistent secret destination
  was created. R3 credentials and allowances remain excluded. This resolves the
  credential-choice gate, not collection egress, sandbox or activation gates.
- `mem-supervisor-store` persists opt-in policies, stable actions, leases,
  immutable provider attempts/settlements and a per-area commit-ordered journal.
  The nine required-database foundation tests pass: competing claims, restart,
  expired fences, bounded budgets, revocation and journal ordering. Migration
  0053 was tested only in a disposable database, not applied to either service.
- Migration 0054 adds private typed proposal persistence. Eight additional
  database tests prove body/outcome/journal atomicity, stale-worker rollback,
  immutable project/request bindings, sanitized failures and restart reads.
  Generic completion cannot mark model work Verified without its persisted
  private result. These are trusted in-process APIs: pre-send project/source/
  destination authorization still needs service integration. Neither migration
  was applied to a running Memory instance.
- Migration 0055 adds immutable local instance/area binding and pre-dispatch
  project/request/evidence context. Seven additional required-database tests
  prove exact binding, duplicate input, current project existence, revocation,
  renewal fencing, corruption and journal-failure rollback. All **24 store
  tests** pass. These checks prevent relabeling model input; they neither grant
  source disclosure permission nor detect another independent/cloned ledger.
  Migration 0055 has only been exercised in disposable databases.
- `mem-supervisor-runtime` now admits one-shot provider calls through that store.
  It fixes Sol, counts the exact bounded input, reserves worst-case cost, commits
  an attempt before sending and retains uncertain charges without automatic
  retries. Its private-persistence wrapper commits a validated proposal with the
  completed action. Health checks and privileged installation cannot invoke this
  model path. Private suggestions are not approvals. Twenty-seven tests use
  synthetic inputs/local HTTP providers; no actual model request has been made.
  Exact instance/project/input context is now committed before token counting;
  wrong instance or project and changed bound input cannot reach that endpoint.
  Revocation during counting prevents generation. The fleet allocation still
  requires one authoritative budget ledger per area and deployment/recovery
  fencing of duplicate or stale writers.
- The pinned AMD64/ARM64 Pi archives and exact executable hashes were checked.
  Eleven process/protocol tests pass. Two separate real AMD64 diagnostics passed
  in a pinned Ubuntu 26.04 container with no network, read-only root, no host
  configuration/secrets, dropped capabilities and a non-root UID. The no-token
  diagnostic correctly stopped at missing credentials. With a hardcoded invalid
  **test-only** token, the real fixed RPC handshake confirmed selected
  `openai/gpt-5.6-sol`, disabled retries/compaction and empty session state, then
  reaped the process. Provider execution stayed disabled. This is local control
  compatibility, not authenticated model availability or agent activation.
  [Reproduction and limitations](../../../crates/mem-supervisor-pi/README.md).
- The upstream AMD64 and ARM64 Pi artifacts require **glibc 2.43**. Bookworm/Ubuntu 24.04
  could not run AMD64; Ubuntu 26.04 did. The checksum-matched ARM64 binary's
  AArch64 ELF and glibc requirement were inspected statically, not executed.
  Actual CA architecture/runtime compatibility remains a console gate.
- Content/provenance classifier v2 prepared a mode-0600 review pack from the
  isolated restore: 3,107 histories / 3,368 versions, 2,830 area candidates and
  277 protected-archive candidates. Full-pack digest:
  `4304ee374af6f8e07ac8b0f00ed29a784ba74bd037434cc24161c078a3b8b2a7`.
  Candidate areas, including archive holds: Workshop 1,600, General 1,065,
  Homelab 441, Trust 1. These are
  heuristic suggestions, not approved ownership or import instructions. Test/
  evaluation project segments and mixed provenance remain explicit holds.
- A separate Codex engineering content review read the 11 Trust-topic histories
  (13 versions / 39 source rows), verifying exact IDs/hashes against v2. It
  proposes one Trust, eight Homelab, one Workshop and one unresolved mixed-boundary
  hold. The CA-host knowledge and NAS certificate-consumer knowledge are distinct;
  their shared raw context must not be copied across boundaries. The protected
  `trust-content-review-20260917.json` has SHA-256
  `c740b4707753a2f6135ba1ea502ff740c41b35e9cd82e7b471364953a9906935`.
  It is not labelled as Olivier's approval or current factual verification.
- The offline planner's opt-in manifest-v2 extension now supports exact reviewed
  provenance references/excerpts for sources whose task/session remains archived.
  Original IDs/hashes and every row remain accounted; old source checks stay
  archived, other cross-area edges stay blocked and v1 bytes are unchanged.
  Thirty-five planner tests pass, including actual CLI and exact 0054 schema
  gates. Review attestations are external assertions, not authenticated human
  identity. No real decision was auto-approved; no target projection schema or
  database importer is implemented by this slice.
- The subsequent `mem-area` materializer consumes a complete bounded NDJSON
  payload stream with exact inventory/hash/order/count checks. It retains every
  original row in a provisional protected archive and returns opaque area
  partitions only on success. Original JSON numeric bytes are not round-tripped
  through floating point. Materializable row IDs, declared/logical references,
  versions and inherited task/capture project ownership must match the reviewed
  inventory. Mixed sources yield only explicitly reviewed note/reference fields;
  original task/path/context and prior source checks stay archived.
- `mem-area-store` and additive migration 0056 implement a **trusted in-process
  fresh-target importer**, not a deployment CLI. Rows, immutable import/provenance
  receipts, canonical state, source-check sequence and the disabled-automation
  state commit together. Nine required-database synthetic tests pass, including
  exact history/tombstones with reverse UUID insertion order, safe reviewed
  projections, schema/identity/row corruption rejection, retry on a still-inactive
  target, and insertion/verification/deferred-commit rollback. Independent review
  found and closed missing replay-inactivity and payload/receipt-ID checks;
  payload FK/logical/ownership checks were strengthened alongside them.
  See [import boundaries](../../../crates/mem-area-store/README.md).
  This original slice did not cover populated relationships or an operator CLI;
  the later evidence below closes those software gaps. No real-data partition
  has been approved, materialized or imported. Operator review, archive durability,
  actual release artifacts and writer fencing remain open.
- R1/federation regression run: 13 `mem-store` and 26 `mem-federation-store`
  database tests pass. A test-only race was fixed by waiting for PostgreSQL
  backend termination before asserting stale-owner fencing; production fencing
  was unchanged. The affected two-test binary passed 25 repeated runs.
- Combined focused verification passed **149 Rust tests** across `mem-area`,
  the four supervisor crates, `mem-store` and `mem-federation-store`, with
  required disposable PostgreSQL coverage. The two real sandboxed Pi diagnostics
  are separate explicit runs (normally ignored). Backup/restore/inventory/
  classification tooling passed 36 Node tests. Workspace formatting, focused
  strict Clippy and boundary checks passed. This is not a full workspace,
  browser, PDS, Lean or release-gate run.
- The current follow-up passes **181 focused Rust tests**: 48 area planning/
  materialization, nine target-import, 23 pure supervisor, 24 supervisor-store,
  27 supervisor-runtime, 11 Pi-control, 13 local-store and 26 federation-store.
  Database tests required the dedicated disposable PostgreSQL server; two
  environment-dependent real Pi sandbox diagnostics remain explicitly ignored
  in this run (the earlier AMD64 evidence is separate). Strict focused Clippy,
  workspace formatting, boundary checks, 36 Node data-tool tests, 17 release-tool
  tests and 12 CA-preflight tests pass. Twenty-two failed-run synthetic fixture
  databases were removed from that verified test server; no real-data database
  or protected restore was changed. Both existing services still report their
  original identities and versions with database-up health. No supervisor model
  calls, real-data imports, CA changes, release publication or activation occurred.

- Exact private payload export (`0436b17`) now reads the pinned isolated restore
  in a read-only transaction, preserving every original JSON byte inside bounded
  NDJSON envelopes. The protected `partition-payload.ndjson` is 1,695,135,398
  bytes, mode 0600, SHA-256
  `1e9ba6e6740599dbe2d00e6b16b32bc6d4463c312045f43d9ce4d90105ba837f`.
  Export alone was not reported as inventory verification.
- The new read-only `memory-area-import verify-payload` (`d1dc9ab`) independently
  **verified all 104 tables and 2,331,495 rows against the unchanged inventory**,
  including exact file hashes, schema columns, owned row identities, references,
  ordering, counts and complete-stream termination. It retained all 100 notices.
  The real run took 351.23 seconds and peaked at 1,892,852 KiB RSS. It required no
  ownership manifest, fabricated approval, archive rewrite or database connection.
- The protected `memory-area-import` operator (`3983f01`) implements inspection,
  preparation and explicit local-only import. Input descriptors and artifacts
  are hash-bound through pinned private file handles; no configuration/credential
  discovery or automatic migrations/startup occurs. Five tests use synthetic
  databases/private files, the actual binary, interruption/tampering, safe
  provenance projections, inactive-target reconciliation and sanitised failures.
- Manifest-v2 reviewed relationships (`639df89`) retain both endpoint projects
  without inventing a single project owner or changing inventory bytes. Each
  edge requires its own review and both endpoints in one destination area;
  cross-area/archive edges remain blocked. A populated PostgreSQL migration
  test preserves exact edges/attribution and rejects changed replay. Together
  the latest planner/materializer/operator/import tests pass **70 tests** with
  required disposable PostgreSQL; strict focused Clippy and boundaries pass.
- Supervisor inspection (`edb0d1d`) now works through the authenticated service,
  Web, CLI and TUI. A separate durable configured instance UUID is checked
  against the database binding, never confused with the service boot UUID.
  Instance-wide metadata requires global `memory_read`; a project role is
  insufficient. Reads create no jobs, policies, claims or provider calls.
  Policy, budgets, counts and ordered journal IDs are shown without private
  contents. The UI explicitly reports **execution not integrated**, including
  when a stored policy is enabled. Refresh clears stale/denied results.
- The disposable real browser/CLI/PTY rehearsal (`4304047`) passed against the
  same synthetic read-only service and independent test database: durable
  identity matched, paused policy/history rendered, terminal exited cleanly,
  and no work was created. The browser screenshot at
  `/data/tmp/memory-supervisor-inspection-E9ct4l/supervisor.png` was visually
  inspected. No screenshot contains real knowledge or credentials. All **99 Web
  tests**, Web build, OpenAPI/router parity, global-access HTTP test, supervisor
  store inspection and lean CLI build pass. The React review kept requests
  abortable with explicit signed-out/forbidden/offline/identity states.
- Wider verification passes **241 lean CLI/TUI tests** and **22 required-database
  service repository tests**. One pre-existing documentation portability check
  was first run and failed on machine-local paths in the concurrent M2 financial
  review; it was excluded from the subsequent focused run, not reported passed.
  The unrelated M2/M4 work was preserved. The no-default-feature service build
  still emits existing offline-stub warnings; no workspace-wide Clippy or release
  gate pass is claimed. TUI help/navigation expectations and the old Debian
  restart-all test were corrected to the approved opt-in behavior.
- The Debian builder now packages the two locked, architecture-specific migration
  operators, read-only CA preflight, release-staging tool and operator guides.
  Stubbed lifecycle/staging tests prove they are included but never executed
  by a package hook; no package was built for release, signed, installed or
  activated by this test. No Pi binary, secret or private dataset is bundled.
- Opt-in deterministic observation is now implemented: migration 0057 stores a
  five-minute database schedule and immutable operational counts. A writable,
  non-evaluation primary service polls every ten seconds, requires the exact
  instance/area binding and enabled policy, and never bootstraps authority.
  Action, schedule, observation, result and history commit together. Tests prove
  concurrent-check serialization, durable restart, policy/revocation denial,
  deferred-commit rollback and recovery from an absent service pool. No model,
  content, remote request, knowledge mutation or installation is involved.
- Inspection surfaces show the historical observation and its policy/time, not
  a claim of current availability or full agent execution. The Web suite now
  passes **100 tests** and its build; five focused CLI/TUI tests pass. The store
  suite passes **28 tests**, with focused strict Clippy and boundaries passing.
  The importer supports exact schema heads 0056 and 0057; a new required-DB test
  proves an old receipt survives additive upgrade unchanged but cannot authorize
  replay at a different head. No live schema or configuration was changed.
- Explicit pause/resume now runs through `/v1/supervisor/policy`, Web confirmation,
  CLI preview/`--confirm`, and TUI `p`/`e` then Enter. Current global system_admin
  and credential validity are locked through commit, separately from project
  permissions. Instance/revision/expected-state checks prevent stale changes;
  same-actor exact request replay returns a receipt without undoing a later pause.
  Policy, metadata journal and protected actor audit are atomic. Required-DB
  tests cover competing identical requests, changed retries, CSRF/Origin,
  read-only mode, revoked credentials and deferred audit failure rollback.
  Controls grant no new disclosure consent and do not activate Pi.
- A second **real browser/CLI/PTY control rehearsal** completed six confirmed
  synthetic policy changes, reconciled CLI retries, and finished paused with six
  audit records and zero provider attempts. It caught and fixed a missing Web
  JSON content type; PTY verification now waits for a bounded rendered state
  rather than assuming a fixed render delay. Web passes **109 tests** and build;
  six focused CLI/TUI tests, OpenAPI parity, strict store/lean-CLI Clippy and
  boundaries pass. Service Clippy reports only the existing offline-stub warnings.
- Named-instance staging now uses a finite clean environment, not a partial
  ambient-variable denylist. Synthetic launcher tests prove host API keys,
  arbitrary Memory configuration overrides, proxies, SSH agent and shell startup
  files do not reach the child. Federation requires a deliberate launcher flag
  and the exact instance's systemd credential; an ambient config cannot enable
  it. The staging TOML also explicitly disables offline replay, reinforcement and
  provenance schedulers. Package lifecycle/staging tests pass without installing
  a package or changing services. These are launcher tests, not real host sandbox
  enforcement or CA activation evidence.

## 14:00 Europe/London checkpoint, 2026-09-17

The verified software slices do **not** constitute the real four-area deployment.
CA console inventory/database binding/runtime compatibility, approved complete
ownership decisions, final host/offline backups/writer fencing, signed releases,
actual enrollment, and sandboxed Pi/admission integration remain open. A verified
16:00 four-host demo is not promised. Existing live services stay unchanged while
implementation continues; no simulation is labelled as a deployed Trust host.

Next: review and classify the protected source by content/provenance; obtain the CA
console inventory and complete the writer-stop/host/offline backup gates. Model
integration still needs current collection/destination consent and Pi proxy-only
egress; credential selection is resolved. Full agent scheduling and supervision,
Web/TUI agent actions/chat, real releases, enrollment and the live four-area demo
remain unimplemented or unverified. The inspection-only browser/terminal rehearsal
does not satisfy those gates or prove actual agent execution. No full-workspace,
PDS/Lean or release-gate pass is claimed from the focused tests above.

Every release and live activation requires evidence in this ledger. Do not mark
the contract complete based on scaffolding or synthetic tests alone. CA console
steps, operator privileges, restore verification and safe model credential binding
remain explicit operational gates.
