# Company onboarding deployment — 2026-10-15

Status: **owners upgraded; Olivier login and two-project company credentials verified**.
This is not completed laptop synchronization acceptance.

## Applied changes

- Homelab and Workshop use the frozen Linux candidate SHA-256
  `b37d8a057f99640727755e9971e951c3f1424ba29a2e1e080150de872347d248`,
  reporting `2.1.0-rc.5`. Existing area IDs, databases, state mounts, PDS/gateway
  credentials and answerer/federation identities are preserved.
- Both databases migrated from 76 to 84 through the same SQLx 0.8.6 and frozen
  migration sources that passed the [isolated rehearsal](company-backup-20261015.md).
  Runtime `verify_only` remains enabled. The 13 new tables receive only the
  rehearsed SELECT/INSERT/UPDATE/DELETE privileges for each existing app role.
- Each owner was stopped before a fresh final encrypted custom dump. Decryption
  and plaintext-hash comparison succeeded before migration. These final dumps
  complement the independently restored earlier backups; they are not another
  full gateway/PDS snapshot.
- Owner containers remain non-root, read-only-root, capability-dropped with
  bounded processes/memory and no published ports. Dedicated **internal**
  ingress networks connect them to Caddy; existing database/federation networks
  and addresses remain. Homelab's observed route table contains no default route.
- Existing public system trust plus the verified public homelab root is mounted
  for HTTPS. No CA private key or PDS credential is exposed to a laptop.
- Authentik provider 5 retains its identity modes, secrets, flows and existing
  callbacks. Only two strict authorization callbacks were appended:
  `https://memory.metatao.net/v1/auth/callback` and
  `https://workshop.metatao.net/v1/auth/callback`.
- Caddy serves these two origins only to LAN/WireGuard and the two exact owner
  ingress IPs. Human authentication is enforced by each Memory owner.
  Caddy administration is bound to its existing monitoring address
  `172.18.0.3:2019`, not the new ingress interfaces. Prometheus metrics still
  return 200; both ingress-interface administration connection probes fail.
  Preserve that management address if recreating the Caddy network/container.

## Client continuity and authority limits

The old installation tokens are not compatible human identities in multi-user
mode. Two new project-scoped `operator` credentials were issued through the
existing authenticated API, stored privately and installed in the existing area
client TOMLs. Their names end in `local-operator-20261015`; they expire on
**2026-11-14** and need operator renewal before then. They cover existing local
read/capture/curation/job duties, not global administration or implied remote
disclosure. Their secrets are not in this repository.

Both new credentials returned authenticated `multi_user` / `service_token`
identity responses. Three existing Homelab/Workshop watcher services were
restarted with their original session/repository identities; they and the manager
are active. An active process that cached an old installation token must reload
its configured credential. No blanket Codex/Claude process restart was performed.

Old local-owner historical records/grants are preserved, but a replacement
principal does not inherit its remote question results or expertise grant.
Human administration and any new-human federation grant require the actual
verified OIDC identity. Existing answerer/supervisor identities were not replaced.
The inactive Workshop question worker remains inactive: its pinned old config
hash must be reconciled explicitly before any future start.

## Evidence and recovery

Private operation directory:
`/home/olivier/.local/state/memory-company-onboarding-20261015-71rLGm`.
It contains original container/config snapshots, before/after client configs,
one-time token receipts, final encrypted dumps, migration/start receipts and
prepared company configuration. Treat it as sensitive operator state.

New running container IDs:

- Homelab: `df3667a60a79c2c37ae7dbba1afe5e4590f77d3fbdfd3b199d9b63cc787e1e6c`
- Workshop: `6ace99e22a673eaad7c515614aaf8c9a63850adf8a3be350c4fb9255b9aa892f`

The previous containers are stopped, restart-disabled and retained with
`-before-mac-onboarding-20261015` suffixes. **Do not start those older binaries
against schema 84.** Recover forward with the candidate and preserve accepted
post-cutover work. No automatic database rollback is performed.

Both new health endpoints report database up. Anonymous Homelab identity access
returns 401; the new scoped credential returns 200. Company HTTPS root returns
200 on Monolith and the real Mac with normal certificate validation. Login
redirects through Authentik (307); a redirect alone is not a successful login.

### Verified human login and dashboard correction

The real browser subsequently completed OIDC login as `olivier` on **both**
company origins. Both displayed the signed-in human, live service and Access
administration. The stable issuer is
`https://auth.metatao.net/application/o/memory-layer`, with subject `olivier`;
the existing `memory-admins` mapping supplies administration. `akadmin` remains
a separate identity without additional Memory grants. The identical contact
email on these accounts does not merge their identities or passwords.

The Authentik Memory application tile still pointed at the obsolete
`http://127.0.0.1:4040/`. Its launch URL alone was changed to
`https://memory.metatao.net/v1/auth/login`; provider, flows, credentials and
permissions were unchanged. Starting from that tile now initiates the actual
Memory callback rather than opening a machine-local legacy installation.

The two project manifests were approved and saved at revision 1. Their
first-party skills archive is pinned to commit `89d52aa8`, SHA-256
`9b60f54852a038a057c3c51c7cc71d7358d42c832bbb1e5b54357a7b104cbbdb`.
A verified HTTPS fetch on the Mac matched that hash. Its existing Homelab
checkout is clean and retains its HTTPS origin; the Memory clone destination
is absent. No project content has yet been downloaded to the Mac.

### Approved two-project company activation

Olivier explicitly approved four 90-day server-only credentials: management and
scoped device delivery for each exact Homelab/Memory project. All four were
issued through the human-authenticated Web delegate review flow and expire on
**2027-01-13**. Private receipts were saved outside the repository; no token was
printed or transferred to the Mac. The Web delegate panel is commit `c4b37e48`;
85 focused access tests and its production build passed.

Homelab was recreated with these four credentials in its process environment.
The prior same-version container is retained stopped/restart-disabled as
`memory-homelab-preview-app-20260919-before-company-delegates-20261015`.
Executable, configuration, Web/state mounts, network addresses and schema are
unchanged. HTTPS `/healthz` reports `ok`, version `2.1.0-rc.5`.

Authenticated delivery-catalog checks against **both owners** returned exactly
the intended project at revision 1 for issuer/subject `olivier`, with offline
access and curated contributions available. Existing group-derived human rights
already authorize this; no redundant direct human grant was created. Trust and
General are absent. Catalog agent destination declarations do not grant actual
Codex/Claude disclosure consent.

The real Mac first-run reached native prerequisite checks but stopped before
device enrollment: SSH could not write the login Keychain. A separate native
`security show-keychain-info` check reported `User interaction is not allowed`.
Olivier was asked to unlock the login Keychain locally; no password collection,
credential fallback or Keychain security change was attempted. Its first-run
journal is retained for exact-input resume.

Homelab infra commits: `01bae42` (management listener restriction), `9ca42f8`
(private authenticated Memory origins). Memory replacement operator's focused
tests: 11 passed, including schema-changing upgrade forward-recovery behavior.

## Still required

- Keep company transport credentials in the **process environment**; unlike
  OIDC, current company transports do not read the adjacent env file. Renew the
  four bounded credentials before their expiry; renewal is not automatic.
- Enroll the Mac device, select projects, retain existing Homelab checkout, clone
  Memory, start local instances and demonstrate both synchronization directions.
- Verify capture/retry behavior beyond service liveness, scoped denial, conflict,
  revocation, reboot and independent Codex/Claude disclosure approvals.

General, Trust, gateway/PDS, existing Pi grants and R3 were not upgraded or enabled
by this owner-only operation. No paid provider was called.
