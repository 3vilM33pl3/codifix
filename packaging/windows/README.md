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
- `memory-layer-<version>-windows-x86_64-build.<n>-server-tools.msi`
- a matching versioned `.manifest.json` with source commit, installer identity,
  artifact hashes, sizes, signing status and explicitly bundled capabilities
- matching `.sha256` files

The unsigned per-user MSI installs under `%LOCALAPPDATA%\Programs\Memory Layer`, adds its `bin` directory to the user `PATH`, and bundles the web UI, skill templates, README, example config, PowerShell completion, first-party operators and a legacy localhost PostgreSQL + pgvector Compose template. Native instance installation does not use that template or fall back to Docker/WSL.

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

## Journaled named-instance operator (native acceptance pending)

`native-instance.ps1` installs or upgrades one **Memory service**. It is bundled
with the helper scripts in `share/memory-layer/operators`. It is not an all-stack
installer: provision native PostgreSQL/pgvector and a private exact-instance
configuration separately. Gateway/PDS/supervisor service installation remains a
separate native acceptance item.

1. Allocate one stable operation UUID. Run `-Action Preview` with `-Name`,
   `-OperationId`, `-PackagePath` (candidate ZIP), `-ManifestPath`,
   `-ManifestSha256`, `-ConfigPath`, `-HealthUri`, `-ExpectedInstanceId` and
   `-RecoveryReference` pointing to actual operator recovery evidence. For an
   upgrade preview, also pass `-PreviewAction Upgrade` (the default is Install).
   `/healthz` must be loopback HTTP and report that exact configured UUID in
   `area_instance_id`, not the process-local `instance_id`.
2. Review the metadata-only preview. Execute the same inputs elevated using
   `-Action Install` or `Upgrade`, its `-ExpectedInputSha256`,
   `-DatabaseCompatibilityConfirmed`, and the same `-RecoveryReference`.
   The action and recovery reference are bound to the preview and retries.
   The adapter performs **no database backup**.
3. Reuse the exact operation ID and inputs after interruption. Changed inputs
   fail closed; completed retries do not restart a service. Journals and observed
   phase history live under `%ProgramData%\MemoryLayer\operations\<operation-id>`.

Releases are hash-verified against the reviewed ZIP and full payload manifest,
then retained under `%ProgramFiles%\Memory Layer\releases\<release-identity>`.
One virtual account `NT SERVICE\MemoryLayer-<name>` receives modify access only
to `%ProgramData%\MemoryLayer\instances\<name>`, plus read/execute access to its
release. Admins and SYSTEM retain operational control. The adapter checks paths,
owners and write ACLs; it never relaxes a broad writable directory into trusted
state. The existing configuration must match on upgrade, and only a strictly
newer installer build is accepted. Unrelated services, instances and user data
are not stopped, replaced or deleted.

Interrupted extracted staging trees are renamed and recorded, never silently
deleted. A failed readiness check retains the new release and journal; it does
**not** restore old executables or a stale database. Repair forward using the
same operation if inputs are unchanged. A failed first installation that needs
different inputs, a legacy service without a receipt, or a conflicting immutable
release requires explicit operator reconciliation; the adapter will not adopt
or overwrite it automatically. Keep previous releases and recovery evidence.

Pure validators and atomic journal replacement can run without Windows/admin:

```powershell
./packaging/windows/test-native-instance.ps1
./packaging/windows/native-instance-validation-tests.ps1
```

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

These packages contain the client, SCM adapter and first-party operator
executables, not a complete native Windows server stack. Operator compilation
does not establish native runtime parity. The Compose template is optional
legacy client guidance, not a native PostgreSQL/PDS/Pi dependency installation. The manifest records
`native_stack_bundled: false`; native server acceptance must not be inferred from
a successful MSI build. No signing or public release is performed by this script.

The **server-tools MSI is a machine-level scaffold**, not the completed local
stack installer. It installs the same first-party tools/assets under
`%ProgramFiles%\Memory Layer Native Server Tools`, without changing system PATH,
provisioning dependencies, adding accounts, starting services or changing data.
Its separate stable UpgradeCode is `B209C344-2641-4F0C-BEAF-37A9C827C243`; it does
not replace the per-user installer identity. Removing this MSI removes only its
tools directory. Named service releases, configuration and databases created
through the explicit operator are retained and must be retired separately.
