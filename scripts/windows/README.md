# Disposable native Windows lab

Host state lives under `/data/VMs/memory-windows-native`, outside the repository.
`vm.py` pins installation media by hash, creates a 200 GiB thin qcow2, prepares
unattended installation with a private one-time password and SSH key, and starts
QEMU with KVM, UEFI, TPM, 8 CPUs and 16 GiB RAM. No host project directory or
credentials are mounted in the guest. SSH is forwarded only on `127.0.0.1:22290`;
the graphical console and guest-agent transports use private Unix sockets.

The VM is synthetic build/test infrastructure, not a production deployment.
Media is downloaded over verified HTTPS from Microsoft's Enterprise Evaluation
Center. Check and record the actual edition/build before accepting the guest.
The current media is evaluation software, not a purchased Windows licence.

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
global SSH configuration. Bootstrap execution has not yet been observed.

Use a local VNC client with `/data/VMs/memory-windows-native/lab/vnc.sock` for
the installation console. Do not expose VNC or SSH to a public interface.

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
first; PDS also needs its copied source root. The stage binds the runner and
input hashes, runs as the logged-in synthetic user through a local scheduled
task and survives SSH disconnects. `Status` returns metadata; execution logs
remain in that private operation directory. A failed/interrupted operation is
never silently rerun under the same ID. Inspect its receipts and use a fresh
operation ID for a deliberate retry. Take the clean snapshot **before** starting
BuildTools. This runner's native scheduling behavior still needs VM validation.
