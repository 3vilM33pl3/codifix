# Mac onboarding execution ledger

## Approved scope and environment

The [approved contract](README.md) remains the completion checklist. Work started
on 2026-10-05 from `46f5d611`, on `codex/mac-project-onboarding`.

The required `memory-plan-execution` checkpoint succeeded through a new isolated
development instance, not through a production area or the retired shared source:

- HTTP: `127.0.0.1:4250`, reported `2.1.0-rc.4-dev`, database connected.
- PostgreSQL: disposable container `memory-onboarding-dev-pg-20261005`, loopback
  port `55470`, dedicated `memory_onboarding_dev` database and volume.
- Plan thread: `mac-onboarding-20261005`.
- Plan capture: `7907f06d-6439-4997-89f7-182b3d4ec66d`.
- Curated plan: `2ebf61e5-1ca1-4bd1-a404-5e35a41f5ad9`.
- Configuration/plan source: `/home/olivier/.local/state/memory-mac-onboarding-dev/`.

The prior dev service was not running, and its overlay referenced the retired
shared database. It was not reactivated or changed. Existing Workshop, Homelab,
General, Trust and gateway/PDS services remain outside this development rollout.
The isolated service has no provider keys/model, and its configured provider URL
is an unused loopback endpoint. Background work, federation, clustering, watchers
and embedding work are disabled.

## Implementation slices

| Slice | State | Evidence or remaining work |
| --- | --- | --- |
| Contract checkpoint | Complete | Memory IDs above; contract committed. |
| Sol/Astra project defaults | Complete | Two TOML files parse successfully; bounded read-only adviser and updated repo guidance. |
| Local company administration backend | Implemented and focused tests passed | Commits `5140b5d1`, `2fb2a3f2`; 36 auth tests on a separate disposable PostgreSQL database, including six onboarding tests; OpenAPI inventory test, formatting and diff checks. Live provider RBAC and company-wide authority remain unverified. |
| Administration Web UI | Implemented and focused tests passed | Users/Projects/Devices/Company alongside existing memberships/tokens; latest full run: 260 Web tests and production build passed, including separate owning-human agent disclosure review. This is a development UI, not a deployed company console. |
| Unified cross-area administration | Owner delegation, credential provisioner, coordinator and console grant workflow implemented | `16051bb8`, `9f60b457`, `60d27978`, `cc1fe53a`, `af387f94`; explicit owner/token/namespace/role ceiling, ownership guards and durable uncertain outcomes. Separate delivery delegates and company devices are now implemented below; live owner provisioning remains pending. |
| Device login and selection | Backend, CLI/browser approval and terminal project picker implemented | `9f439c27`, `6ac5b994`, `121fab3b`, `f57c25e0`, `4abe06b1`; latest CLI enrollment/workspace/picker/agent suite: 75 tests passed. Native Keychain execution remains unobserved; prerequisite-based journey is now implemented. |
| Selective snapshots/offline search | Gateway, company/local enrolled-device delivery, bootstrap, private metadata producer and explicit CLI projector/runner implemented | `e741af31`, `d8c5f8db`, `8f578566`, `00d7f2fa`, `09df86da`, `d2d8508f`, `15b649d1`, `46fce2d1`, `99cea0c4`; latest required-database federation store/runtime all-target regression: 85 tests. Latest auth regression: 85 tests with bounded concurrency, including company delivery and independent agents. Ordinary retrieval does not merge cached records; native execution remains pending. |
| Company device and composite selection | Implemented; database/HTTP and Web tests passed | `a8f90068`, `02c576b9`, `8118fe1c`, `99cea0c4`, `90c0cabc`, `cdf8580b`; immutable owner approvals, pinned trust, zero ordinary project/global device authority, current-owner refresh and no legacy fallback. Complete identity survives equal project names/UUIDs. Details below; native login remains unobserved. |
| Installer and tool/repository setup | Journaled workspace preparation and prerequisite-based native operator implemented and reviewed | `9dabaca8`, `e305955a`, `178560cc`, `894323cb`, `901f6805`, `56b87363`, `730d48c1`, `f087b735`; 21 operator and 13 packaging tests passed. Latest prior enrollment/workspace/client suite: 44 tests. Exact immutable package helpers, independent cluster/roles/databases/LaunchAgents and recovery are modeled with synthetic commands. Native APFS/Keychain/launchd and self-contained dependency distribution remain pending. |
| Read-only local cache interfaces | Web/TUI implemented and reviewed; backend/configuration tests passed | `cb755bb5`, `1f0a12fb`, `f4814db4`; three PostgreSQL/HTTP cache tests, two cache configuration tests and nine focused CLI/TUI tests passed. Exact subscription allowlist; same-snapshot freshness and citations; no normal-query merge, model forwarding or write controls. This is not a native Mac acceptance result. |
| Joined prerequisite-based journey | Implemented with synthetic recovery evidence | `d234521d`, `9f0bc203`, `53ba8812`; login/select/workspace/subscription/native provisioning and explicit agent setup share stable journals. Exact replays preserve clones, credentials, completed stages and consent. The latest complete operator/package suite passed 63 tests; two subsequently added short-confirmation/EOF cases also passed. Native dependency distribution remains open. |
| Independent agent identities and settings | Implemented; real PostgreSQL/HTTP, CLI and MCP transport tests passed | `8fb6b39f`, `9705b8bb`, `d0d18d34`, `d91d4f10`, `e94bbb45`, `b95aafa7`, `4b57e512`, `86f83d58`, `27dd79c7`; browser changes `bacae72d`, `bca625a3`. Separate one-project Codex/OpenAI and Claude/Anthropic grants, monotonic denial, read-only cache tools and protected settings merges. No automatic model calls or inherited F1 authority. |
| Guided first-run and runtime packaging | Implemented; synthetic verification passed | `9b855528`, `a63cfbad`, `205fa9e7`, `c13920a9`, `ffeef91b`, `e0b778fc`, `0140a679`; exact optional runtime tree/manifest/launcher and selected-input generation. Latest complete Python suite: 100 tests. No produced native dependency bundle, installer or clean-Mac result is claimed. |
| Local contribution drafts/outbox | Storage foundation implemented, not a submission workflow | `3a14afc3`, `1ec1a6f5`; migration `0076`, bounded captures/pending-add drafts, canonical input binding, CAS history and separate payload storage. Required-DB record/store regression: 45 tests. CLI/UI, human consent, owner authorization and replay remain open. |
| Real macOS acceptance | Pending | Requires native execution; Linux builds/tests are not evidence of this. |

## Invariants under review

- Exact areas are Workshop, Homelab, Trust and General; catalog revisions never
  constitute delivery grants. Trust is remote-only even when an operator edits it.
- Human identity is the configured OIDC issuer/subject, never inferred from email.
  OIDC login must not reactivate a locally disabled principal.
- Effective access includes group grants after an explicit grant is removed.
- New global administration requires central authorization and protected audit;
  browser writes also require CSRF and respect read-only mode.
- IdP credentials stay server-side. The management integration is opt-in and must
  not expose credentials or provider response bodies. External effects have
  durable intents/outcomes; timeout or interrupted confirmation is uncertain.
- Device revocation is independent; disabling a human also fences their linked
  sessions and machine credentials.
- No broad federation pull is used for laptop project selection. The scoped
  gateway feed is opt-in and origin-authenticated; devices cannot access it
  directly or obtain gateway credentials. The device delivery adapter must
  enforce current human/device authority before selecting and transmitting.

## Focused validation and review evidence

The Rust administration tests exercised reviewed catalog compare-and-swap and
immutable history; Trust/offline rejection and unpinned tool rejection; direct
grant removal while group access survives; disabled-user OIDC/session/token
fencing; device revocation; shared HTTP authorization/CSRF/read-only policy; and
rollback when the protected audit append fails. Fake Authentik tests covered
bounded provider responses, managed-path restrictions, durable intents and
uncertain outcomes. No live Authentik mutation occurred.

Astra's independent review found two recovery defects: provider operations lacked
a durable target/correlation, and Web refresh could lose an uncertain operation's
reference. The backend now persists bounded correlation metadata and the browser
restores metadata-only operation history without resubmitting effects. Provider
response bodies and credentials are not displayed. The Web recovery tests include
lost HTTP confirmations, strict operation-ID parsing and reloaded invitation
metadata (`5c27e0d2`).

Migration `0066` had already been applied by the development test helper. It was
restored byte-for-byte to its applied checksum; correlation fields are additive
in `0067`. No migration checksum bypass, source reset or production migration was
used. Device enrollment starts at `0068`.

Device approval binds the reviewed catalog revision as well as the exact
area/project/owner identity. A changed manifest requires renewed review. The
server never releases a new bearer secret through polling: the client prepares
its random credential locally and enrollment approves its hash. This is an
implementation contract, not evidence that native Keychain login already works.

### Combined service/client smoke test

The source-built CLI with embedded PostgreSQL service and TUI features passed a
real HTTP smoke test against the isolated dev database. Anonymous administration
was denied; authenticated metadata had `Cache-Control: no-store`; stable-subject
registration, effective reader access and catalog revision 1 worked; disabling
the synthetic human removed effective permissions. Single-user/local-owner
credentials could neither start production device enrollment nor approve a
device. The fake OIDC issuer and repository URLs are `.example.invalid`; no live
identity-provider call, clone, paid call or existing-service change occurred.

Browser checks of the real built UI confirmed that a non-OIDC local owner sees
an explicit sign-in requirement and a disabled device-review action. The public
return path contains only the user code, not a polling or credential secret.
No browser console errors were observed. This verifies denial behavior, not a
positive Authentik login or Mac Keychain execution.

The package build script now requires Darwin/tooling and an exact native
architecture/target match, uses locked dependency resolution and private staging,
and bundles exact read-only Codex templates. Postinstall has no service restart
or recursive ownership change; existing configurations, read-only files and
symlinks are preserved. No `.pkg` was built or installed on a Mac here.

The initial strict Clippy run encountered Rust 1.99's `async_trait`-generated
`double_must_use` lint in `mem-search` and `mem-reinforce`. Commit `4de07b4b`
adds narrowly scoped, explained compatibility annotations on those two traits;
a focused strict rerun subsequently passed for federation/store/runtime/config,
service and CLI all-targets with embedded PostgreSQL/TUI. Later slices must rerun
it. The boundary check exposed a direct CLI SQLx dependency; `b025cf5b` moved
connection/schema inspection behind the validated runtime API and boundaries
passed again, without weakening the check.

The first unbounded auth regression exhausted the disposable PostgreSQL fixture's
100-client limit (29 pool timeouts). The same suite passed all 57 tests with
`--test-threads=2`; no real database limits or services were changed. Concurrent
Git tests also exposed a briefly fork-inherited workspace lock. `d995af4e`
explicitly releases the OS lock on guard drop; the complete 44-test client suite
then passed. No failing initial run is reported as passing.

### Selective feed and projection

The gateway binds reviewed publication keys to company/area/owner/project/space,
keeps project cursors under transaction locks, masks withdrawn historical events,
and signs nonce/device/revision-bound pages with a distinct purpose. Tests use a
disposable database and actual detached signatures for runtime verification.
Filtered pages verify individual publication receipts, not a complete repository
or contiguous space history. No device downloads an unrestricted CAR.

The separate laptop projector commits the page receipt, read-only projection and
cursor atomically. Lexical search requires an active current source version,
matching canonical state and eligible reviewed evidence. Original authors,
source/shared versions, locators and digests remain available for citations.
It does not write local authoritative memories, embeddings or reinforcement.

Astra's review found that archived memories were searchable and empty approved
projects could not initialize. Both are corrected and included in the six store
tests. Registration is a patch: the owner adapter must explicitly disable a
previously registered stream when it ceases to be eligible, never assume omission
withdraws it. Full source/authority fingerprints and delivery rechecks are part
of the next adapter slice.

The owner adapter now uses append-only migration `0071` for a durable registration
intent/confirmation journal. It checks the exact enrolled device credential,
current linked human permissions, approved manifest and current reviewer/sharing
authority before selecting content and again after HTTP. Lost gateway confirmation
replays the same operation, not a fresh registration. Revocation or source changes
during HTTP deny delivery. Serialization and expiry checking occur under the final
protected transaction; its commit is the delivery authorization linearization
point. These tests passed with actual PostgreSQL and signed fake-gateway replies.
Remote-owner device enrollment is deliberately still closed.

The explicit laptop runner pins audience, local subscription,
company/area/owner/project/space, catalog revision and gateway key schedule; it
does not accept identity or trust from content pages. A private profile permits
only a dedicated loopback laptop database. Starting/reconnecting/polling software
is not evidence that a native Mac service or database has been installed. Five
required-database tests cover restart/partial pagination, signed wrong audience,
mid-pass policy changes, revocation, empty snapshots, no local-memory writes,
HTTP failure sanitization and cancellation during network wait. Missing-subscription
inspection leaves the projection table unchanged. CLI tests check current
offline consent, same-name/different-identity catalog entries, changed bootstrap
and no silent key adoption before initialization.

Astra's bounded read-only replica review found no blocking disclosure/correctness
defect. Its requested missing-subscription and network-cancellation cases are now
tested. The private profile also rejects server database addresses, another
area's database, URL query overrides, missing ports and Trust.

## Completion status

### Native operator and local cache evidence

The prerequisite-based native operator now prepares, confirms and journals its
own per-user PostgreSQL cluster, non-superuser area roles, exact loopback databases
and uniquely named LaunchAgents. It pins the executable, installed PostgreSQL/
pgvector, helper runtime and subscription metadata. Each replay checks current
Keychain-backed scope and preserves interrupted initialization without restarting
an unrelated installation. Python 3.11+ and PostgreSQL/pgvector still require
native installation; synthetic commands do not prove this on a Mac.

Astra found a crash window that could leave a private journal with two hard links,
blocking exact replay. Atomic no-replace rename now publishes it. Tests interrupt
after publication and race a competing file without overwriting it; general
hard-link rejection remains intact. The operator's 21 tests and all 13 packaging
tests pass after this fix and the cache configuration additions.

The opt-in `[laptop_cache]` view requires exact subscription/device/company/owner/
project/space bindings, literal loopback service/database and an exact area
database name. Models, MCP, offline replay, enrichment and autonomous work are
disabled. The operator derives its allowlist only from reviewed metadata.
Inspection never initializes subscriptions; search and displayed cursor/freshness
read the same committed projection. Other stored subscriptions are not exposed.

Astra found a database-URL gate bypass involving `@` inside a query. Parsed URL
validation now occurs before connecting/migrating, using the already locked pure
URL parser. Query and fragment attack variants are rejection tests. This adds no
transport/runtime/database dependency to `mem-config`. A second review found no
further actionable bug in the cache/backend/Web/TUI paths.

Web `More → Offline` and TUI `Offline` display exact subscription identity, last
verified page time, source/shared versions, author and reviewed evidence digests
as plain text. Unknown/stale cache age is not current remote authority. Selection
changes and cancelled requests cannot show another project's stale result.
Ordinary prompts, model clients and MCP are not invoked. Three PostgreSQL/real
HTTP tests and two configuration tests pass; the CLI/TUI focused filter passed
nine tests, including seven new cache tests. The first complete TUI run exposed a
missing help section; it was corrected, and the complete rerun passed 95 tests.
The current OpenAPI inventory and boundary checks also passed. Strict Clippy
identified a collapsible input-bound guard, which was corrected before rerunning.

Company delegate provisioning issues a secret once after the protected audit
commits. Exact replay cannot reissue it, restore revocation, adopt an existing
principal or expand configured ceilings. Five PostgreSQL/HTTP tests include
browser CSRF, audit rollback and administrator expiry during a lock wait.
These credentials have only project-scoped `memory_read`/`auth_manage`; they are
not laptop-delivery authority. Fleet delivery must add separate explicit authority
and must never forward a laptop bearer credential to another owner.

The development instance remains isolated and earlier operational installations
remain unchanged. The last build-cache check was 25 GiB; no broad cache deletion
was performed and the working executable was retained.

### Company device enrollment and owner bridge

Company metadata comes only from configured non-Trust owners using newly scoped
delivery delegates, not membership-management credentials. An actual human
browser session supplies the current issuer/subject from locked database rows;
neither client-reported identity nor email becomes authority. The Web company
marker is explicit and errors never downgrade into local enrollment.

The approval persists exact area/owner/project/revision/digest selections and
stable remote operation IDs before contacting owners. Every owner independently
checks current human read/export permission and the reviewed manifest. Confirmed
receipts and public gateway trust are immutable. Lost confirmations are uncertain;
only the exact request resumes. Local activation is a separate transaction, not
a claim of distributed atomicity. Company devices receive no global permissions
or ordinary local project memberships.

The company proxies selected pages without forwarding the laptop bearer or
cookies. Both owner and company recheck authority after remote calls. Scoped
nonces bind the full identity, including when owners preserve identical source
project UUIDs. Signature, audience, nonce, expiry, manifest and digest checks
use the enrolled bootstrap; a later response cannot silently replace its keys.

The required PostgreSQL auth suite passed all 82 tests with two threads. Two
independent disposable databases and actual company/owner HTTP routers exchanged
real detached-signature gateway pages, replayed exact requests and rejected
unselected projects, Trust, altered retries, generic access and human revocation
after owner bytes were prepared. Four enrollment tests additionally cover lost
owner replies, malformed receipts/bootstrap, cancellation and challenge/session
expiry during unlocked HTTP. These are synthetic services, not live IdP or Mac
acceptance.

The company-hop forgery test then altered a successfully verified owner's gateway
signature in transit. The company returned conflict without knowledge bytes;
the two real-HTTP company delivery tests passed again after this injection was
added. Independent federation/PDS/ATProto checks passed: 85 store/runtime tests,
24 adapter tests, 11 repository/signature verification tests and 41 neutral tests.
The nested Lean build/tests passed with existing linter warnings; these are
modeled protocol regressions, not proofs of deployed onboarding authentication.

Astra identified that a saved device catalog could falsely report current access
after owner revocation. The catalog now checks each current owner subscription
outside database locks, requires the saved bootstrap unchanged, and rechecks the
company binding before returning. Astra's focused rereview found this resolved.
The broader deployed authentication and native Mac environment remain unverified.

The overall plan is not complete. Native package installation, a clean-Mac
journey, live Authentik integration and new live project synchronization have
not been observed. The explicit CLI/projector and prerequisite-based native
operator are development candidates, not a self-contained installer.

Remaining implementation work:

- Local capture/proposal/offline replay onboarding, supported project plugin
  formats and production/validation of reviewed native dependency binaries.
- Online Trust/F1 expertise integration using the new scoped laptop identities;
  a cache disclosure grant does not authorize it automatically.
- Native package/Keychain/LaunchAgent/pgvector/APFS and full clean-Mac acceptance.

R3 remains paused. No existing company area was upgraded and no autonomous
supervisor, paid execution or public release authority was added.

### Terminal project preparation

`memory projects select` renders exact composite identities, the complete
manifest/tool/output policy, templates and local output paths. Explicit
reader/offline consent and confirmation prepare only a metadata journal. Current
device identity, owner catalog and full manifest are checked again after the
terminal closes. Apply/resume uses exact area/owner/project identity and retains
the reader/offline requirement instead of relying on historical approval.

The full CLI suite passed 56 tests with:

```bash
cargo test -p mem-cli --no-default-features --features project-sync,tui --locked commands::auth_enrollment::
```

Twelve new cases cover review/confirmation/cancellation, Trust and unavailable
offline scope, equal names/UUIDs at different owners, changed manifests/devices,
exact-owner apply and offline permission withdrawal before repository/tool
effects. Ratatui TestBackend checks cover small-terminal selection; this is not a
native PTY or Keychain rehearsal. Astra identified and the implementation fixed
lost offline eligibility on apply and invisible selections after wrapping.

### Independent project agents and settings recovery

The owning human browser approves a hash-only request for each Codex/OpenAI and
Claude/Anthropic project grant; administrators cannot substitute another human.
The client stores independently generated secrets only in Keychain. Parent
disablement, device logout/revocation, changed owner policy and expiry remain
effective. Expiry binds PostgreSQL's exact microsecond precision, without a
tolerance window or authority extension. Three focused company-agent tests and
the complete 85-test `mem-service auth::` regression passed against the required
disposable PostgreSQL database.

The native CLI independently fetches only its own receipt. Known denial persists
in a separate monotonic Keychain fence, including during initial activation;
stale concurrent success cannot clear it. Authority is checked again after
network waits and before releasing cache results. Explicit offline consent is
bounded by prior activation/expiry and network failures, not server denials.
The complete CLI enrollment/workspace/picker/agent filter passed 75 tests.

The MCP server exposes exactly cache status and cited lexical search. A real
Tokio duplex JSON-RPC connection test initializes, lists and invokes those tools,
and rejects ordinary `memory_query` and caller-selected project overrides. All
three focused MCP tests passed. This is actual protocol execution on Linux, not
native Keychain/macOS evidence.

Settings previews bind the exact completed workspace, native profile, manifest,
receipt and generated command. Changes append only owned Codex MCP TOML or merge
the matching Claude JSON object; they preserve existing model/custom settings.
Keychain journals contain public generated metadata/digests, never raw settings
backups, bearer tokens or local database URLs. A physical workspace lock
coordinates Memory writers independently of journal directory or provider.
Target-volume staging recovers exact retained partial prefixes while preserving
conflicting files. Same-user, non-cooperating modification in the final rename
window is not a POSIX compare-and-swap guarantee. Native directory locking and
APFS behavior remain acceptance work.

Astra identified the partial-write and per-journal-lock gaps; both were fixed and
the source closure review found no remaining blocker in those fixes. Focused CLI
agent tests passed 18 cases, including interrupted publication and cross-journal
serialization. The full Python operator/package/journey suite passed 63 tests;
the subsequent agent-helper run passed all 13 tests, including two new short
confirmation/EOF cases. All providers/native commands in Python tests are fake.

Explicit `journey-agents --renew-grant OLD_UUID` retains old history/fences and
requires fresh browser approval. It recovers already-prepared replacement IDs,
never automatically renews consent, and leaves expired MCP entries fenced.
`journey-status` reports recorded progress only. Installer confirmation now
accepts `yes` for the displayed immutable plan instead of requiring hand-typed
SHA-256; the exact digest still binds the native CLI effect.

Strict Clippy passed for the embedded-PostgreSQL/TUI CLI and, separately, the
service/MCP/record crates with all targets and no default features. Formatting,
boundary checks, OpenAPI/router inventory and diff checks passed. The CI workflow
now runs the project-sync CLI filter explicitly and a clearly named synthetic
Linux macOS-onboarding contract job. Its YAML parsed locally; no remote CI run is
claimed.

No clean-Mac journey, live Authentik provisioning, customer deployment, paid
provider execution or R3 continuation is claimed. The last cache check was
27 GiB with 369 GiB free; no cache or user data was deleted.

### Guided first-run, optional runtime and local draft foundation — 2026-10-06

The isolated dev service had stopped; only its owned `4250` process was restarted
with the same disposable loopback `55470` database. The Memory resume workflow
confirmed the approved plan remains in execution, not verified complete. Models,
workers and existing area deployments remain unchanged.

Optional runtime packaging accepts a reviewed architecture/prefix-bound manifest
and exhaustive immutable Python/PostgreSQL/pgvector files and notices. It retains
raw manifest hashes across the build and rejects aliases, links and extra files.
Installed verification regenerates the descriptor and checks the complete tree.
The read-only preinstall rejects an existing bundle ID rather than overwriting
executables in use. A runtime-only `memory-onboard` launcher uses the exact pinned
Python and descriptor, isolated imports and a sanitized native environment. It
does not execute dependencies during packaging/postinstall. Native architecture,
loader dependencies and exact-prefix relocation remain unverified; producer
assertions and synthetic files do not establish runtime suitability.

Guided `first-run` logs in and presents the currently authorized catalog, then
generates private inputs rather than requiring manual JSON. It distinguishes
full composite identities and remote-only Trust. Selected identity, revision and
complete public manifest digest are bound by journey v2; v1 remains supported.
Its exact selected input is journaled before publication, preserving IDs after
lost acknowledgement. Existing repositories/custom settings are retained and
new native services reuse the earlier guarded operator. Confirmed apply can
create a missing user LaunchAgents directory without changing existing modes or
jobs. No port conflict causes a legacy service to be stopped.

A separate local contribution store holds raw-capture or pending-add drafts,
bounded typed payloads, canonical hashes and metadata-only local transition
history. Exact replay is harmless; changed source/text/time/scope conflicts;
generation CAS prevents stale updates and completed receipts remain immutable.
It does not initialize, write or advance a delivered replica/projection/cursor.
Accepted metadata describes a future trusted adapter's historical owner outcome,
not something the store independently authenticates. No submission worker or
user interface exists yet. Offline-copy/model grants must not be inferred as
contribution consent, and company devices must retain zero ordinary roles.

Astra's bounded design review found that owner-forwarded federation correction
submission is invalid when the executor is the target origin. The existing
pending-add transaction helper is reusable, but a future contribution adapter
must separately check current human/device/owner consent and commit a full-input
receipt, capture/proposal and R1 event atomically. Legacy offline replay is not
reused: it does not perform that current-permission check. Corrections remain
unsubmitted until an explicit authorized path is designed.

Independent source review found a runtime validator bug rejecting legitimate
leading-underscore CPython modules; `e0b778fc` fixes it with representative
stdlib and `_ctypes` module fixtures. No other actionable finding was reported
in that bounded review. This is not deployed authentication or native proof.

Verification on the final slice:

- `python3 -m unittest discover -s packaging/macos/tests -p 'test_*.py'`: 100
  passed, using synthetic native commands/runtime files only.
- Required disposable PostgreSQL: `cargo test -p mem-record -p
  mem-federation-store --all-targets --locked`: 45 passed, including the new local
  contribution tests and existing durability, expertise and scoped-delivery
  regressions. No customer/production database was used.
- Strict all-target Clippy for those two crates, formatting, boundary checks,
  package shell syntax and diff checks passed.
- Build cache: 28 GiB, 368 GiB free; no deletion or blanket cleanup.

The remaining work is authenticated contribution consent/CLI/replay, supported
non-Memory plugin formats, new-identity Trust expertise, producing the reviewed
native runtime and observing the complete clean-Mac journey. R3 stays paused;
no live Authentik account, existing service, provider call or public release was
changed or enabled.
