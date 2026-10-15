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

### Optional reviewed runtime package

On the native build host, an explicitly reviewed redistributable Python and
PostgreSQL/pgvector tree can be included without downloads or laptop source builds:

```bash
./packaging/build-pkg.sh --runtime-bundle /ABS/REVIEWED/RUNTIME-TREE \
  --runtime-manifest /ABS/REVIEWED/runtime-manifest.json
```

`runtime_bundle.py` schema v1 requires an exact architecture and versioned prefix
`/usr/local/share/memory-layer/runtime/<bundle_id>`, explicit producer assertion
that the tree was built for that prefix, exact Python/PG/vector versions,
redistribution notices and an exhaustive SHA-256/mode inventory. Input files must
be `0444` or `0555`; links, traversal, extra files and case aliases are rejected.
Ordinary leading-underscore CPython modules are supported. Files are copied
byte-identically, not patched or codesigned after hashing. Producers must supply
any required signed nested binaries; the Memory binary's separate signing does
not sign the runtime. Manifest bytes are pinned across the build interval.

The optional preinstall is read-only: it rejects another target volume, symlinked
ancestors and an already-existing exact bundle prefix. It never repairs or
overwrites a runtime in use. A changed bundle needs a new ID; interrupted package
extraction or same-ID repair requires operator inspection, not automatic deletion.
Postinstall still performs no dependency execution, login or service activation.

Optional packages also include `memory-onboard`, which runs their exact Python
with isolated imports and the fixed first-party operator. `first-run` binds the
installed descriptor, asks for company login/projects/repository locations and
generates private resumable inputs; no hand-written JSON is needed. See the
[guided first-run procedure](operator-README.md#guided-first-run-with-an-optional-runtime-bundle).

This is packaging/verification software, not a produced or Mac-tested native
runtime. Architecture, loader dependencies and exact-prefix relocatability still
need observed native validation; a producer assertion or hashes cannot prove
them. The original lightweight package remains available without these options.

Do not install a lightweight package over a bundled-runtime installation. Both
variants use `com.memory-layer.pkg`; macOS Installer can delete previously
receipted runtime and `memory-onboard` files omitted from the replacement payload.
New lightweight packages refuse this replacement in preinstall when either the
runtime directory or launcher is present, before payload changes.
Older lightweight packages lack this protection and must not be used for such
an update. This guard does not implement runtime upgrades or repair missing files.

For application-only updates of an existing native installation, build with
`./packaging/build-pkg.sh --arch aarch64 --application-update` (or `x86_64`).
This creates `memory-layer-<version>-macos-<arch>-application-update.pkg` with
the separate `com.memory-layer.application` receipt. It updates Memory, Web
assets and onboarding helpers without taking ownership of or omitting files
from the original runtime receipt. It requires an existing non-symlinked native
installation on the startup volume and cannot include runtime inputs. It does
not upgrade runtime dependencies or start services. Verify the installed
application and existing runtime after installation; package contract tests
alone do not prove macOS Installer behavior.

The bundled `journey` connector joins login, project selection, reviewed workspace
setup and native provisioning. `journey-agents` adds separately approved
Codex/OpenAI and Claude/Anthropic Keychain identities and project MCP entries,
without changing custom settings or enabling paid calls. `journey-status` reports
recorded progress; none of these commands is native Mac acceptance evidence.

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
remains unverified. Reviewed project inventory alone does not clone repositories,
activate named services, deliver offline data or perform selective sync; the
explicit journey operator orchestrates those separately confirmed stages. No
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
python3 packaging/macos/tests/test_onboarding_journey.py
python3 packaging/macos/tests/test_onboarding_agents.py
```

Actual `.pkg` installation/upgrade, signing, launchd behavior and native Keychain
operation still require a separate macOS acceptance run.

## Service labels

- Backend: `com.memory-layer.mem-service`
- Watcher: `com.memory-layer.memory-watch.<project>`
