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

After Olivier unlocked the Keychain locally, SSH still could not use it. A
temporary, non-persistent Aqua LaunchAgent successfully read Keychain settings,
and native `memory auth login` in that session progressed past Keychain storage.
No Keychain security settings were weakened. This distinguishes a login-session
restriction from an incorrect password or a locked Keychain.

The next observed failure is transport access from the GUI session. A standalone
read-only HTTPS probe linked against the candidate's exact reqwest dependencies
returns HTTP 200 over SSH, but the same binary as an Aqua LaunchAgent returns
`HostUnreachable` / OS error 65 for `10.22.6.42:443`. This is consistent with macOS
Local Network privacy; it is not evidence of a TLS failure. Olivier was asked
to allow Memory through the native Local Network prompt/settings. Device
approval, local services and synchronization were blocked by that restriction.
Temporary operator probes use public health metadata only.

### Actual Mac enrollment and installer contract correction

Olivier enabled **Python** in macOS Local Network settings. The bundled-Python
first-run process then reached the company service from its native login session.
Olivier explicitly approved `Olivier Mac`, device
`edeb80fd-e57b-4900-a45d-eea632515676`, fingerprint prefix `9987b4f64f16`,
for offline Homelab/Memory knowledge and locally curated contributions. The Web
reported approval recorded and the Mac persisted enrollment in Keychain. This
does not authorize Codex/Claude onward disclosure or paid model execution.

The real catalog exposed two integration defects not represented by the earlier
Python fixtures: Rust flattens the onboarding manifest, and a company device's
ordinary role remains `reader` even when separately approved for contributions.
Commits `0f67763d` and `d6169434` correct the parser and report explicit current
device contribution consent separately from roles. Original manifest bytes and
digests are unchanged; missing consent defaults to denied. All 88 onboarding
Python tests and five required-PostgreSQL company-device tests passed, including
reader-only contribution consent and owner-denial revalidation.

Homelab was subsequently replaced with the corrected candidate SHA-256
`853da77a3ba5f8194327fea8a9cfbd24e4b887b3d930fd652f1baf49e4b8f5ae`;
container `47acdba062f8b2b520cd7d46f350c99146c745d66c8008d68d274474c949d867`.
Its previous same-schema container is retained stopped/restart-disabled with
`-before-catalog-consent-20261015`. Configuration, credentials, mounts other than
the executable, network addresses and schema 84 are preserved. Both owners'
verified HTTPS health checks report `ok` and database `up` after the replacement.
The native Mac rebuild and project installation remain in progress; enrollment
is not evidence of a completed download or upload.

The subsequent contract check found that workspace journals hash Rust struct
serialization, whereas curated subscriptions hash RFC 8785 JSON. Commit
`fa5ff196` verifies the latter against the reviewed manifest without changing
the original journal digest. An actual Rust-generated Unicode golden vector
backs the regression test. All 26 journey tests passed on Linux and the Mac;
the Mac's 13 first-run tests also passed. Targeted service Clippy and formatting
passed.

The corrected ARM64 executable is SHA-256
`7ce155d69ed06dc5d80ccb440d196c01e3f02bf3f77b1abdfd4a59fece3f163d`.
Its update package is SHA-256
`c8db0383e1cd4ba9b22eff590d0f19f8e03ae75a1007e20710210b7a7c1e818a`,
at the existing Mac build directory's
`target/memory-layer-2.1.0-rc.5-macos-aarch64.pkg`.
This unsigned internal update was intended to reuse the installed runtime. That
assumption proved incorrect during actual installation (see correction below).
The original runtime-containing
package is retained as `memory-layer-2.1.0-rc.5-macos-aarch64-with-runtime-original.pkg`.
Olivier completed this update. Installed executable and operator hashes match,
but Installer removed the omitted runtime and `memory-onboard` as obsolete files
from the same package receipt. No local project database/service had been
created; Keychain enrollment and user repositories were not removed. Do **not**
use the lightweight package to update a runtime-containing installation.

A complete repair package was built from the corrected executable/operator plus
the original verified runtime tree, with no source/dependency rebuild. SHA-256:
`32b3d53ec91e89a7412bcb5f1acfdbae6b10b4af3a694ff120b6c1033921ebd1`.
It is `target/memory-layer-2.1.0-rc.5-macos-runtime-repair.pkg` in the Mac build
directory. The immutable runtime prefix is currently absent, so the existing
full-package preinstall permits restoration. Local Installer authorization and
restored-runtime verification initially remained pending. Commit `5e782a13`
adds a lightweight preinstall guard to prevent repeating this omission; all
18 package contract tests passed.

Olivier subsequently installed the complete repair. The installed executable
and corrected journey helper match their expected SHA-256s, `memory-onboard`
executes again, and exhaustive installed-runtime file/mode/hash validation
passed. The original Keychain/device identity is reused. The GUI-session
first-run was restarted with `onboarding-v2`; macOS requested Keychain access
for the updated executable before project selection.

After Olivier approved Keychain access, the catalog displayed both authorized
projects and their explicit curated-contribution consent. Both were selected;
the Homelab checkout was preserved and missing project Codex templates installed.
Skill installation then correctly refused a version mismatch: the approved,
SHA-256-matching archive declares `2.1.0-rc.5`, while the reviewed pin contains
build metadata, `2.1.0-rc.5+89d52aa8`. No local database or service was created.
Commit `a4bc8b91` permits omission of valid SemVer build metadata only; archive
hashes, prereleases, project manifests and journal identities remain checked.
All 33 focused workspace tests passed; the ARM64 release built natively.

Commit `e68612a6` adds an explicit application-only package using the separate
`com.memory-layer.application` receipt, with no runtime/launcher ownership.
All 21 packaging contract tests passed. The Mac candidate is
`target/memory-layer-2.1.0-rc.5-macos-aarch64-application-update.pkg` in the
existing build checkout, SHA-256
`d636ca9279d7cd5301ec500deef5872b49aa74c9180a9a02a66987a9ddf02b18`.
Its executable SHA-256 is
`0f1950566296b25f138cea3b8285e0b45430fae59814017951ce2a4b4b6c6fdc`.
This unsigned internal candidate has been opened for normal local Installer
authorization; installed runtime preservation remains to be verified, not
inferred from the receipt design. The old executable-bound journals and attempt
logs are preserved; `onboarding-v3` will use the existing approved device and
same two projects. No new login or expanded permission is requested.

The earlier first-run stopped before project selection/native effects. Its
executable-bound journal is preserved. The prepared next attempt uses sibling
state `onboarding-v2` with the existing Keychain device, not a new enrollment.

Homelab infra commits: `01bae42` (management listener restriction), `9ca42f8`
(private authenticated Memory origins). Memory replacement operator's focused
tests: 11 passed, including schema-changing upgrade forward-recovery behavior.

## Still required

- Keep company transport credentials in the **process environment**; unlike
  OIDC, current company transports do not read the adjacent env file. Renew the
  four bounded credentials before their expiry; renewal is not automatic.
- Select projects, retain existing Homelab checkout, clone
  Memory, start local instances and demonstrate both synchronization directions.
- Verify capture/retry behavior beyond service liveness, scoped denial, conflict,
  revocation, reboot and independent Codex/Claude disclosure approvals.

General, Trust, gateway/PDS, existing Pi grants and R3 were not upgraded or enabled
by this owner-only operation. No paid provider was called.
