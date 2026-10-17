# Native Windows supervisor candidate

The Linux artifact pins, Docker launcher and legacy Linux-only control probe
remain unchanged. The latter returns `wrong_architecture` on Windows rather
than claiming a Linux ELF probe establishes Windows readiness.

The Windows execution path requires a reviewed work manifest containing
`sandbox.windows_artifacts`: exact upstream source revision, both reviewed
patch digests and the digest of the **observed native build**. The surrounding
work-manifest digest binds these fields. There is no default Windows binary
digest and no fallback to another provider, executable, container or network.

## Build (disposable Windows guest)

From an x64 MSVC Developer PowerShell with Git, tar, Rustup and native C build
dependencies, using an existing local Git repository containing the pinned Pi
commit:

```powershell
./crates/mem-supervisor-pi/scripts/build-windows.ps1 `
  -SourceRepository C:/memory-lab/pi-upstream `
  -BuildRoot C:/memory-lab/pi-native-build-1 -InstallToolchain
```

The build root must not exist. Source is extracted from the exact Git commit,
not the source checkout's working tree. The script verifies both patch hashes
and the unchanged upstream lock, then records the resulting binary digest and
compiler metadata. No provider credentials may be present. `build-evidence.json`
explicitly does not claim execution or isolation verification. Preserve the
complete upstream notice in `patches/LICENSE.pi`; internal patch approval is not
a public redistribution or legal-clearance decision.

Stage the exact driver (`memory-pi-sandbox.exe`) and built `pi.exe` in an
operator-owned private directory before referencing them in a work manifest.
The runtime currently requires the same strict private ACL policy as authority
files for these inputs; arbitrary broadly readable installation directories are
not silently accepted. `private_runtime` must also be a protected local directory.

## Execution boundary

The host copies and hashes both executables into a dedicated flat bundle, then
launches the driver with zero-capability AppContainer and a resource-limited
Job. Only standard RPC/diagnostic pipes and one already connected private
broker handle are inherited. Credentials remain in the trusted broker; Pi gets
only a one-work bearer, not a provider key. The driver starts Pi suspended,
verifies identical AppContainer identity/current job, then resumes it.

The narrow Windows patch intercepts the fixed Responses request before VCR or
network forwarding. It performs one bounded framed exchange over that inherited
handle. The host dispatches its exact body through the existing admission,
durable accounting and disclosure checks. It does not forward arbitrary paths
or destinations. Cancellation/deadline closes ingress and kills/reaps the entire
Job; recovery remains governed by existing durable uncertain-attempt policy.

After the entire job is empty, code/scratch ACLs and integrity labels are
restored and the AppContainer profile is deleted. Failed cleanup remains an
error; its private operation directory is retained for operator inspection.
This is not a delegated-task or automatic retry mechanism.

## Verification status

Observed on Linux: existing Pi tests (16 passed, 5 explicitly ignored), runtime
tests (40 passed, 4 explicitly ignored), Windows GNU all-targets compilation and
Windows library/binary Clippy. Database tests can return early without the
required database environment; these totals do **not** establish database or
Windows runtime acceptance. Linux-only Docker execution tests remain Linux-only.
The synthetic authority-file fixtures now use actual private ACL creation on
Windows rather than attempting Unix mode operations.

Pending in the native Windows guest:

```powershell
cargo test --locked -p mem-supervisor-pi --lib
cargo test --locked -p mem-supervisor-runtime --lib launcher::platform::tests
cargo test --locked -p mem-platform --lib
cargo build --locked -p mem-platform --features windows-native-tests --bin memory-windows-isolation-probe
./target/debug/memory-windows-isolation-probe.exe
```

Repeat the isolation probe through the actual Windows service host to exercise
nested Job constraints. The scratch execution test attempts DACL self-modification
before execution; an inherited non-execute ACE is not by itself proof of denial.
The full successful launcher test must also require the returned proposal digest
and removal of the completed operation directory: protected code handles must be
released only after verified cleanup, but before that directory is deleted.
Build and run the pinned native Pi with a fake provider, then exercise the full
durable supervisor workflow and revocation/cancellation/restart through the native
launcher. Until those runs are observed, native supervisor parity remains open.
