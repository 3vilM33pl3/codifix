# Native Mac onboarding rehearsal — 2026-10-14

Continuation of [project synchronization](project-sync-execution.md), not a
completed installation or acceptance report.

## Verified environment

- Olivier confirmed the new SSH host fingerprint out of band before its single
  saved host entry was replaced. The previous known-hosts file was preserved.
- Target: `olivier@laptop.metatao.net`, Darwin arm64, macOS 26.6 (25G72),
  Xcode 27.0 (27A266a), Apple Clang 21.0.0, Rust/Cargo 1.99.0.
- Ten logical CPUs, 32 GiB memory; initial free disk approximately 658 GiB.
- Existing Homelab checkout, Codex/Claude configuration and unrelated
  LaunchAgents were discovered and left intact. No existing Memory executable
  or PostgreSQL process was found in the initial scoped checks.
- Native source snapshot `e7f1966c` was copied to
  `/Users/olivier/Library/Caches/memory-layer-build/e7f1966c`, not the user's
  working repositories. Later native fixes must be recorded separately.
- Existing Homebrew Rust/Python were reused as build prerequisites. Node 24,
  CMake and pkgconf were installed with their normal Homebrew dependencies;
  no Homebrew service was enabled. These are build-host dependencies, not proof
  that the resulting runtime is self-contained.

## Observed checks

- Native Web production build passed. Production-only npm audit reported zero
  advisories; the full dependency install reported one high-severity development
  dependency advisory. No blind dependency upgrade was performed.
- Operator tests under Homebrew Python refuse its group-writable Cellar
  ancestor. Existing Homebrew permissions were not changed and path validation
  was not relaxed. Repeat with the protected bundled Python before acceptance.
- Mac HTTPS to Authentik initially fails local certificate verification. The
  public Robot Motel CA root was copied for scoped diagnostic verification only;
  adding it to the user's login Keychain awaits explicit approval. No CA private
  key is transferred. Root SHA-256:
  `4b4daffdab26a650aaea1777ea311451f1a12d29b14ab284357ce9ea02514038`.
- Native Memory release build and native runtime source build started. Neither
  is marked passed merely because compilation is running.

## Runtime production

The source lock pins PostgreSQL 17.11, pgvector 0.8.7 and Python 3.14.8 with
upstream archive SHA-256s obtained from the installed Homebrew formula metadata.
The producer stages files without writing to `/usr/local`, checks native loader
dependencies and ad-hoc signs nested Mach-O files before hashing.

The first native PostgreSQL configure attempt rejected `--without-ssl`; the
producer now uses PostgreSQL's default no-TLS configuration. This distribution
is only for the private loopback cluster. Company HTTPS remains implemented by
Memory. Package signing/notarization, privileged installation and actual
database/vector/Keychain/LaunchAgent behavior still need observed verification.

## Company deployment preparation

Read-only inventory confirmed existing private DNS and Authentik infrastructure
can be reused without creating public DNS records. Live Homelab/Workshop remain
single-user preview services with no company registry/delegates configured.
Canonical Authentik issuer discovery includes a trailing slash while stored
identity namespaces do not. Commit `f91d15c9` preserves both contracts; five
configuration tests and 35 database-backed company tests passed. The identity
provider's subject mode is not changed by this fix.

The running owner containers have no default route and only internal networks.
Their loopback relay must not be exposed as the LAN proxy because it conveys
local-peer semantics. The planned minimal change is one internal ingress network
per owner, connecting only that owner and Caddy; exact HTTPS aliases resolve to
Caddy on that network. Caddy retains its existing Authentik upstream. Owners keep
their current database/federation networks and receive no general egress route.
The new listeners must use multi-user authentication before LAN routing starts.
Existing token callers need an identity/continuity check before that switch.

## Native portability fixes and current status

- PostgreSQL 17.11 compiled natively; its server executable links only Apple's
  system library in the observed `otool -L` output. This is not yet a database
  initialization/restart test.
- pgvector compiled, exposing two actual packaging mismatches: macOS emits
  `vector.dylib`, and staged `pg_config` relocates its directory answers. The
  original install applied `DESTDIR` twice. Commit `5e2f26cf` separates compile
  and final installation paths and accepts hash-bound `.dylib` metadata while
  retaining existing `.so` manifest compatibility.
- Python's first build autodetected Homebrew OpenSSL and failed installation of
  `_ssl`. Commit `aec41938` restricts the build PATH and disables the unused
  Python `_ssl`/`_hashlib` extensions. The operator uses Memory for HTTPS, and
  Python's built-in digest implementations remain available. The incomplete
  Python tree was moved aside, not deleted; native production is being resumed.
- All 112 Python packaging/operator tests pass on Linux after these changes.
  These are not native acceptance evidence.
- SSH became intermittently unavailable during compilation. Existing builds
  survived the first interruption. A temporary two-hour idle-sleep assertion was
  started; no persistent power setting was changed.

## Verified native build milestone

- Full-feature ARM64 Memory compiled and ran `--version`, reporting
  `memory 2.1.0-rc.5-dev`. The initial `e7f1966c` executable SHA-256 is
  `f6c77a81d4d7848629407449930b1e3db9ba69ce126793cb8dfbec2bf6bbb84f`;
  this is not the final candidate hash. Its inspected dependencies are exclusively
  macOS system libraries/frameworks. The issuer fix was transferred and the
  package build is rebuilding against it.
- The 177 MiB runtime now passes exhaustive file/mode/hash validation as well as
  Mach-O dependency and ad-hoc signature checks. PostgreSQL timezone hard links
  are flattened into independent files. Resume preserves matching read-only
  license notices instead of trying to overwrite them.
- Runtime manifest:
  `/Users/olivier/Library/Caches/memory-layer-build/runtime-arm64-v1/runtime-manifest.json`,
  SHA-256 `77d8e304b63856ca5d2f434ca4cd548b3773be20ee38b7ea16ff00cb0c240de2`.
- All **114** packaging/operator tests passed on the real Mac under the bundled
  Python. Native tests exposed `/var` temporary-path aliases, fixture assumptions
  about UID 1000, an installed system `pkgbuild`, and macOS's older `/usr/bin/python3`.
  Fixtures now use the actual user/interpreter and controlled tool discovery.
  Package staging canonicalizes only its build-owned temporary parent; installed
  and user-data path checks remain unchanged.
- A synthetic native PostgreSQL cluster initialized, loaded pgvector **0.8.7**,
  stored a vector, stopped, restarted and returned the same row. No TCP listener
  was enabled. The server was stopped and its disposable state preserved at
  `/Users/olivier/Library/Caches/memory-pg-smoke.LxrJ5G`.
  This test used a staging-only `DYLD_LIBRARY_PATH` for the uninstalled libpq;
  it does **not** replace exact-installed-prefix acceptance.
- The Monolith full release build including the issuer fix passed. It reports
  `memory 2.1.0-rc.5-dev`, SHA-256
  `b37d8a057f99640727755e9971e951c3f1424ba29a2e1e080150de872347d248`.

The PKG, final rebuilt Mac executable hash, user Keychain trust, privileged
package installation, actual company login and two-project journey remain open.
Ad-hoc signing is not Developer ID signing or notarization. No live service has
been switched merely because these native build checks passed.

No live owner database migration, company exposure, device grant, paid provider
call, source-data transfer, or laptop service activation is claimed here.
