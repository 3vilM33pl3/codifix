# Company onboarding deployment — 2026-10-15

Status: **owners upgraded; human login and company provisioning in progress**.
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

- Homelab: `8ed1006729fa5d44805142d7eab4bb103a9479eb0c8b9ec555e360798d3f9e3b`
- Workshop: `6ace99e22a673eaad7c515614aaf8c9a63850adf8a3be350c4fb9255b9aa892f`

The previous containers are stopped, restart-disabled and retained with
`-before-mac-onboarding-20261015` suffixes. **Do not start those older binaries
against schema 84.** Recover forward with the candidate and preserve accepted
post-cutover work. No automatic database rollback is performed.

Both new health endpoints report database up. Anonymous Homelab identity access
returns 401; the new scoped credential returns 200. Company HTTPS root returns
200 on Monolith and the real Mac with normal certificate validation. Login
redirects through Authentik (307); a redirect alone is not a successful login.

Homelab infra commits: `01bae42` (management listener restriction), `9ca42f8`
(private authenticated Memory origins). Memory replacement operator's focused
tests: 11 passed, including schema-changing upgrade forward-recovery behavior.

## Still required

- Complete actual human login, verify the stable subject, provision and privately
  configure exact owner management/delivery delegates.
- Supply company transport credentials in the **process environment**; unlike
  OIDC, current company transports do not read the adjacent env file. Merely
  writing that file is not a completed company setup.
- Review/publish the two onboarding manifests and assign current user access.
- Enroll the Mac device, select projects, retain existing Homelab checkout, clone
  Memory, start local instances and demonstrate both synchronization directions.
- Verify capture/retry behavior beyond service liveness, scoped denial, conflict,
  revocation, reboot and independent Codex/Claude disclosure approvals.

General, Trust, gateway/PDS, existing Pi grants and R3 were not upgraded or enabled
by this owner-only operation. No paid provider was called.
