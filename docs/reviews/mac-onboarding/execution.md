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
| Company administration backend | In progress | Reviewed catalog, stable identities, local disablement, device revocation and opt-in Authentik adapter; validation pending. |
| Administration Web UI | In progress | Users/Projects/Devices alongside existing memberships and tokens; validation pending. |
| Unified cross-area administration | Pending | Explicit remote authority and partial-failure handling; a local catalog cannot grant access in another database. |
| Device login and selection | Pending | Browser-authorized, revocable enrollment; CLI/Web/TUI and Keychain. |
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

## Completion status

The overall plan is not complete. No installer, clean-Mac test, live Authentik
integration or new project synchronization has been claimed. R3 remains paused,
and this work grants no new autonomous supervisor or paid model authority.
