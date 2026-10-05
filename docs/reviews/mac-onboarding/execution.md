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
| Administration Web UI | Implemented and focused tests passed | Users/Projects/Devices alongside existing memberships/tokens; latest full run: 183 Web tests and production build passed. This is a development UI, not a deployed company console. |
| Unified cross-area administration | Owner delegation and coordinator implemented | `16051bb8`, `9f60b457`; five required-database company tests and 49 auth regression tests passed. Explicit owner/token/namespace/role ceiling, ownership guards and durable uncertain outcomes. Fleet enrollment and company UI integration remain pending. |
| Device login and selection | Backend, CLI and browser approval implemented | `9f439c27`, `6ac5b994`, `121fab3b`; 44 required-database auth tests and 15 CLI tests passed. Native Keychain execution and TUI selection remain unfinished. |
| Selective snapshots/offline search | Gateway and read-only projector implemented | `e741af31`, `d8c5f8db`; six required-database store tests, two signed-page runtime tests and 41 neutral-core tests passed. Denied-project sentinel absence, withdrawn-byte exclusion, independent origins, replay/rollback/cursors, revoked authority, archived/tombstoned exclusion and empty projects are covered. Enrolled-device origin adapter and automatic laptop synchronization are still being implemented. |
| Installer and tool/repository setup | Journaled workspace preparation implemented | `9dabaca8`, `e305955a`, `178560cc`; 13 offline packaging tests and 34 focused enrollment/workspace tests passed. Native area provisioning is unfinished. Astra's follow-up findings on checkout filters, resumed repositories and physical journal containment are being corrected; native APFS acceptance remains pending. |
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
a full strict rerun remains required. This is not recorded as a passed run.

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

## Completion status

The overall plan is not complete. No installer, clean-Mac test, live Authentik
integration or new project synchronization has been claimed. R3 remains paused,
and this work grants no new autonomous supervisor or paid model authority.
