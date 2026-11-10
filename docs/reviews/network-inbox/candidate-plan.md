# Finish the outstanding Network/Inbox engineering work

Approved 10 November 2026. Base: feat/network-inbox at 2ed92c9b.
Finish line: build and validate an installable Linux candidate. No production deployment, real AT login, live Linear writes, public publication or CA/SSH changes.
Models: Luna for bounded packaging/metadata/docs; Sol for transport/integration; Astra coordinates reviews and corrections. Separate worktrees and incremental commits.

- [x] Tracking: create “Network/Inbox — Remaining engineering work” under Codifix in Notion with dependencies, owners, acceptance and evidence; retain approved plan in repo.
- [x] Candidate identities: core 3.1.0-rc.1, desktop 0.2.0-rc.1; update coordinated manifests/locks and explicit Windows allocation; preserve old release identities; previous stable < candidate < next stable in Debian/upgrade checks.
- [x] Linux artifacts: codifix-agency included in existing DEB and Bookworm build; inactive companion templates/examples; separate optional broker archive with locked production dependencies, source, launcher, inactive unit, notices and provenance. Node >=22, validated on24. Installation creates no grants, starts no service and migrates no live database.
- [x] Broker hardening: JSON complexity checks independent of declared MIME when consumed as JSON, legitimate handle text retained, cloning cannot bypass; SDK stays0.5.13. Optional broker remains disabled/non-production with exact unresolved advisory recorded, not represented as fixed.
- [x] Complete isolated workflow: native transport -> human HTTP service -> Inbox ticket -> agency/model -> plan_prepare/apply -> Linear fixture readback/cards; disposable PostgreSQL full chain through0097, synthetic test-only auth, persisted human access, exact known plan UUID/digest grants. No writes during prepare; wrong/stale scope and revocation denied; interrupted/uncertain effects reconciled without duplicates.
- [x] CI/install validation: new crates trigger required DB jobs, explicit broker tests/audit, workflow/package checks wired; fresh install/upgrade/repair/removal in disposable container harness while preserving hosted-runner guard. Known advisory explicit; unexpected findings fail. Existing authority/disclosure/budget/auth/cancel regressions stay green.
- [x] Delivery: Astra review corrections resolved; committed source; core/desktop/broker candidate artifacts with hashes/source revision; runbooks and new acceptance evidence distinguish native transport, renderer, startup/package tests from live accounts. Notion statuses updated, plan verified complete.

Another operator must be able to install and exercise the candidate in an isolated environment using the delivered artifacts/instructions.


