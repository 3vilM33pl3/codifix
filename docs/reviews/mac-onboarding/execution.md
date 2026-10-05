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
| Administration Web UI | Implemented and focused tests passed | Users/Projects/Devices/Company alongside existing memberships/tokens; latest full run: 203 Web tests and production build passed. This is a development UI, not a deployed company console. |
| Unified cross-area administration | Owner delegation, credential provisioner, coordinator and console grant workflow implemented | `16051bb8`, `9f60b457`, `60d27978`, `cc1fe53a`, `af387f94`; six company tests plus five delegate database/HTTP tests and two delegate configuration tests passed. Explicit owner/token/namespace/role ceiling, ownership guards and durable uncertain outcomes; sanitized registry discovery, exact confirmation, retained operation IDs and distinct current/group rights. Fleet enrollment and live owner provisioning remain pending. |
| Device login and selection | Backend, CLI and browser approval implemented | `9f439c27`, `6ac5b994`, `121fab3b`; 44 required-database auth tests and 15 CLI tests passed. Native Keychain execution and TUI selection remain unfinished. |
| Selective snapshots/offline search | Gateway, local enrolled-owner delivery, bootstrap, private metadata producer and explicit CLI projector/runner implemented | `e741af31`, `d8c5f8db`, `8f578566`, `00d7f2fa`, `09df86da`, `d2d8508f`, `15b649d1`, `46fce2d1`; latest required-database federation store/runtime regression: 84 tests; auth regression: 57 tests with bounded concurrency, plus new configuration test. Two private profile/metadata tests and 41 neutral-core tests passed. Cross-owner device approval, ordinary retrieval/Web/TUI integration and native execution remain pending. |
| Installer and tool/repository setup | Journaled workspace preparation and prerequisite-based native operator implemented and reviewed | `9dabaca8`, `e305955a`, `178560cc`, `894323cb`, `901f6805`, `56b87363`, `730d48c1`, `f087b735`; 21 operator and 13 packaging tests passed. Latest prior enrollment/workspace/client suite: 44 tests. Exact immutable package helpers, independent cluster/roles/databases/LaunchAgents and recovery are modeled with synthetic commands. Native APFS/Keychain/launchd and self-contained dependency distribution remain pending. |
| Read-only local cache interfaces | Web/TUI implemented and reviewed; backend/configuration tests passed | `cb755bb5`, `1f0a12fb`, `f4814db4`; three PostgreSQL/HTTP cache tests, two cache configuration tests and nine focused CLI/TUI tests passed. Exact subscription allowlist; same-snapshot freshness and citations; no normal-query merge, model forwarding or write controls. This is not a native Mac acceptance result. |
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
remain unchanged. The last build-cache check was 24 GiB, with 373 GiB free on the
workspace filesystem; no broad cache deletion was performed.

The overall plan is not complete. Native package installation, a clean-Mac
journey, live Authentik integration and new live project synchronization have
not been observed. The explicit CLI/projector and prerequisite-based native
operator are development candidates, not a self-contained installer.

Remaining implementation work:

- A single company-approved device across configured owners, without exposing
  owner credentials or expanding project authority.
- Company project-selection parity across CLI/Web/TUI. The dedicated local
  read-only cache search/status views are implemented; ordinary retrieval still
  does not merge these records or forward them to a model.
- Separately scoped Codex/OpenAI and Claude/Anthropic MCP credentials and
  disclosure policies, not reuse of the device credential.
- Local capture/proposal/offline replay onboarding, supported project plugin
  formats and reviewed native dependency installation.
- Native package/Keychain/LaunchAgent/pgvector/APFS and full clean-Mac acceptance.

R3 remains paused. No existing company area was upgraded and no autonomous
supervisor, paid execution or public release authority was added.
