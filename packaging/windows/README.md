# Windows Packaging

For the breaking 3.0.0 rename, follow the [release-specific reinstall procedure](../CODIFIX-3-REINSTALL.md) before installing.

Build a native Windows release from PowerShell on a machine with Rust/MSVC, Node, .NET, and WiX 4.0.5 installed:

```powershell
./packaging/windows/test-release-identity.ps1
./packaging/windows/build-windows.ps1 -InstallerBuild 1
```

Build from a clean committed checkout. Outputs are written to
`target/windows/dist/windows-v<version>-build.<n>/`; existing candidate directories
are never replaced:

- `codifix-<version>-windows-x86_64-build.<n>.zip`
- `codifix-<version>-windows-x86_64-build.<n>.msi`
- `codifix-<version>-windows-x86_64-build.<n>-server-tools.msi`
- a matching versioned `.manifest.json` with source commit, installer identity,
  artifact hashes, sizes, signing status and explicitly bundled capabilities
- matching `.sha256` files

The unsigned per-user MSI installs under `%LOCALAPPDATA%\Programs\Codifix`, adds its `bin` directory to the user `PATH`, and bundles the web UI, skill templates, README, example config, PowerShell completion, first-party operators and a legacy localhost PostgreSQL + pgvector Compose template. Native instance installation does not use that template or fall back to Docker/WSL.

The portable ZIP uses the same `bin`/`share` layout. Add the extracted `bin` directory to `PATH` before running `memory`.

Both ZIP and MSI variants require the **Microsoft Visual C++ v14 x64
Redistributable**, installed separately. Use the latest supported x64 package
from [Microsoft](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist);
its version must be at least as recent as the MSVC build tools used for the
application. Codifix's installers do not download or install it. Visual Studio
itself is not an end-user prerequisite.

The Windows release manifest records each packaged executable's actual direct
DLL imports from native `dumpbin /DEPENDENTS`, its inspection-tool version and
whether v14 CRT imports are present. Inspection errors fail the build; this is
not proof of complete transitive DLL closure or execution on a clean PC without
the redistributable. Run `test-runtime-dependencies.ps1` for parser contracts.

`-SkipBuild` reuses only a successful build receipt for the same committed source,
with unchanged executable and Web asset hashes. A failed compiler/build tool
does not silently package an older executable.

### Stable 3.0.0 preview acceptance

The stable application release uses `-InstallerBuild 2`, MSI version `3.0.2`
and `windows-v3.0.0-build.2` artifact identities. These packages remain a
Windows **preview**, not complete native-stack parity. Private lab installer
builds 60000/60001 are intentionally not downgraded in place.

On a disposable GitHub Windows runner, after provisioning a fresh native
PostgreSQL/pgvector database, run:

```powershell
./packaging/windows/check-release.ps1 `
  -DistDir target/windows/dist/windows-v3.0.0-build.2 `
  -InstallerBuild 2 -ExpectedVersion 3.0.0 -ExpectedCommit $env:GITHUB_SHA `
  -TestDatabaseUrl $env:CODIFIX_RELEASE_DATABASE_URL
```

The exact loopback database must be named `codifix_release_*`. The check verifies
manifest/payload hashes, native capture/replay, curation, lexical search, Web
serving and restart, then installs, repairs and removes both MSI variants while
preserving a user-data sentinel. It starts no installed services and makes no
provider calls. It refuses non-GitHub hosts and does not certify AppContainer,
full-stack provisioning or the remaining Windows parity checklist.

## Native service adapter

The bundle includes `codifix-windows-service.exe`, an SCM-aware host for one
foreground Codifix/gateway/operator process:

```text
codifix-windows-service --name Codifix-<instance> --directory <absolute-state-directory> -- <absolute-executable> [arguments]
```

This command is an SCM service entry point, **not** an interactive launcher or
service installer. A machine operator must separately provision the service
identity, protected immutable executable tree, exact instance configuration and
state ACLs. Do not register `codifix service run` directly as an SCM executable.
The adapter starts its child suspended, binds it to a kill-on-close Job Object,
then resumes it. STOP/shutdown cancel the process tree; normal durable leases and
intents handle interrupted work. SCM `Running` means the process was launched,
not that its database/HTTP readiness passed. A dedicated readiness probe remains
required before switching clients. It does not grant AppContainer isolation,
perform migrations or implement a complete machine-level installer.

Windows-only adapter argument tests compile with the platform package:

```powershell
cargo test -p codifix-platform --all-targets --locked
```

Service registration, stop/restart and descendant cancellation still require a
native VM acceptance test; Linux cross-compilation is not that evidence.

## Journaled named-instance operator (native acceptance pending)

`native-instance.ps1` installs or upgrades one **Codifix or federation gateway service**. It is bundled
with the helper scripts in `share/codifix/operators`. It is not an all-stack
installer: provision native PostgreSQL/pgvector and a private exact-instance
configuration separately. PDS/supervisor installation remains separate. Both
supported service paths still require native VM acceptance.

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
   phase history live under `%ProgramData%\Codifix\operations\<operation-id>`.

For a gateway, additionally select `-ExecutableName codifix-federation-gateway.exe`.
Use `-ExpectedInstanceId` for the configured **authority**, add `-ExpectedSpaceId`
and `-StatusAudienceId` for an existing enrolled instance with current Pull
membership, and omit `-HealthUri`. Supply the private gateway **JSON** configuration
with its existing protected credential paths; do not put tokens on the command
line. The virtual service account must already have the required narrow access
to those secrets and database resources. The adapter does not enroll a probe
identity or confer new permissions.

The gateway receives exactly one positional config path. Readiness invokes its
operator-only `check-status CONFIG SPACE AUTHORITY AUDIENCE` command against the
configured numeric loopback listener (wildcard listeners map to loopback).
Nonloopback-specific binds cannot use this probe. It verifies the existing
signed nonce-bound `/v1/status` reply, trusted receipt keys, freshness, space,
authority and audience. The endpoint independently enforces current Pull access
and the database writer fence. Only matched identities, observation time and a
readiness-scope label leave the probe. This is **not** a PDS publication,
repository-integrity or model/supervisor health check; a running SCM service
alone is insufficient. No public readiness endpoint is added.

Releases are hash-verified against the reviewed ZIP and full payload manifest,
then retained under `%ProgramFiles%\Codifix\releases\<release-identity>`.
One virtual account `NT SERVICE\Codifix-<name>` receives modify access only
to `%ProgramData%\Codifix\instances\<name>`, plus read/execute access to its
release. Configuration is administrator-owned under
`%ProgramData%\Codifix\configurations\<name>` and read-only to that exact
service. Shared ancestors carry non-inheriting, exact-service read/control ACEs;
adding another instance preserves these grants without opening sibling data.
Admins and SYSTEM retain operational control. The adapter checks paths,
owners and write ACLs; it never relaxes a broad writable directory into trusted
state. The existing configuration must match on upgrade, and only a strictly
newer installer build is accepted. Unrelated services, instances and user data
are not stopped, replaced or deleted.

The elevated gateway probe adds `--service-name Codifix-<name>`. It resolves
the exact virtual account through SCM and uses an operation-scoped read-only
exception for that SID on protected configuration and its selected audience
token. The exception cannot authorize service ownership, writing, deletion or
ACL changes, and does not change normal protected-file readers. Credential
files must likewise be administrator-owned and read-only to the gateway service;
do not put them in its writable runtime directory. Re-preview old unfinished
operations after this immutable-configuration layout change: their former
command paths intentionally do not match the new operation digest.

Native identity/layout testing (elevated disposable VM only) is available as
`native-instance-layout-tests.ps1 -ServiceHostPath <codifix-windows-service.exe>
-IsolationProbePath <codifix-windows-isolation-probe.exe>` (build the latter with
the `windows-native-tests` feature; it is never a release executable).
It creates two temporary virtual service accounts and verifies their own
read/write state, immutable authority, sibling denial and first-instance access
after the second is installed, using the real protected `Input` under both
identities and the exact-service reader under the elevated operator. These tests have not yet supplied native
acceptance evidence.

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

Hosted Windows CI and the disposable Windows VM use `check-platform.ps1` for
native protected-file/platform tests and `check-onboarding.ps1` for synthetic
Credential Manager and physical-path tests. The latter requires no company
credentials or paid providers. These smoke checks do not certify project sync,
SCM deployment, AppContainer isolation or the full onboarding journey.

## Installer identity and upgrades

Keep `codifix --version` at the application semver (for example `3.0.0-rc.5`).
Allocate a new `InstallerBuild` for each published Windows candidate. The MSI
version is `<application-major>.<application-minor>.<InstallerBuild>`, so RC-to-RC
updates increase even when the application semver stays unchanged. Increase this
build number across application patch releases within the same major/minor; do
not restart it at a patch boundary. Each field uses Windows Installer bounds.

The Codifix per-user UpgradeCode is `C956DBF2-EE64-5A94-B05E-77D35B330F67`.
ProductCode includes the application version and build number; PackageCode also
includes the exact source commit. Do not republish different bytes under one
candidate identity. The candidate `3.0.0-rc.1` build 1 has MSI version `3.0.1`; stable
`3.0.0` must use a later installer build, such as `3.0.2`. Codifix does not
upgrade the old Memory Layer product. Back up and uninstall that product before
installing Codifix; restore state only through the reviewed cutover procedure.

These packages contain the client, SCM adapter and first-party operator
executables, not a complete native Windows server stack. Operator compilation
does not establish native runtime parity. The Compose template is optional
legacy client guidance, not a native PostgreSQL/PDS/Pi dependency installation. The manifest records
`native_stack_bundled: false`; native server acceptance must not be inferred from
a successful MSI build. No signing or public release is performed by this script.

The **server-tools MSI is a machine-level scaffold**, not the completed local
stack installer. It installs the same first-party tools/assets under
`%ProgramFiles%\Codifix Native Server Tools`, without changing system PATH,
provisioning dependencies, adding accounts, starting services or changing data.
Its separate stable UpgradeCode is `356D7E23-5C49-5D9B-BE8E-D6BB9A02F958`; it does
not replace the per-user installer identity. Removing this MSI removes only its
tools directory. Named service releases, configuration and databases created
through the explicit operator are retained and must be retired separately.

## Offline native runtime prerequisites

`operators/native-prerequisites.ps1` is a separate, explicit `Preview`/`Install`
operation. It installs **already built** PostgreSQL 17.11, pgvector 0.8.7 and
app-local MSVC CRT bytes; it does not require Visual Studio, download software,
initialize a database, create services or touch an existing database. Separate
strict manifest kinds also accept prepared pinned Node/PDS and patched Pi
payloads; see [runtime schemas](NATIVE-RUNTIME-SCHEMAS.md). This does not build
those payloads or install/configure their services. `native_stack_bundled`
remains false.

Supply a ZIP and a digest-pinned JSON manifest from an operator-protected input
directory. Manifest fields are `schema_version: 1`,
`kind: codifix-native-postgres-runtime`, `platform: windows-x86_64`,
`postgres_version`, `pgvector_version`, `crt_version`, `package_sha256`, complete
`payload_files` (`name`, `sha256`) and exactly three `components`:
`postgresql`, `pgvector`, `msvc-crt`. Each component has `version`,
`source_sha256`, `license` and a `notice_path` included in the payload. The
bundler must retain the actual upstream licence/redistribution terms; this
operator does not grant redistribution rights or fabricate notices. Include the
full prepared `pgsql` tree, extension SQL/control files and required native DLLs,
not a selected set of executable files. Never bundle database state or secrets.

Run `-Action Preview -OperationId UUID -PackagePath ZIP -ManifestPath JSON
-ManifestSha256 SHA256`. Review its destination/input digest, then repeat with
`-Action Install -ExpectedInputSha256 DIGEST` from an elevated operator shell.
Installed runtimes are immutable, hash-identified directories under
`%ProgramFiles%\Codifix\runtimes`; operation receipts and interrupted
staging remain there for recovery. Repeating the same operation verifies rather
than replacing the runtime; changed inputs require a new reviewed operation.
Service-account access is a separate explicit service-installation concern.
Successful file installation is **not native execution or clean-Windows
acceptance evidence**. Run `test-native-prerequisites.ps1` for portable descriptor
tests; the full clean Windows runtime still requires observed testing.

To bind a named Codifix/gateway installation to these exact local runtime bytes,
pass `-NativeRuntimeManifestPath`, `-NativeRuntimeManifestSha256` and
`-NativeRuntimeOperationId` to `native-instance.ps1` in both Preview and execution.
The operation ID is the successful prerequisite-install receipt, not the named
service's new operation ID. The preview includes this binding; installed files
and the verified receipt are rechecked and held against replacement **before
the selected service stops**, and the registration retains the binding. A later
upgrade cannot silently omit an existing binding. Without these optional fields,
the existing externally managed database installation path is unchanged.
This proves only protected runtime bytes, not PostgreSQL process/database
readiness. Database provisioning/service operation remains separate, and normal
Codifix or signed-gateway readiness is still required. It does not grant a Codifix
participant access to database credentials or runtime files.

### Fresh native database provisioning (native acceptance pending)

The source-tree `native-postgres-preview.ps1` describes one fresh named cluster,
its loopback-only port, exact virtual service account and pinned native runtime.
It accepts the verified runtime manifest/digest/operation, a new provisioning
operation UUID, `-Name`, `-Port`, `-ClusterPath`, `-PasswordFile` and
`-RecoveryReference`. The cluster must be exactly
`%ProgramData%\Codifix\databases\<name>` and must not exist. Existing services
and databases are not adopted, upgraded or stopped.

The initialization password is read from an operator-private file outside the
cluster (32–4096 UTF-8 bytes, one line without a terminator), never from command
arguments, environment variables or the preview output. Its fingerprint is
bound into the overall preview digest but is not emitted separately. The
preview retains protected input/runtime handles and rejects unrelated secret
readers, path aliases and mutable runtime input. SCRAM is mandatory; no trust
authentication or wildcard listener is generated.

After reviewing the preview, `native-postgres.ps1 -Action Provision` accepts the
same inputs plus `-ExpectedInputSha256` from that preview. Exact retries retain
their operation ID and journal; changed inputs fail closed. The operator records
SCM creation, private initialization, configuration and start as separate external
effects, checking cluster identity before resumed changes and authenticated SQL
identity before extension creation. Partial initialization is retained for
inspection rather than deleted or blindly restarted. Temporary password files
and exact operator runtime access are cleaned up without changing the original
password source.

This source implementation still needs **native provisioning/recovery acceptance**.
It is not a completed local-stack installer. PostgreSQL is registered for manual
start; full-stack activation must own reboot policy. Application databases,
non-superuser Codifix credentials, PDS and Pi installation remain separate.
`test-native-postgres-plan.ps1` verifies portable policy/recovery decisions only.
