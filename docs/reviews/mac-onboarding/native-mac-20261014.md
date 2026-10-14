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
identity namespaces do not; a focused compatibility fix is being tested before
any live configuration change. The identity provider's subject mode is not
changed by this fix.

No live owner database migration, company exposure, device grant, paid provider
call, source-data transfer, or laptop service activation is claimed here.
