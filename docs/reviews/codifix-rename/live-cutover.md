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

- Complete the native Mac installation and client wiring. The Apple Silicon
  candidate package and native acceptance passed; packages are verified and
  staged. Installation requires the user's local administrator authentication;
  a runtime-inclusive installer handoff is ready at
  `~/.local/state/codifix-rename-20261017/mac-install-and-resume.md`. It installs
  the verified payload/runtime; preserved-state activation follows. Publish final release
  packages after the corrected candidate completes.
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
no failures reported. No public release or tag has been published.

Crash recovery reconfirmed all three Monolith area health checks. The Mac
installer has not yet run: the Codifix binary, runtime and package receipt are
absent, and all seven original jobs remain loaded. Its verified installer still
requires local administrator authentication; activation remains pending.
