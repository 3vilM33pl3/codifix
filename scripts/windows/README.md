# Disposable native Windows lab

Host state lives under `/data/VMs/memory-windows-native`, outside the repository.
The lab control directory, firmware, TPM, journals and recovery copies stay there;
an explicitly selected active disk may use the dedicated private NVMe directory
described below. No other VM or installation is relocated.
`vm.py` pins installation media by hash, creates a 200 GiB thin qcow2, prepares
unattended installation with a private one-time password and SSH key, and starts
QEMU with KVM, UEFI, TPM, 8 CPUs and 16 GiB RAM. No host project directory or
credentials are mounted in the guest. SSH is forwarded only on `127.0.0.1:22290`;
the graphical console and guest-agent transports use private Unix sockets.
Guest timezone and virtual RTC both use UTC, independently of Monolith's
daylight-saving timezone. Confirm guest/host UTC agreement before receipt tests.

The VM is synthetic build/test infrastructure, not a production deployment.
Media is downloaded over verified HTTPS from Microsoft's Enterprise Evaluation
Center. Check and record the actual edition/build before accepting the guest.
The current media is evaluation software, not a purchased Windows licence.

`NativeEval` is a separate, Limited interactive proof stage requiring an explicit
clean copied source directory. Its bound inputs are `guest-native-eval.ps1`,
`guest-evaluation-proof.mjs`, `evaluation-helper.exe` and `r3_score.exe` in the
private guest inputs directory. It runs deterministic knowledge answers through
the actual AppContainer helper, checks excluded-project/hidden-grader denials,
and requires fabricated citations to fail while human judgments stay unresolved.
It also drives a real native command/final-export loop with a fake model,
verifies nested payload disclosure controls, and checks output/deadline failure
receipts. No provider is invoked. The separate `guest-rust-evaluation.ps1` /
`guest-rust-proof.mjs` diagnostic prepares an explicit private Rust/MSVC/library
subset for actual positive, wrong-answer and broken-source compilation controls;
its source alone is not execution evidence.
Receipts retain exact source/tool hashes and private operation paths. This is not
an official R3 campaign, a Rust grading pass or a hard disk-limit claim; it makes
no model calls.

`guest-native-eval-bootstrap.ps1` is a separate fixed `PrepareOnly` action for
consolidated acceptance. Run it from the latest clean copied source in the
interactive lab account, for example:

```powershell
& "$SourceRoot\scripts\windows\guest-native-eval-bootstrap.ps1" -Action PrepareOnly `
    -SourceRoot $SourceRoot -OperationId ([guid]::NewGuid()) `
    -RustProofOperationId 'a8cf16d2-5913-4ee7-ac68-48bb6b5a2e80'
```

It rechecks that retained Rust proof and preserves its profile/toolchains,
builds only the current helper/Memory/three R3 examples with `--locked --offline`
under the shared product-build mutex, and writes a new private derived profile.
The helper's outer Job launches only native pack preparation; the database URL
is a synthetic port-9 placeholder and no database is contacted. New receipts and
input/output hashes stay below `runs/NEW_OPERATION/native-eval-bootstrap`.
The full campaign guard remains closed. No existing evidence is overwritten and
no tool installation, provider call or VM operation is part of this script.
Portable checks are `python -m unittest scripts/windows/test_native_eval_bootstrap.py`;
the additional PowerShell parser/helper check requires `pwsh`.

## Host prerequisite and recovery

On 2026-10-04, loading `kvm_amd` failed with `Operation not supported`.
Kernel message: `kvm_amd: SVM not supported by CPU 4`. None of the 32 logical
CPUs exposes the `svm` flag. The Ryzen 5950X supports AMD virtualization, so
firmware SVM/AMD-V must be enabled and verified after an operator-managed reboot.
Do not reboot Monolith automatically or substitute software emulation.

On 2026-10-16 Olivier enabled SVM; exact `svm` appeared on all 32 threads,
`/dev/kvm` became accessible and QMP confirmed KVM acceleration. The default
lab also needs the narrow `swtpm.apparmor.example` include if the installed
AppArmor profile does not permit its `/data` paths. Preserve existing local
rules and reload only the swtpm profile; do not disable AppArmor.

Windows 11 Setup requires Secure Boot. For the existing uninstalled lab, stop
the guest and run `python3 scripts/windows/vm.py enable-secure-boot` once. This
retains a complete pre-change snapshot and selects packaged Microsoft-enrolled
OVMF variables, secure firmware and SMM. Cold snapshots now include `lab.json`
so restoration also restores the matching firmware descriptor. An interrupted
firmware switch blocks boot until its retained state is inspected.

The first installation failed with `UnattendProcessDiskConfigInfo` /
`0x8007000D`: the seed incorrectly used partition IDs as modification order,
skipping order 2. The corrected seed uses orders 1, 2 for partitions 1, 3,
matching [Microsoft's UEFI example](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-setup-diskconfiguration-disk-modifypartitions-modifypartition-order).
For this uninstalled disposable lab only, `refresh-seed` rebuilds the seed while
preserving the existing account password and SSH key and retaining previous
seed files. It does not change the disk; subsequent installation still wipes
only the disposable lab disk as declared in its original answer file. Never
use installation media/seed recovery to repair an installed working system.

## Resume installation after KVM is available

The lab was already created; do **not** run `create` over it.

```bash
python3 scripts/windows/vm.py start --install
python3 scripts/windows/vm.py boot-key  # only if firmware asks to boot from CD
python3 scripts/windows/vm.py status
python3 scripts/windows/vm.py ssh 'Get-Content C:\MemoryLab\bootstrap-result.json'
```

The bootstrap installs signed VirtIO network/serial drivers, the native guest
agent and native Windows OpenSSH. Password authentication is disabled. A first
SSH host key is retained in this lab's private `known_hosts`, never the user's
global SSH configuration. Bootstrap completion has not yet been observed.

The installed lab reached the desktop and its guest agent works. Its optional
OpenSSH Windows capability download timed out. `guest-ssh.ps1` offers an explicit
lab-only recovery path using the official, hash-pinned Microsoft OpenSSH archive;
it additionally requires valid Microsoft signatures, reuses the existing public
key and refuses to replace an existing SSH service or race active servicing.
It does not enable password authentication. This preview OpenSSH package is VM
administration tooling, not a Memory customer runtime dependency. Its native
installation was observed on 16 October, followed by a key-only connection as
`memory-winlab\memorylab`. The firewall-rule creation required a `DisplayName`
fix; only the inspected remaining steps were resumed, without regenerating
keys. See the [execution evidence](../../docs/reviews/windows-native/parity-20261016.md).
Use QGA for recovery if necessary; do not repeatedly start optional-feature installs.

Use a local VNC client with `/data/VMs/memory-windows-native/lab/vnc.sock` for
the installation console. Do not expose VNC or SSH to a public interface.

### Guest-agent automation when SSH is unavailable

Use the existing private `guest-agent.sock`, without modifying the VM disk,
credentials, SSH configuration or seed:

```bash
python3 scripts/windows/vm.py guest-info
python3 scripts/windows/vm.py guest-copy-in scripts/windows/guest-build-tools.ps1 --target 'C:\MemoryLab\inputs\build-tools-qga.ps1'
python3 scripts/windows/vm.py guest-exec --capture-output 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -- -NoProfile -NonInteractive -Command '[Environment]::OSVersion.VersionString'
python3 scripts/windows/vm.py guest-exec-status 1234
python3 scripts/windows/vm.py guest-exec-status 1234 --show-output --saved-status qga-status-1234-RECEIPT.json
```

Use the PID/receipt actually returned, not the example placeholders. Execution
is asynchronous and uses literal executable/argument arrays, never implicit
shell parsing. It runs as the guest-agent service (normally SYSTEM), **not** the
interactive user; this does not validate user-context Keychain/Credential Manager
or watcher workflows. Capture requires `--capture-output`; status prints only
exit/stream metadata until `--show-output`. Completed replies are consumed by
QGA, so they are retained as private host receipts for explicit later display.
Output is JSON-escaped, including terminal controls and invalid UTF-8.

Copies are limited to regular files up to 512 MiB and create-new paths beneath
`C:\MemoryLab\inputs`. Existing targets are refused. Transfers use bounded
base64 chunks, flush/close, then byte-count/SHA-256 read-back verification.
Interrupted or failed copies remain for inspection; use a new filename, not
blind overwrite/retry. This is a trusted synthetic guest transfer helper, not
a hostile-guest filesystem sandbox. Never pass secrets on argv: QGA itself can
log command arguments. No command retries an uncertain process launch or write.
Protocol framing follows the [official QGA reference](https://www.qemu.org/docs/master/interop/qemu-ga-ref).

## Cold snapshots and recovery

```bash
python3 scripts/windows/vm.py shutdown
python3 scripts/windows/vm.py status  # wait for the guest and TPM to stop
python3 scripts/windows/vm.py snapshot clean
python3 scripts/windows/vm.py restore clean
```

Snapshots copy the disk, UEFI variables and TPM state together. They refuse a
running guest or an existing snapshot name. Restore retains every previous
component in a private `restore-*` journal directory. An interrupted switch
blocks boot; `recover-restore restore-<id>` completes an already prepared switch.
An interrupted preparation requires inspecting the retained state first; no
automatic deletion or replay of uncertain work occurs. These are cold storage
operations, not evidence that Memory's database restore has passed.

### Optional cold disk relocation

If the rotating `/data` disk is saturated, preserve the existing installed VM
while moving only its active disk to the dedicated user-owned NVMe directory:

```bash
python3 scripts/windows/vm.py shutdown
python3 scripts/windows/vm.py status  # QEMU and swtpm must have exited
python3 scripts/windows/vm.py relocate-disk --destination-directory /home/olivier/.local/share/memory-windows-lab/NEW-PRIVATE-NAME
python3 scripts/windows/vm.py start
```

Choose a new direct child directory; relocation never adopts or overwrites an
existing destination. It refuses pending restore/firmware recovery, copies the
existing standalone 200 GiB qcow2 sparsely, verifies exact source/copy SHA-256
and geometry, and retains the inactive original with matching firmware/TPM
recovery metadata under the lab. No conversion, reinstall, credential reset,
live mirroring, symlink or mount trick is involved.

An atomic `active-disk.json`, separate from guest `lab.json`, selects storage.
Missing/invalid configured storage stops startup; it never falls back to the
inactive HDD disk. Its initial hash records the cold copy, not a claim that the
disk remains unchanged after guest writes. Snapshots use the selected disk.
Restore stages and retains replacement/previous disks beside the active disk
on NVMe, while journaling under `/data`, so interrupted cross-filesystem recovery
does not rely on a cross-device rename. Restoring an older guest configuration
cannot silently switch storage back to the old disk. Never manually boot both
copies; inspect incomplete relocation/restore evidence before taking new actions.

Host-only checks:

```bash
python3 -m unittest discover -s scripts/windows -p 'test_*.py'
```

Native dependency, application, sandbox and installer acceptance remain pending
in [the execution ledger](../../docs/reviews/windows-native/README.md).

## Native build tooling

After the clean guest/bootstrap snapshot, copy `build-tools.json` and
`guest-build-tools.ps1` with `vm.py copy-in <file> --name <filename>`. Invoke
the latter through `vm.py ssh`, passing the copied manifest. It refuses non-lab
hosts, verifies every artifact before execution/extraction, checks Microsoft's
bootstrapper signature and retains a resumable result under
`C:\MemoryLab\BuildCache`. The manifest pins compiler, Node, Git, .NET and WiX;
mutable upstream URLs must still match their reviewed hashes.

This provisions the disposable developer snapshot only, not the customer native
stack. Native PostgreSQL/PDS/Pi and clean-install runtime dependency acceptance
are separate checklist items. PowerShell parsing and host manifest tests do not
constitute native installation evidence.

## Native PostgreSQL proof

After the build-tools snapshot, copy `native-postgres.json` and
`guest-native-postgres.ps1` into the guest. Run the script with that manifest and
`-Action Prepare`, then `-Action Start` (which also verifies the extensions).
`Verify` repeats SQL checks; `Stop` stops only its fixed lab cluster. Native
execution is still pending. The script refuses other hosts and never targets
the host's or a customer's database.

The reviewed EDB PostgreSQL 17.11 binary archive and pgvector 0.8.7 source are
SHA-256 bound to verified HTTPS downloads. pgvector is built with MSVC; vector,
pgcrypto and pg_trgm must load in native PostgreSQL. A local DPAPI-protected
synthetic password authenticates the loopback-only port 55471; no production
credential is copied. Preparation and SQL execution have separate receipts.
This is dependency evidence tooling, not the finished server installer.

## Native PDS dependency proof

`guest-native-pds.ps1 -ManifestPath <native-pds.json> -SourceRoot <copied-source>`
prepares only a disposable runtime. It reuses the exact package and dependency
lock extracted from the pinned PDS image, not newly resolved upstream versions.
The package manager tarball has a reviewed SHA-512; Node is the pinned 24.18.1.
It builds only better-sqlite3 with native MSVC and the manifest-pinned embeddable
Python, tests in-memory SQLite, rejects runtime reparse points and inventories
the resulting native tree. All state stays below `C:\MemoryLab\NativePds`.

That receipt explicitly leaves PDS protocol verification false. Follow it with
the stdio probe and actual Rust named-pipe/signed-repository tests before claiming
native PDS support. No accounts, public identities, PDS service or production
credentials are provisioned by dependency preparation. See
[upstream provenance](../../packaging/windows-pds/upstream/PROVENANCE.md).

Long provisioning stages can use the lab-only `guest-stage.ps1`: `Start` takes
a stable `OperationId` and one of `BuildTools`, `PostgresPrepare`,
`PostgresStart`, `PdsPrepare`. Copy the scripts/manifests into `C:\MemoryLab\inputs`
first; PDS also needs its copied source root. `PiBuild` additionally binds
`guest-native-pi.ps1` and `native-pi.json` and requires a copied source root. It
runs the pinned builder and fake-provider verification under the same operation
ID, not inherited-broker/AppContainer supervision. The stage binds the runner and
input hashes, runs as the logged-in synthetic user through a local scheduled
task and survives SSH disconnects. `Status` returns metadata; execution logs
remain in that private operation directory. A failed/interrupted operation is
never silently rerun under the same ID. Inspect its receipts and use a fresh
operation ID for a deliberate retry. Take the clean snapshot **before** starting
BuildTools. Observed results and remaining acceptance work are recorded in the
Windows execution ledger; dispatch alone is never success evidence.

The host helper avoids hand-written nested PowerShell quoting:

```bash
python3 scripts/windows/vm.py guest-stage-start BuildTools --operation-id YOUR-NEW-UUID
```

Use a real new UUID, not that placeholder. `dispatch_requested` is not success;
inspect the operation's `status.json` and retained logs. The host and guest
runner digests must agree before dispatch. Both launch layers use a
process-scoped execution policy, never a machine-wide policy change. Native API
checks and Pi compilation run as the ordinary interactive user (`Limited`), while dependency
installers use `Highest`; elevated scheduled logons may lack a usable Credential
Manager session even when their name/SID matches the desktop user.
