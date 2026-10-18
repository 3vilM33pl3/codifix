# Codifix live cutover — updated 2026-10-18

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

## Current closeout status

- **Published:** [v3.0.0-rc.1](https://github.com/3vilM33pl3/codifix/releases/tag/v3.0.0-rc.1)
  from immutable source `66c2d1ecc4ca9b0c052e1d854f71a0703f998b71`.
  [Release run 39073102523](https://github.com/3vilM33pl3/codifix/actions/runs/39073102523)
  passed all 11 jobs. Publication reused its verified artifact; no rebuild.
  All 43 downloaded files match the frozen candidate. GitHub normalized four
  Debian filenames (`~rc.1` to `.rc.1`); restoring those names in a separate
  verification directory passed the unchanged `SHA256SUMS`. The release notes
  include exact download/check commands. The release is prerelease, not latest.
  The Release workflow is active again.
- **Client repair complete:** `d71adbe6` corrected active bootstrap/tool names.
  Nine canonical skills are installed on Monolith; obsolete skills and 13 old
  PATH entries are archived outside discovery. The Mac source and origin are
  updated, old wrapper/Claude skills archived, and installed CLI/Workshop/MCP
  reads pass. Its six application jobs kept their PIDs and successful exit state.
  Existing sessions may need reopening to refresh their loaded tool inventory.
- **Documentation:** `917c46ac` updates README, changelog, release notes and
  installation pages to the published candidate. Content/link/asset checks and
  the 89-page production build passed. Vercel production deployment
  `dpl_FFqNR3cEF5KpY8efqx2FEeSTwuSg` is READY at exact source
  `917c46ac56a030ab0fe39d7b32deb4cd06bb882a`. Live `/docs` and
  `/docs/install/linux-debian` returned HTTP 200 with the new release links and
  Debian download mapping. Company Workshop/Homelab health also returned HTTP
  200/status ok on Codifix 3.0.0-rc.1.
- **Follow-up CI repairs:** main run `39109203814` exposed two independent
  issues after candidate publication. `4fcc0838` explicitly releases the
  campaign budget journal lock when its owner exits, including failed resume
  validation. A parallel child can briefly retain a descriptor after the owner
  closes it; the live-owner exclusion remains enforced. Twelve focused tests
  and 20 parallel repetitions (240 executions) passed; Astra reviewed the fix.
  `ba97b17d` fixes synthetic macOS fixtures that hashed the self-hosted runner's
  Python beneath group-writable pyenv ancestors. The fixture now pins private
  synthetic Python and still tests rejection of unsafe ancestors. All 149
  synthetic tests passed under the runner's actual PATH. Production path guards
  and host permissions are unchanged. Both fixes are later main-branch work,
  not changes to the published candidate bytes. Follow-up run `39110168686`
  attempt 2 passed macOS onboarding, format, web build, Windows compilation
  and Offline CA Release Preparation, but exposed three further failures.
  `5014af69` aligns company-agent tests with the optional database harness;
  all four also passed against disposable PostgreSQL with required-DB mode.
  `a1a01374` resolves Cargo's configured target directory for R1 and all five
  R3 smoke steps; the real R1 process-recovery smoke passed using the runner's
  external build directory. `ef8554fc` wraps long footer status text, fixing
  the actual 672px page overflow in a 390px viewport. The F1 browser workflow
  passed again with document width 390px and the original assertion intact.
  Web build and workflow shell/YAML checks passed; Astra found no source
  blocker. Remote CI on these final corrections remains pending.
  Workflow selectors now use runner label `codifix` (`cc13cfe2`); the existing
  registered runner remains online. The redundant tag-triggered Release run
  `39108940650` is confirmed cancelled.
- **Second follow-up batch:** run `39119347339` on `ebc73f36` passed Windows,
  synthetic macOS, format, web and offline CA preparation. Its DB job passed
  the Rust workspace tests, CLI build and real R1 process smoke before failing
  in later R3 fixtures. The three remaining job failures are addressed by:
  - `b67c7e36`: token-environment tests run in exact subprocesses, removing
    their race with API tests; seven affected tests passed 20 parallel runs.
  - `b58f4aa0`: federation fixtures launch the actual `codifix` binary and
    canonical gate variables, with isolated child HOME/configuration. The full
    real-PDS run passed C/D recovery/security, Stage E browser/CLI/PTY, F1
    bidirectional browser/CLI/PTY/MCP, watched-agent and signed-CAR checks.
    Production authorization and its 30-second retry remain unchanged; only
    the initial fixture propagation deadline accounts for that retry.
  - `d7dee064` and `0a30ec30`: local campaign Plan/CheckApprovals dispatch before
    service configuration loading; other campaign children receive explicit
    inert loopback configuration. Existing approval/manifest validation remains
    intact. Empty-HOME integration passed; approved-provider 7/7, review/runner
    2/2 and representative-task 1/1 passed with no skips or paid calls.
  - `3eee1324`: the isolation fixture uses configured Cargo output, the current
    disposable guard and exact Codifix purpose string. Both member boundary
    probes, both restricted SQL/TLS controls and no-host-port/network checks
    passed. Only owned disposable fixtures were cleaned up.
  Astra reviewed the source changes; production auth/routing guards and the CA
  host are unchanged. These are later source fixes, not rebuilt release assets.
  Run `39121056109` subsequently passed DB Integration, private PDS/federation,
  Windows, synthetic macOS, format, web and offline CA preparation. Its only
  failed selected job was Rust Tests: local journal recovery observed no lost
  owner immediately after owner drop. `dd323332` explicitly unlocks all three
  local journal lock scopes, covering normal exit and errors while preserving
  live-owner exclusion and uncertainty-only recovery. A retained duplicate
  descriptor deterministically reproduced the old failure; the fixed platform
  crate passed 7/7 and 20 parallel repetitions passed 60 journal tests. Astra
  found no source blocker. Remote CI on this final correction remains pending.
- **CA pending:** its latest RESULT INBOX still contains only the 17 October
  inventory. Existing console binding/recovery follow-ups remain pending.
  CA/Trust cutover needs Olivier's approval of its concrete scope; SSH remains
  disabled. No CA changes were made during publication or client repair.
- **Explicit distribution limits:** stable 3.0.0 is not released. Mac and Windows
  packages are unsigned; Mac packages are not notarized. Windows remains preview,
  and Homebrew remains HEAD-only. OS reboot, pixel-level UI and a separate Homelab
  contribution test are not claimed by this checkpoint.

Publication evidence: `release-promotion-39073102523/parentmetadata.json`,
`verified-local.json`, `draft-upload-receipt.json`, and the downloaded assets in
that private directory. The artifact ZIP SHA-256 is
`9c169980aa65cebb0ee32064cc90784e470455c136471688eebc6008d47906e4`;
aggregate checksum-file SHA-256 is
`78fb16f3825ac2956ec2e517d9e66dee6ee03f2f414c008801e1af037535b570`.
Astra independently reviewed publication provenance and the four-name mapping.
Future publication verification handles that mapping in `2fdb2ec1`; 12 focused
release tests pass, and the verifier also passed against these 43 real files.

Client evidence: `client-integration-repair/20261018T084018/receipt.json` on
Monolith and `client-integration-repair/20261018T084122/` on the Mac. The Mac's
existing local repository path/project slug remain explicit routing exceptions;
its original six project memories were read successfully. On 18 October,
Olivier authorized the Go prerequisite installation. Homebrew Go 1.27.2 arm64
is installed with its official bottle checksum verified. The actual
`resume-project memory` Go helper passes in the normal login shell through the
unchanged Workshop route, without CLI override variables, and reads all six
original active memories. The earlier missing-Go diagnosis was a non-login SSH
PATH issue: manual Go 1.27.1 already existed and remains preserved. Its old
symlink is archived; the normal user Go link now selects Homebrew. Protected
configuration hashes and six running application PIDs are unchanged. Receipt:
`go-helper-prerequisite-20261018/mac-go-receipt.json` in the private evidence
directory. Plain non-login SSH shells still require the login shell or an
absolute Go path. No CA access or changes occurred.

Follow-up evidence: `ci-followup-391101-attempt2/` contains completed CI job logs,
`token-parallel-local.log`, `pds-canonical-gates-local-4.log`,
`r3-approved-full-local.log`, `r3-review-runner-local.log`,
`r3-rehearsal-task-local.log`, and `isolation-canonical-local-2.log`.
The deterministic journal reproduction and passing tests are retained in
`journal-lock-fix/` as `codifix-journal-before.log`,
`codifix-journal-after.log`, and `codifix-journal-parallel.log`.

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

Historical candidate-progress observations (superseded by publication above):
run `39073102523` was running at source
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
