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

`-SkipBuild` reuses only a successful build receipt for the same committed source,
with unchanged executable and Web asset hashes. A failed compiler/build tool
does not silently package an older executable.

## Native service adapter

The bundle includes `memory-windows-service.exe`, an SCM-aware host for one
foreground Memory/gateway/operator process:

```text
memory-windows-service --name MemoryLayer-<instance> --directory <absolute-state-directory> -- <absolute-executable> [arguments]
```

This command is an SCM service entry point, **not** an interactive launcher or
service installer. A machine operator must separately provision the service
identity, protected immutable executable tree, exact instance configuration and
state ACLs. Do not register `memory service run` directly as an SCM executable.
The adapter starts its child suspended, binds it to a kill-on-close Job Object,
then resumes it. STOP/shutdown cancel the process tree; normal durable leases and
intents handle interrupted work. SCM `Running` means the process was launched,
not that its database/HTTP readiness passed. A dedicated readiness probe remains
required before switching clients. It does not grant AppContainer isolation,
perform migrations or implement a complete machine-level installer.

Windows-only adapter argument tests compile with the platform package:

```powershell
cargo test -p mem-platform --all-targets --locked
```

Service registration, stop/restart and descendant cancellation still require a
native VM acceptance test; Linux cross-compilation is not that evidence.

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
