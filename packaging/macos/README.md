# macOS Packaging

Codifix now supports a native macOS service model:

- Homebrew for installation
- `launchd` LaunchAgents for the shared backend and per-project watchers
- `~/Library/Application Support/codifix/` for shared config, env, runtime, and logs

## Current layout

- Shared config: `~/Library/Application Support/codifix/codifix.toml`
- Shared env: `~/Library/Application Support/codifix/codifix.env`
- LaunchAgents: `~/Library/LaunchAgents/`
- Logs: `~/Library/Application Support/codifix/log/`

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
  --notarize --notary-profile "codifix-notary"
```

The build uses `cargo --locked` and a private temporary staging directory,
cleaned on exit. It never clears `target/macos-pkg`. Completions execute the
selected binary during staging, so the build host must be able to run that
architecture (a target installed in Rust alone is insufficient).

Release builds create the standard package first, then reuse its exact native
executable and Web assets for the runtime-preserving update:

```bash
./packaging/build-pkg.sh --arch aarch64
./packaging/build-pkg.sh --arch aarch64 --application-update --skip-build
```

`--skip-build` requires a successful build receipt matching the source commit,
Cargo manifests, binary and every Web asset. It refuses changed inputs rather
than packaging stale output. Both architecture lanes use the same procedure.
`packaging/macos/check-release.sh ARCH VERSION STANDARD.pkg UPDATE.pkg` verifies
the native architecture/version and matching payloads, then installs both
variants and checks preservation of independently owned runtime files. This
installer check refuses non-disposable/non-GitHub hosts; it must not run on an
active user's Mac. Published standard packages require separately installed
Python/PostgreSQL/pgvector for local onboarding; application-update packages
preserve, but do not upgrade, an already installed runtime.

The `.pkg` installs to `/usr/local/`. Post-installation starts or restarts **no
services**. When a non-root console user and their existing Application Support
directory can be verified, defaults are seeded as that user: only a missing
Codifix directory, `log/`, `run/`, config and env file are created. Existing
configuration, secrets, assets, settings, ownership and permissions are retained,
including read-only files and symlinked/relocated paths. If no suitable user or
writable directory exists, per-user seeding is skipped; there is no root-home
fallback or recursive ownership change.

Exact project-scoped Codex templates are installed read-only at:

- `/usr/local/share/codifix/onboarding/.codex/config.toml`
- `/usr/local/share/codifix/onboarding/.codex/agents/astra_adviser.toml`

They are packaged references, not activated global user settings. Installation
does not overwrite `~/.codex/`, deploy project templates, clone repositories or
grant company access.

The package also includes the first-party explicit provisioning operator and
its read-only helper modules under
`/usr/local/share/codifix/onboarding/operator/`. It is not invoked by
postinstall. [Operator inputs, private local provisioning and recovery](operator-README.md)
describe the prerequisite-based candidate: installed Python 3.11+, reviewed
installed PostgreSQL/pgvector and the full architecture-matched Codifix binary
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
`/usr/local/share/codifix/runtime/<bundle_id>`, explicit producer assertion
that the tree was built for that prefix, exact Python/PG/vector versions,
redistribution notices and an exhaustive SHA-256/mode inventory. Input files must
be `0444` or `0555`; links, traversal, extra files and case aliases are rejected.
Ordinary leading-underscore CPython modules are supported. Files are copied
byte-identically, not patched or codesigned after hashing. Producers must supply
any required signed nested binaries; the Codifix binary's separate signing does
not sign the runtime. Manifest bytes are pinned across the build interval.

The optional preinstall is read-only: it rejects another target volume, symlinked
ancestors and an already-existing exact bundle prefix. It never repairs or
overwrites a runtime in use. A changed bundle needs a new ID; interrupted package
extraction or same-ID repair requires operator inspection, not automatic deletion.
Postinstall still performs no dependency execution, login or service activation.

Optional packages also include `codifix-onboard`, which runs their exact Python
with isolated imports and the fixed first-party operator. `first-run` binds the
installed descriptor, asks for company login/projects/repository locations and
generates private resumable inputs; no hand-written JSON is needed. See the
[guided first-run procedure](operator-README.md#guided-first-run-with-an-optional-runtime-bundle).

This is packaging/verification software, not a produced or Mac-tested native
runtime. Architecture, loader dependencies and exact-prefix relocatability still
need observed native validation; a producer assertion or hashes cannot prove
them. The original lightweight package remains available without these options.

Do not install a lightweight package over a bundled-runtime installation. Both
variants use `dev.codifix.pkg`; macOS Installer can delete previously
receipted runtime and `codifix-onboard` files omitted from the replacement payload.
New lightweight packages refuse this replacement in preinstall when either the
runtime directory or launcher is present, before payload changes.
Older lightweight packages lack this protection and must not be used for such
an update. This guard does not implement runtime upgrades or repair missing files.

For application-only updates of an existing native installation, build with
`./packaging/build-pkg.sh --arch aarch64 --application-update` (or `x86_64`).
This creates `codifix-<version>-macos-<arch>-application-update.pkg` with
the separate `dev.codifix.application` receipt. It updates Codifix, Web
assets and onboarding helpers without taking ownership of or omitting files
from the original runtime receipt. It requires an existing non-symlinked native
installation on the startup volume and cannot include runtime inputs. It does
not upgrade runtime dependencies or start services. Verify the installed
application and existing runtime after installation; package contract tests
alone do not prove macOS Installer behavior.

New runtime-containing packages use a manifest-digest-specific
`dev.codifix.runtime.<sha256>` receipt. Each new bundle ID is installed
side by side; Installer must not obsolete the earlier runtime. The launcher
selects the new generation for new setup. Existing services retain their pinned
paths and require an explicit migration/review; installing does not switch them.
Old runtime files and receipts remain until an operator confirms no remaining
consumer uses them. This does not change the existing-prefix overwrite guard.

The bundled `journey` connector joins login, project selection, reviewed workspace
setup and native provisioning. `journey-agents` adds separately approved
Codex/OpenAI and Claude/Anthropic Keychain identities and project MCP entries,
without changing custom settings or enabling paid calls. `journey-status` reports
recorded progress; none of these commands is native Mac acceptance evidence.

Outputs:

- `target/codifix-<version>-macos-x86_64.pkg`
- `target/codifix-<version>-macos-aarch64.pkg`

## Official signing and notarization

For a proper public macOS distribution:

- sign the `memory` binary with a `Developer ID Application` certificate
- sign the `.pkg` with a `Developer ID Installer` certificate
- notarize the final `.pkg` with Apple using `notarytool`
- staple the notarization ticket to the `.pkg`

One-time credential setup:

```bash
xcrun notarytool store-credentials "codifix-notary" \
  --apple-id "<apple-id>" \
  --team-id "<team-id>" \
  --password "<app-specific-password>"
```

Then build, sign, notarize, and staple in one pass:

```bash
./packaging/build-pkg.sh \
  --sign-app "Developer ID Application: <name> (<team-id>)" \
  --sign-pkg "Developer ID Installer: <name> (<team-id>)" \
  --notarize --notary-profile "codifix-notary"
```

Validation:

```bash
pkgutil --check-signature target/codifix-<version>-macos-<arch>.pkg
xcrun stapler validate target/codifix-<version>-macos-<arch>.pkg
spctl -a -vv -t install target/codifix-<version>-macos-<arch>.pkg
codesign --verify --verbose=2 /usr/local/bin/codifix
```

## Homebrew formula

The canonical Homebrew formula now lives in `../../Formula/codifix.rb`.

Install from the tap:

```bash
brew tap 3vilM33pl3/codifix https://github.com/3vilM33pl3/codifix
brew install 3vilM33pl3/codifix/codifix
```

For unreleased `main` branch changes:

```bash
brew install --HEAD 3vilM33pl3/codifix/codifix
```

Release tags publish `codifix-<version>.tar.gz` as the Homebrew source archive.
Refresh `Formula/codifix.rb` only after that archive exists on the GitHub
release and its SHA256 has been verified.

For optional legacy local setup after installation, run these explicitly:

```bash
codifix wizard
codifix service enable
codifix watcher enable --project <slug>
```

`codifix service enable` provisions the shared service API token automatically if it is missing or still set to the development placeholder.

This legacy wizard is not the company onboarding installer journey. Company
device login (`codifix auth login --company https://<company-origin>`) and browser
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

- Backend: `dev.codifix.service`
- Watcher: `dev.codifix.watch.<project>`
