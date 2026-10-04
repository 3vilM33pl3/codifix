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

## Current host prerequisite

On 2026-10-04, loading `kvm_amd` failed with `Operation not supported`.
Kernel message: `kvm_amd: SVM not supported by CPU 4`. None of the 32 logical
CPUs exposes the `svm` flag. The Ryzen 5950X supports AMD virtualization, so
firmware SVM/AMD-V must be enabled and verified after an operator-managed reboot.
Do not reboot Monolith automatically or substitute software emulation.

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
