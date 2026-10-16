# Windows Packaging

Build a native Windows release from PowerShell on a machine with Rust/MSVC, Node, .NET, and WiX 4.0.5 installed:

```powershell
./packaging/windows/test-release-identity.ps1
./packaging/windows/build-windows.ps1 -InstallerBuild 1
```

Build from a clean committed checkout. Outputs are written to
`target/windows/dist/windows-v<version>-build.<n>/`; existing candidate directories
are never replaced:

- `memory-layer-<version>-windows-x86_64-build.<n>.zip`
- `memory-layer-<version>-windows-x86_64-build.<n>.msi`
- a matching versioned `.manifest.json` with source commit, installer identity,
  artifact hashes, sizes, signing status and explicitly bundled capabilities
- matching `.sha256` files

The unsigned per-user MSI installs under `%LOCALAPPDATA%\Programs\Memory Layer`, adds its `bin` directory to the user `PATH`, and bundles the web UI, skill templates, README, example config, PowerShell completion, and a localhost PostgreSQL + pgvector Compose template.

The portable ZIP uses the same `bin`/`share` layout. Add the extracted `bin` directory to `PATH` before running `memory`.

## Installer identity and upgrades

Keep `memory --version` at the application semver (for example `2.1.0-rc.5`).
Allocate a new `InstallerBuild` for each published Windows candidate. The MSI
version is `<application-major>.<application-minor>.<InstallerBuild>`, so RC-to-RC
updates increase even when the application semver stays unchanged. Increase this
build number across application patch releases within the same major/minor; do
not restart it at a patch boundary. Each field uses Windows Installer bounds.

The existing per-user UpgradeCode stays `7E6D7DA0-7D74-43F0-B816-E422B9E01B82`.
ProductCode includes the application version and build number; PackageCode also
includes the exact source commit. Do not republish different bytes under one
candidate identity. The first new build (`2.1.1` as an MSI version) supersedes the
older `2.1.0` MSI, but is still application `2.1.0-rc.5`.

These packages currently contain the **client**, not a complete native Windows
server stack. The Compose template is optional legacy client guidance, not a
native PostgreSQL/PDS/Pi dependency installation. The manifest records
`native_stack_bundled: false`; native server acceptance must not be inferred from
a successful MSI build. No signing or public release is performed by this script.
