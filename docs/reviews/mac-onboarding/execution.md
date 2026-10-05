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
| Administration Web UI | Implemented and focused tests passed | Users/Projects/Devices alongside existing memberships/tokens; 170 Web tests and production build passed. This is a development UI, not a deployed company console. |
| Unified cross-area administration | Pending | Explicit remote authority and partial-failure handling; a local catalog cannot grant access in another database. |
| Device login and selection | In progress | Exact browser review, bounded per-device approval and current-human authority checks; CLI/Web/TUI and native Keychain acceptance remain unfinished. |
| Selective snapshots/offline search | Pending | Project-scoped authorization before bytes leave the server; verified publication data only. |
| Installer and tool/repository setup | Pending | Journaled, pinned, non-destructive native Mac workflow. |
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
- No broad federation pull is used for laptop project selection. Scoped snapshots
  are a remaining feature, not an existing property of the instance/space stream.

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

## Completion status

The overall plan is not complete. No installer, clean-Mac test, live Authentik
integration or new project synchronization has been claimed. R3 remains paused,
and this work grants no new autonomous supervisor or paid model authority.
