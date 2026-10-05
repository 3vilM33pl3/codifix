# macOS Packaging

Memory Layer now supports a native macOS service model:

- Homebrew for installation
- `launchd` LaunchAgents for the shared backend and per-project watchers
- `~/Library/Application Support/memory-layer/` for shared config, env, runtime, and logs

## Current layout

- Shared config: `~/Library/Application Support/memory-layer/memory-layer.toml`
- Shared env: `~/Library/Application Support/memory-layer/memory-layer.env`
- LaunchAgents: `~/Library/LaunchAgents/`
- Logs: `~/Library/Application Support/memory-layer/log/`

## Native .pkg installer

Build a standalone macOS `.pkg` from the repo root on Darwin. Install Cargo,
Node/npm and Apple's `pkgbuild` tools first; this script does not install build
dependencies. The selected Rust target must match the package architecture:

```bash
./packaging/build-pkg.sh                          # unsigned, native architecture
./packaging/build-pkg.sh --arch x86_64            # Intel package
./packaging/build-pkg.sh --arch aarch64           # Apple Silicon package
./packaging/build-pkg.sh \
  --sign-app "Developer ID Application: ..." \
  --sign-pkg "Developer ID Installer: ..."
./packaging/build-pkg.sh \
  --sign-app "Developer ID Application: ..." \
  --sign-pkg "Developer ID Installer: ..." \
  --notarize --notary-profile "memory-notary"
```

The build uses `cargo --locked` and a private temporary staging directory,
cleaned on exit. It never clears `target/macos-pkg`. Completions execute the
selected binary during staging, so the build host must be able to run that
architecture (a target installed in Rust alone is insufficient).

The `.pkg` installs to `/usr/local/`. Post-installation starts or restarts **no
services**. When a non-root console user and their existing Application Support
directory can be verified, defaults are seeded as that user: only a missing
Memory directory, `log/`, `run/`, config and env file are created. Existing
configuration, secrets, assets, settings, ownership and permissions are retained,
including read-only files and symlinked/relocated paths. If no suitable user or
writable directory exists, per-user seeding is skipped; there is no root-home
fallback or recursive ownership change.

Exact project-scoped Codex templates are installed read-only at:

- `/usr/local/share/memory-layer/onboarding/.codex/config.toml`
- `/usr/local/share/memory-layer/onboarding/.codex/agents/astra_adviser.toml`

They are packaged references, not activated global user settings. Installation
does not overwrite `~/.codex/`, deploy project templates, clone repositories or
grant company access.

The package also includes the first-party explicit provisioning operator and
its read-only helper modules under
`/usr/local/share/memory-layer/onboarding/operator/`. It is not invoked by
postinstall. [Operator inputs, private local provisioning and recovery](operator-README.md)
describe the prerequisite-based candidate: installed Python 3.11+, reviewed
installed PostgreSQL/pgvector and the full architecture-matched Memory binary
are required. No prerequisites are silently downloaded or source-built, and a
self-contained dependency distribution/native Mac acceptance is not complete.

Outputs:

- `target/memory-layer-<version>-macos-x86_64.pkg`
- `target/memory-layer-<version>-macos-aarch64.pkg`

## Official signing and notarization

For a proper public macOS distribution:

- sign the `memory` binary with a `Developer ID Application` certificate
- sign the `.pkg` with a `Developer ID Installer` certificate
- notarize the final `.pkg` with Apple using `notarytool`
- staple the notarization ticket to the `.pkg`

One-time credential setup:

```bash
xcrun notarytool store-credentials "memory-notary" \
  --apple-id "<apple-id>" \
  --team-id "<team-id>" \
  --password "<app-specific-password>"
```

Then build, sign, notarize, and staple in one pass:

```bash
./packaging/build-pkg.sh \
  --sign-app "Developer ID Application: <name> (<team-id>)" \
  --sign-pkg "Developer ID Installer: <name> (<team-id>)" \
  --notarize --notary-profile "memory-notary"
```

Validation:

```bash
pkgutil --check-signature target/memory-layer-<version>-macos-<arch>.pkg
xcrun stapler validate target/memory-layer-<version>-macos-<arch>.pkg
spctl -a -vv -t install target/memory-layer-<version>-macos-<arch>.pkg
codesign --verify --verbose=2 /usr/local/bin/memory
```

## Homebrew formula

The canonical Homebrew formula now lives in `../../Formula/memory-layer.rb`.

Install from the tap:

```bash
brew tap 3vilM33pl3/memory https://github.com/3vilM33pl3/memory
brew install 3vilM33pl3/memory/memory-layer
```

For unreleased `main` branch changes:

```bash
brew install --HEAD 3vilM33pl3/memory/memory-layer
```

Release tags publish `memory-<version>.tar.gz` as the Homebrew source archive.
Refresh `Formula/memory-layer.rb` only after that archive exists on the GitHub
release and its SHA256 has been verified.

For optional legacy local setup after installation, run these explicitly:

```bash
memory wizard
memory service enable
memory watcher enable --project <slug>
```

`memory service enable` provisions the shared service API token automatically if it is missing or still set to the development placeholder.

This legacy wizard is not the company onboarding installer journey. Company
device login (`memory auth login --company https://<company-origin>`) and browser
approval are separate foundations; native macOS Keychain/installer acceptance
remains unverified. Reviewed project inventory does not clone repositories,
activate named services, deliver offline data or perform selective sync. No
complete Mac/company onboarding or public release is implied by these scripts.

## Offline packaging contract checks

These tests use synthetic users, build tools and payloads, without building a
native package, signing, notarizing, installing dependencies or changing services:

```bash
for script in packaging/build-pkg.sh packaging/macos/postinstall.sh packaging/macos/seed-user.sh; do
  bash -n "$script" || exit 1
done
python3 packaging/macos/tests/test_package_contract.py
python3 packaging/macos/tests/test_onboarding_operator.py
```

Actual `.pkg` installation/upgrade, signing, launchd behavior and native Keychain
operation still require a separate macOS acceptance run.

## Service labels

- Backend: `com.memory-layer.mem-service`
- Watcher: `com.memory-layer.memory-watch.<project>`
