# Codifix live cutover — 2026-10-17

This is the current operational checkpoint. The full rename remains in progress;
older rehearsal notes describe historical states, not the live deployment.

## Delivered

- Workshop, Homelab and General run Codifix 3.0.0-rc.1 with existing area identities,
  credentials and canonical data. All three passed health, authenticated reads and
  actual writes. Existing PostgreSQL clusters/volumes are retained.
- Active app/database container and relay service names use Codifix. Local old
  federation work is retired; original signed history, receipts and keys remain.
- GitHub is `3vilM33pl3/codifix`; repository identity and history are retained.
  Implementation is on `main` and `codex/codifix-technical-rename`.
- `https://codifix.dev/docs` serves the Codifix site. The old documentation host
  redirects with its path intact. Legacy v2.1 downloads are explicitly labelled.
- The installed command is `~/.local/bin/codifix`. Area client configuration is
  in `~/.config/codifix/areas/`. Scoped MCP wrappers are copied to
  `~/.local/state/codifix-clients/`; existing credentials are unchanged.
- The canonical checkout is `/home/olivier/Projects/codifix`. Workshop project
  `codifix` retains UUID `fb83249e-c78c-43eb-a4ba-928513a9023a` and existing memory.
  Its path-keyed configuration/state/cache key is `codifix-6b784a8b3a7500a1`.
  The repository helper successfully resumes it through the Workshop API.
- The separate source dev service runs Codifix 3.0.0-rc.1-dev against a fresh
  `codifix_project_dev` database/login. Health reports `status=ok`, `database=up`.
  The old dev unit is disabled; old dev/shared databases remain unchanged.
  Discovery, offline replay, automation, background and model work stay disabled.

The gateway is also live on a fresh Codifix space, retaining its original
authority and history. Signed status returned 200; missing/invalid authentication
returned 401. The runtime retirement-marker privilege fix is committed as
`90c9805c`, built, deployed and independently reviewed by Astra. A rollback-only
privilege check accepted marker SELECT and rejected marker UPDATE and unrelated
project-table SELECT. Private evidence: `gateway-live-evidence.md`.

Ordinary renewal of four existing service leaves under the unchanged preview
issuer restored PDS session refresh. Twelve fresh membership records are
published and signature-verified. Workshop, Homelab and General are reconnected,
each at cursor 12 with zero sync failures; original retired cursor 23 and local
permissions remain preserved. There were zero source subscriptions before and
after, and no knowledge was republished. Evidence: `nonca-reconnected-smoke.json`.

## Approved local certificate renewal

Olivier approved the scoped renewal on 17 October. It is installed with unchanged
signing/service keys, subjects, SANs, extensions and permissions. The local root
now expires **16 November 2026 at 22:05:06 UTC**; the four existing leaves expire
**24 October 2026 at 22:05:06 UTC**. Active Codifix trust copies and fingerprint
pins, including the refresh maintainer's bound policy, were updated together.
Historical evidence and original bytes remain in the private recovery set.

All ten named containers are running. Verified TLS/hostname checks passed for
all four endpoints; Workshop/Homelab/General health and federation cursor 12
passed with zero sync failures. The existing PDS session remains generation 284.
The separate ca.metatao.net host, SSH and Trust enrolment were untouched.
Evidence: `approved-root-renewal/result.json` and `smoke.json` in the private
operational evidence directory below.

## Remaining work

- Publish final release packages after the corrected candidate completes.
  The Mac runtime, sync and original watchers are restored as described below;
  its private app build does not establish signed/notarized distribution.
- Refresh running client sessions. Codifix desktop plugin is installed/enabled;
  the legacy plugin is disabled with its cache retained. Primary MCP project
  access and both scoped stdio wrappers passed. Watch manager, Workshop/Homelab
  watchers and existing embedding bridges/timers are restored. The General latch
  was confirmed stale and archived; normal scheduling resumed without a manual
  provider call. After crash recovery, this Codex session exposes the renamed
  primary and scoped Codifix MCP tools; other existing clients may still need reload.
- CA stays on its existing installation. Any change there requires Olivier's
  approval; SSH remains disabled. CA work does not block other participants.

## Recovery and temporary remnants

Private operational evidence and one recovery bundle per changed area are under
`~/.local/state/codifix-rename-20261017/`. These contain credentials and dumps and
must not be committed. The initial area startup evidence is
`live-area-startup-evidence.json`; background restoration is recorded separately.

The old checkout path is a temporary symlink so already-running clients continue
working. Original configuration, generated `.mem` files, stopped app containers,
Docker volume/network identifiers and recovery bundles are retained. Remove the
session link after clients reopen the canonical path; backups need explicit
approval before deletion. These retained objects do not run the old application.

The current area binary SHA-256 is
`94e50052aed58b3d5e227f2cafb801805a19114b81e95347e14c479166d3f8c3`.
Website behavior was verified at source `79186b2f494c9ca92de5c42dc79dd86fbe20bbd7`.
Source validation, Linux candidate builds and both Linux installation acceptance
jobs passed. Apple Silicon build/native acceptance and Windows x64 also passed.
Superseded candidate run `39068603586` predates the gateway fix and was cancelled
while its remaining Intel Mac build was running, freeing the release concurrency
slot. Its completed artifacts are retained, including the staged Mac installer.
These builds do not gate the running Monolith applications.

Corrected unpublished candidate run `39073102523` is running at source
`66c2d1ecc4ca9b0c052e1d854f71a0703f998b71`, including the gateway fix. The crash
recovery snapshot showed Linux amd64/arm64 and Apple Silicon builds active with
no failures reported. The later 22:56 UTC snapshot confirms release contract,
source archive and full source/security validation succeeded, including the Rust
audit with its documented RSA exception. The final Mac-restoration snapshot has
eight successful jobs, also including both Linux builds/installation checks and
Apple Silicon build/native acceptance. Intel Mac and Windows x64 remain running;
none has failed. No public release or tag has been published.

Crash recovery reconfirmed all three Monolith area health checks. The initial
Mac installer authentication blocker is resolved.

Follow-up found the old dev unit and a duplicate legacy-labelled Workshop relay
had restarted after the crash. Both are now disabled/stopped; the Codifix relay
and source dev unit are active. Workshop remains healthy. Dev overlay recovery
copies and the scoped provisioning script are retained privately under
`dev-restoration/` and `provision-isolated-dev.py` in the operational evidence
directory. Existing PostgreSQL administrator privileges sufficed; authentication
and shared cluster roles were not changed.

## Mac runtime cutover

Olivier completed the native runtime installer. One cold recovery copy preserves
3,188 files. The existing PostgreSQL cluster system identifier
`7696918038469587144`, role/database OIDs, password verifiers, project/device/
subscription IDs and original caches are retained. Both databases are renamed
and migrated from schema 84 to 85; old federation bindings are retired with
their signed evidence retained. Existing audit rows are unchanged; migration
history and one definitions-registration audit per area account for the changes.

The three Codifix core launchd jobs are active and healthy. Separate active
plists in `~/Library/LaunchAgents` retain normal login/restart settings; the
bound helper's inert originals are unchanged. All seven old labels are disabled.
Original cached search and history passed for both areas, with all 161 table
fingerprints unchanged during those reads. Astra independently reviewed the
preservation, activation and read evidence; no core-runtime blocker remains.

The native desktop app is copied from the successful private test build; this
does not establish signed/notarized distribution. Olivier completed the reviewed
additive Keychain transfer and allowed the Codifix executable to read the two
existing credentials. The completed transfer command was:

```bash
/usr/bin/swift /Users/olivier/.local/state/codifix-rename-20261017/macos-state.swift --apply --company-origin https://memory.metatao.net
```

Original credentials and profiles remain preserved. The copied app profile now
points to native Workshop at port 55441 with its original UUID; the application
is open. The obsolete separate source-dev process is stopped with its DuckDB
data retained. Both original sync profiles retain their original company origin
(`memory.metatao.net`) and distinct owner identities; company origin and area
owner endpoint are separate bindings.

Olivier enabled Local Network access for the renamed helper. Authenticated
requests then reached the company service but returned HTTP 409: the retained
enrollment referred to historical project/federation bindings. The normal fresh
enrollment flow, already selected in the original plan, resolves that conflict
without rewriting immutable server records. Both offline queues and unresolved
contribution queues were empty; six accepted contribution receipts are retained.

Normal logout revoked the old device. Olivier signed in through Authentik, and
the portal recorded approval of replacement device
`7a1172f3-3e42-4150-a8d4-8e49969a9deb` for exactly the original Workshop and
Homelab project scopes, with offline/contributions enabled and the same 30-day
grant duration. Both server outcomes are confirmed. A transient Authentik page
error cleared after reload; no authentication configuration was changed.

Fresh subscriptions now synchronize successfully: Homelab reached cursor
990/through 990 and Workshop 3582/through 3582, both caught up with zero errors.
Their branded persistent sync jobs are running. Old subscription IDs, profiles,
projections and legacy Keychain records remain retained as historical evidence.
Normal browsing now exposes only the current subscriptions, avoiding duplicate
historical views while retaining the old profiles and projections on disk.

The fresh-device binding operator in `107aa90f` records exact old/new bindings
in the existing immutable receipt table, then updates the disabled local binding
atomically. It checks the original scope, retired/fresh spaces, drained queue and
pinned approval/bootstrap evidence. Twelve focused tests passed against real
PostgreSQL/schema 85, plus the two existing retirement/same-device checks. The
existing same-device function and all applied migrations are unchanged. Astra
reviewed both actual sealed manifests, and both transitions are now applied.
Each changed only the local binding and added its immutable receipt; all other
existing table rows and schema were verified unchanged at the transition.

Seven persistent jobs are running: PostgreSQL, two area services, two sync jobs
and both original-scope watchers. The six application jobs successfully restarted
from their unchanged persistent plists, and both areas remained healthy. Separate
pinned operational configurations leave the cold-bound helper inputs intact.
Original watcher checkpoints are preserved; no new provider/model calls were
enabled. Cookie-backed reads of both original projects returned HTTP 200, and
fresh history reads passed. One truthful Workshop cutover note produced two normal
curated records; both received accepted owner receipts. Workshop then caught up
to cursor 3586. This establishes Workshop upload acceptance; no new Homelab
upload was tested. The six original accepted outbox rows/receipts and all 3,188
recovery-file hashes remain intact. Astra's final evidence review found no
concrete blocker. Full OS reboot and pixel-level UI acceptance were not tested.

Private evidence: `mac-actual-evidence/` (including `company-approval.jsonl`,
`company-approval.png`, `fresh-binding-review.json`,
`fresh-binding-apply-result.json`, `current-functional-result.json`,
`current-restart-result.json`, and `final-complete-runtime.json`),
`mac-actual-read-result.json` and `mac-actual-retirement-result.json`.

The original Mac company origins initially returned 502 because application
container recreation had omitted their internal onboarding network attachments.
Restored Homelab at `192.168.64.3` and Workshop at `192.168.64.11` on their
existing dedicated internal networks; saved recreation specs now include them.
Both `https://memory.metatao.net/healthz` and
`https://workshop.metatao.net/healthz` return the correct healthy Codifix areas.
Existing credential origin bindings remain intact. No Caddy configuration, CA,
firewall or public exposure change was made. Private evidence:
`mac-origin-restoration/result.json`.
