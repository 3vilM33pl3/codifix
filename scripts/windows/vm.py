#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Disposable native Windows lab. No production mounts, passwords or shell eval.

QEMU/KVM runs as the invoking user. Only SSH is forwarded, on host loopback.
Cold snapshots include the disk, UEFI variables and TPM state together.
Never fall back to software emulation or touch a VM not created by this tool.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import time
from urllib.parse import urlparse
import xml.etree.ElementTree as ET

FORMAT = "memory-windows-lab-v1"
DEFAULT_ROOT = Path("/data/VMs/memory-windows-native/lab")
SCRIPT_DIR = Path(__file__).resolve().parent


def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, **kwargs)


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def private_root(path):
    path = Path(path)
    if not path.is_absolute() or not path.is_relative_to("/data/VMs"):
        raise ValueError("lab root must be an absolute child of /data/VMs")
    for parent in [path, *path.parents]:
        if parent.is_symlink():
            raise ValueError("lab root cannot contain symbolic links")
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    stat = path.stat()
    if stat.st_uid != os.geteuid() or stat.st_mode & 0o077:
        raise ValueError("lab root must belong to the operator and be mode 0700")
    return path


def write_json(path, data):
    temporary = path.with_suffix(".partial")
    with temporary.open("x", encoding="utf-8") as output:
        json.dump(data, output, indent=2)
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)


def load(root):
    data = json.loads((root / "lab.json").read_text())
    if data.get("format") != FORMAT or data.get("root") != str(root):
        raise ValueError("not a lab created at this location by this tool")
    return data


@contextlib.contextmanager
def locked(root):
    with (root / "operator.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def kvm():
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        raise ValueError("/dev/kvm unavailable: load kvm_amd and grant this user KVM access")
    # QEMU actually opens KVM and checks VM creation; CPUID flags alone are not proof.
    run("qemu-system-x86_64", "-accel", "kvm", "-machine", "q35", "-cpu", "host",
        "-display", "none", "-nodefaults", "-S", "-qmp", "stdio",
        input=b'{"execute":"qmp_capabilities"}\n{"execute":"quit"}\n',
        stdout=subprocess.DEVNULL, timeout=10)


def unattended(password):
    ns = "urn:schemas-microsoft-com:unattend"
    wcm = "http://schemas.microsoft.com/WMIConfig/2002/State"
    ET.register_namespace("", ns)
    ET.register_namespace("wcm", wcm)
    root = ET.Element(f"{{{ns}}}unattend")

    def node(parent, tag, value=None, **attrs):
        element = ET.SubElement(parent, f"{{{ns}}}{tag}", attrs)
        if value is not None:
            element.text = str(value)
        return element

    def component(settings, name):
        return node(settings, "component", name=name, processorArchitecture="amd64",
                    publicKeyToken="31bf3856ad364e35", language="neutral", versionScope="nonSxS")

    pe = node(root, "settings", **{"pass": "windowsPE"})
    locale = component(pe, "Microsoft-Windows-International-Core-WinPE")
    node(node(locale, "SetupUILanguage"), "UILanguage", "en-US")
    for key, value in [("InputLocale", "0809:00000809"), ("SystemLocale", "en-GB"),
                       ("UILanguage", "en-US"), ("UserLocale", "en-GB")]:
        node(locale, key, value)
    setup = component(pe, "Microsoft-Windows-Setup")
    disk = node(node(setup, "DiskConfiguration"), "Disk", **{f"{{{wcm}}}action": "add"})
    node(disk, "DiskID", 0)
    node(disk, "WillWipeDisk", "true")  # Only the newly created lab qcow2 is attached.
    create = node(disk, "CreatePartitions")
    for order, kind, size in [(1, "EFI", 260), (2, "MSR", 16), (3, "Primary", None)]:
        part = node(create, "CreatePartition", **{f"{{{wcm}}}action": "add"})
        node(part, "Order", order)
        node(part, "Type", kind)
        node(part, "Size" if size else "Extend", size or "true")
    modify = node(disk, "ModifyPartitions")
    for order, (partition, fmt, label) in enumerate(
            [(1, "FAT32", "System"), (3, "NTFS", "Windows")], start=1):
        part = node(modify, "ModifyPartition", **{f"{{{wcm}}}action": "add"})
        node(part, "Order", order)
        node(part, "PartitionID", partition)
        node(part, "Format", fmt)
        node(part, "Label", label)
        if partition == 3:
            node(part, "Letter", "C")
    image = node(node(setup, "ImageInstall"), "OSImage")
    meta = node(node(image, "InstallFrom"), "MetaData", **{f"{{{wcm}}}action": "add"})
    node(meta, "Key", "/IMAGE/NAME")
    node(meta, "Value", "Windows 11 Enterprise Evaluation")
    target = node(image, "InstallTo")
    node(target, "DiskID", 0)
    node(target, "PartitionID", 3)
    user = node(setup, "UserData")
    node(user, "AcceptEula", "true")
    node(user, "FullName", "Memory disposable test lab")
    node(user, "Organization", "Memory Layer")

    specialize = node(root, "settings", **{"pass": "specialize"})
    node(component(specialize, "Microsoft-Windows-Shell-Setup"), "ComputerName", "MEMORY-WINLAB")
    oobe = node(root, "settings", **{"pass": "oobeSystem"})
    # WinPE locale alone does not configure the installed user's OOBE locale.
    locale = component(oobe, "Microsoft-Windows-International-Core")
    for key, value in [("InputLocale", "0809:00000809"), ("SystemLocale", "en-GB"),
                       ("UILanguage", "en-US"), ("UserLocale", "en-GB")]:
        node(locale, key, value)
    shell = component(oobe, "Microsoft-Windows-Shell-Setup")
    node(shell, "TimeZone", "UTC")
    wizard = node(shell, "OOBE")
    for key, value in [("HideEULAPage", "true"), ("HideOnlineAccountScreens", "true"),
                       ("HideWirelessSetupInOOBE", "true"), ("ProtectYourPC", 3)]:
        node(wizard, key, value)
    account = node(node(node(shell, "UserAccounts"), "LocalAccounts"), "LocalAccount",
                   **{f"{{{wcm}}}action": "add"})
    node(account, "Name", "memorylab")
    node(account, "Group", "Administrators")
    cred = node(account, "Password")
    node(cred, "Value", password)
    node(cred, "PlainText", "true")
    auto = node(shell, "AutoLogon")
    node(auto, "Enabled", "true")
    node(auto, "LogonCount", 1)
    node(auto, "Username", "memorylab")
    cred = node(auto, "Password")
    node(cred, "Value", password)
    node(cred, "PlainText", "true")
    command = node(node(shell, "FirstLogonCommands"), "SynchronousCommand",
                   **{f"{{{wcm}}}action": "add"})
    node(command, "Order", 1)
    node(command, "Description", "Bootstrap private Memory lab")
    node(command, "CommandLine", 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '
         '"$d=(Get-Volume | Where-Object FileSystemLabel -eq MEMSEED).DriveLetter; '
         '& ($d + \':\\bootstrap.ps1\')"')
    return ET.tostring(root, encoding="utf-8", xml_declaration=True)


def create(args, root):
    if (root / "lab.json").exists() or (root / "disk.qcow2").exists():
        raise ValueError("lab already exists; never re-create over an existing disk")
    iso = args.iso.resolve(strict=True)
    if len(args.iso_sha256) != 64 or digest(iso) != args.iso_sha256.lower():
        raise ValueError("installation media SHA-256 mismatch")
    source = urlparse(args.media_source)
    if source.scheme != "https" or not source.hostname or not (
            source.hostname == "microsoft.com" or source.hostname.endswith(".microsoft.com")):
        raise ValueError("record the verified official Microsoft source")
    key = root / "ssh-key"
    run("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key)
    seed = root / "seed"
    seed.mkdir(mode=0o700)
    (seed / "Autounattend.xml").write_bytes(unattended(secrets.token_urlsafe(32)))
    shutil.copy2(SCRIPT_DIR / "guest-bootstrap.ps1", seed / "bootstrap.ps1")
    shutil.copy2(key.with_suffix(".pub"), seed / "authorized_keys")
    if args.virtio_iso:
        driver = args.virtio_iso.resolve(strict=True)
        if not args.virtio_sha256 or digest(driver) != args.virtio_sha256.lower():
            raise ValueError("virtio driver ISO SHA-256 mismatch")
    else:
        driver = None
    run("xorriso", "-as", "mkisofs", "-quiet", "-V", "MEMSEED", "-J", "-r",
        "-o", root / "seed.iso", seed)
    run("qemu-img", "create", "-f", "qcow2", root / "disk.qcow2", "200G")
    firmware = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
    shutil.copy2("/usr/share/OVMF/OVMF_VARS_4M.fd", root / "nvram.fd")
    (root / "tpm").mkdir(mode=0o700)
    (root / "snapshots").mkdir(mode=0o700)
    data = {"format": FORMAT, "root": str(root), "iso": str(iso),
            "iso_sha256": args.iso_sha256.lower(), "media_source": args.media_source,
            "virtio_iso": str(driver) if driver else None,
            "virtio_sha256": args.virtio_sha256, "firmware": str(firmware),
            "firmware_sha256": digest(firmware), "ssh_port": args.ssh_port,
            "cpus": 8, "memory_mib": 16384, "created_at": int(time.time())}
    write_json(root / "lab.json", data)
    print(json.dumps(data, indent=2))  # No seed credentials or private SSH key.


def refresh_seed(root):
    """Rebuild uninstalled lab seed from current scripts, preserving its identities."""
    require_stopped(root)
    seed = root / "seed"
    ns = {"u": "urn:schemas-microsoft-com:unattend"}
    original = ET.fromstring((seed / "Autounattend.xml").read_bytes())
    passwords = [node.text for node in original.findall('.//u:Password/u:Value', ns)]
    if len(passwords) != 2 or not passwords[0] or passwords[0] != passwords[1]:
        raise ValueError("existing lab credential is inconsistent; no identity replacement")
    retained = root / f"seed-refresh-{time.time_ns()}"
    retained.mkdir(mode=0o700)
    shutil.copytree(seed, retained / "previous-seed")
    shutil.copy2(root / "seed.iso", retained / "previous-seed.iso")
    staging = retained / "next-seed"
    shutil.copytree(seed, staging)
    (staging / "Autounattend.xml").write_bytes(unattended(passwords[0]))
    shutil.copy2(SCRIPT_DIR / "guest-bootstrap.ps1", staging / "bootstrap.ps1")
    next_iso = retained / "next-seed.iso"
    run("xorriso", "-as", "mkisofs", "-quiet", "-V", "MEMSEED", "-J", "-r",
        "-o", next_iso, staging)
    # The ISO is the sole boot input; retain both versions before switching it.
    for name in ["Autounattend.xml", "bootstrap.ps1"]:
        shutil.copy2(staging / name, seed / name)
    next_iso.replace(root / "seed.iso")
    print(json.dumps({"seed_refreshed": True, "previous_seed": str(retained),
                      "identities_preserved": True, "disk_unchanged": True}))


def qmp(root, execute, arguments=None):
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(15)
        connection.connect(str(root / "qmp.sock"))
        stream = connection.makefile("rwb")
        json.loads(stream.readline())
        for command in ["qmp_capabilities", execute]:
            message = {"execute": command}
            if command == execute and arguments is not None:
                message["arguments"] = arguments
            stream.write(json.dumps(message).encode() + b"\n")
            stream.flush()
            while True:
                response = json.loads(stream.readline())
                if "return" in response or "error" in response:
                    break
            if "error" in response:
                raise ValueError(str(response["error"]))
        return response["return"]


def running(root):
    try:
        return qmp(root, "query-status")
    except (OSError, ValueError):
        return None


def require_stopped(root):
    if running(root):
        raise ValueError("guest must be stopped")
    for name in ["qemu.pid", "swtpm.pid"]:
        pid_file = root / name
        if pid_file.exists():
            pid = int(pid_file.read_text().strip())
            if pid <= 1:
                raise ValueError("invalid lab process identity")
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                continue
            raise ValueError("lab process still exists; wait for clean shutdown")


def start(root, data, install):
    require_stopped(root)
    if (root / "secure-boot-nvram.partial").exists():
        raise ValueError("interrupted firmware update must be reconciled before boot")
    if any(not (path / "complete.json").exists() for path in root.glob("restore-*") if path.is_dir()):
        raise ValueError("interrupted restore must be reconciled with recover-restore before boot")
    kvm()
    for name in ["qmp.sock", "tpm.sock", "guest-agent.sock", "vnc.sock"]:
        (root / name).unlink(missing_ok=True)
    run("swtpm", "socket", "--tpm2", "--tpmstate", f"dir={root / 'tpm'}",
        "--ctrl", f"type=unixio,path={root / 'tpm.sock'},mode=0600,terminate",
        "--flags", "not-need-init", "--daemon", "--pid", f"file={root / 'swtpm.pid'}")
    machine = "q35,smm=on" if data.get("secure_boot") else "q35"
    command = ["qemu-system-x86_64", "-name", "memory-windows-native", "-accel", "kvm",
               "-machine", machine, "-cpu", "host", "-smp", str(data["cpus"]),
               "-m", str(data["memory_mib"]), "-rtc", "base=utc",
               "-drive", f"if=pflash,format=raw,readonly=on,file={data['firmware']}",
               "-drive", f"if=pflash,format=raw,file={root / 'nvram.fd'}",
               "-drive", f"file={root / 'disk.qcow2'},format=qcow2,if=none,id=osdisk",
               "-device", f"ide-hd,drive=osdisk,bus=ide.0,bootindex={2 if install else 1}",
               "-chardev", f"socket,id=chrtpm,path={root / 'tpm.sock'}",
               "-tpmdev", "emulator,id=tpm0,chardev=chrtpm", "-device", "tpm-tis,tpmdev=tpm0",
               "-netdev", f"user,id=net0,net=10.77.0.0/24,hostfwd=tcp:127.0.0.1:{data['ssh_port']}-:22",
               "-device", "virtio-net-pci,netdev=net0", "-device", "virtio-serial-pci",
               "-chardev", f"socket,path={root / 'guest-agent.sock'},server=on,wait=off,id=qga",
               "-device", "virtserialport,chardev=qga,name=org.qemu.guest_agent.0",
               "-qmp", f"unix:{root / 'qmp.sock'},server=on,wait=off", "-display", "none",
               "-vnc", f"unix:{root / 'vnc.sock'}", "-daemonize", "-pidfile", str(root / "qemu.pid"),
               "-D", str(root / "qemu.log")]
    if data.get("secure_boot"):
        command += ["-global", "driver=cfi.pflash01,property=secure,value=on"]
    if install:
        command += ["-drive", f"file={data['iso']},media=cdrom,readonly=on,if=none,id=installiso",
                    "-device", "ide-cd,drive=installiso,bus=ide.1,bootindex=1"]
    command += ["-drive", f"file={root / 'seed.iso'},media=cdrom,readonly=on,if=none,id=seediso",
                "-device", "ide-cd,drive=seediso,bus=ide.2"]
    if data["virtio_iso"]:
        command += ["-drive", f"file={data['virtio_iso']},media=cdrom,readonly=on,if=none,id=driveriso",
                    "-device", "ide-cd,drive=driveriso,bus=ide.3"]
    run(*command)
    print(json.dumps({"status": running(root), "ssh_port": data["ssh_port"],
                      "console": str(root / "vnc.sock"), "acceleration": "kvm"}))


def enable_secure_boot(root, data):
    """Enroll the packaged Microsoft public keys, retaining the previous VM state."""
    require_stopped(root)
    if data.get("secure_boot"):
        print("Secure Boot is already configured")
        return
    firmware = Path("/usr/share/OVMF/OVMF_CODE_4M.secboot.fd")
    variables = Path("/usr/share/OVMF/OVMF_VARS_4M.ms.fd")
    if not firmware.is_file() or not variables.is_file():
        raise ValueError("Microsoft-enrolled OVMF firmware is unavailable")
    name = f"before-secure-boot-{time.time_ns()}"
    snapshot(root, name)
    shutil.copy2(root / "lab.json", root / "snapshots" / name / "lab.json")
    # An interrupted update retains both the original variables and manifest.
    temporary = root / "secure-boot-nvram.partial"
    if temporary.exists():
        raise ValueError("an interrupted firmware update requires inspection")
    shutil.copy2(variables, temporary)
    data = dict(data, firmware=str(firmware), firmware_sha256=digest(firmware),
                secure_boot=True, enrolled_variables_sha256=digest(variables),
                firmware_backup=name)
    write_json(root / "lab.json", data)
    temporary.replace(root / "nvram.fd")
    print(json.dumps({"secure_boot": True, "previous_state": name,
                      "firmware_sha256": data["firmware_sha256"]}))


def ssh(root, data, command, capture=False):
    return run("ssh", "-i", root / "ssh-key", "-p", data["ssh_port"],
               "-o", f"UserKnownHostsFile={root / 'known_hosts'}", "-o", "StrictHostKeyChecking=accept-new",
               "-o", "PasswordAuthentication=no", "-o", "ConnectTimeout=5",
               "memorylab@127.0.0.1", *command, capture_output=capture, timeout=600)


def copy_in(root, data, source, name):
    if source.is_symlink() or not source.is_file():
        raise ValueError("copy source must be a regular non-symlink file")
    if not name or len(name) > 120 or not name[0].isascii() or not name[0].isalnum() or not all(
            c.isascii() and (c.isalnum() or c in "-_.") for c in name):
        raise ValueError("guest input name must be a simple ASCII filename")
    stem = name.split('.', 1)[0].upper()
    if name.endswith('.') or stem in {'CON', 'PRN', 'AUX', 'NUL'} or any(
            stem == f'{prefix}{number}' for prefix in ['COM', 'LPT'] for number in range(1, 10)):
        raise ValueError("guest input name cannot alias a Windows device or trailing dot")
    source = source.resolve(strict=True)
    ssh(root, data, ["New-Item -ItemType Directory -Force C:\\MemoryLab\\inputs | Out-Null"])
    run("scp", "-i", root / "ssh-key", "-P", data["ssh_port"],
        "-o", f"UserKnownHostsFile={root / 'known_hosts'}", "-o", "StrictHostKeyChecking=yes",
        "-o", "PasswordAuthentication=no", "-o", "ConnectTimeout=5",
        source, f"memorylab@127.0.0.1:C:/MemoryLab/inputs/{name}", timeout=600)
    print(json.dumps({"input": name, "bytes": source.stat().st_size, "sha256": digest(source)}))


def snapshot(root, name):
    require_stopped(root)
    if not name or not all(c.isascii() and (c.isalnum() or c == '-') for c in name):
        raise ValueError("invalid snapshot name")
    target = root / "snapshots" / name
    target.mkdir(mode=0o700)  # Immutable; fail if already present.
    for entry in ["disk.qcow2", "nvram.fd", "tpm"]:
        run("cp", "--reflink=auto", "--sparse=always", "-a", root / entry, target / entry)
    if (root / "lab.json").exists():
        shutil.copy2(root / "lab.json", target / "lab.json")
    write_json(target / "snapshot.json", {"format": FORMAT, "name": name, "root": str(root),
                                         "created_at": int(time.time()), "complete": True})
    print(str(target))


def restore(root, name):
    require_stopped(root)
    if not name or not all(c.isascii() and (c.isalnum() or c == '-') for c in name):
        raise ValueError("invalid snapshot name")
    source = root / "snapshots" / name
    receipt = json.loads((source / "snapshot.json").read_text())
    if receipt.get("format") != FORMAT or receipt.get("root") != str(root) or not receipt.get("complete"):
        raise ValueError("incomplete or foreign snapshot")
    # Retain current state in full; interrupted restoration is recoverable locally.
    operation = root / f"restore-{time.time_ns()}"
    operation.mkdir(mode=0o700)
    entries = ["disk.qcow2", "nvram.fd", "tpm"]
    if (source / "lab.json").exists():
        saved = json.loads((source / "lab.json").read_text())
        if saved.get("format") != FORMAT or saved.get("root") != str(root):
            raise ValueError("foreign snapshot firmware configuration")
        entries.append("lab.json")
    write_json(operation / "intent.json", {"format": FORMAT, "snapshot": name,
                                         "root": str(root), "entries": entries})
    for entry in entries:
        run("cp", "--reflink=auto", "--sparse=always", "-a", source / entry, operation / f"new-{entry}")
    write_json(operation / "prepared.json", {"complete_copy": True})
    recover_restore(root, operation.name)


def recover_restore(root, name):
    require_stopped(root)
    if not name.startswith("restore-") or not name.removeprefix("restore-").isdigit():
        raise ValueError("invalid restore operation")
    operation = root / name
    intent = json.loads((operation / "intent.json").read_text())
    if intent.get("format") != FORMAT or intent.get("root") != str(root):
        raise ValueError("foreign restore operation")
    if (operation / "complete.json").exists():
        print(str(operation))
        return
    if not (operation / "prepared.json").exists():
        raise ValueError("copy preparation was interrupted; originals are unchanged; inspect the operation before retry")
    entries = intent.get("entries", ["disk.qcow2", "nvram.fd", "tpm"])
    if entries not in (["disk.qcow2", "nvram.fd", "tpm"],
                       ["disk.qcow2", "nvram.fd", "tpm", "lab.json"]):
        raise ValueError("unknown restore components")
    for entry in entries:
        previous = operation / f"previous-{entry}"
        replacement = operation / f"new-{entry}"
        target = root / entry
        if not previous.exists():
            if not replacement.exists() or not target.exists():
                raise ValueError("restore state requires inspection; no prior state removed")
            target.rename(previous)
        if replacement.exists():
            if target.exists():
                raise ValueError("ambiguous restore target; no state overwritten")
            replacement.rename(target)
        if not target.exists():
            raise ValueError("incomplete restore target")
    write_json(operation / "complete.json", {"restored": intent["snapshot"], "completed_at": int(time.time())})
    print(str(operation))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    sub = parser.add_subparsers(dest="command", required=True)
    create_args = sub.add_parser("create")
    create_args.add_argument("--iso", type=Path, required=True)
    create_args.add_argument("--iso-sha256", required=True)
    create_args.add_argument("--media-source", required=True)
    create_args.add_argument("--virtio-iso", type=Path)
    create_args.add_argument("--virtio-sha256")
    create_args.add_argument("--ssh-port", type=int, choices=range(1024, 65536), default=22290)
    start_args = sub.add_parser("start")
    start_args.add_argument("--install", action="store_true")
    sub.add_parser("status")
    sub.add_parser("shutdown")
    sub.add_parser("enable-secure-boot")
    sub.add_parser("refresh-seed")
    snap = sub.add_parser("snapshot")
    snap.add_argument("name")
    restore_args = sub.add_parser("restore")
    restore_args.add_argument("name")
    recover = sub.add_parser("recover-restore")
    recover.add_argument("name")
    sub.add_parser("boot-key")
    cmd = sub.add_parser("ssh")
    cmd.add_argument("guest_command", nargs=argparse.REMAINDER)
    copy_args = sub.add_parser("copy-in")
    copy_args.add_argument("source", type=Path)
    copy_args.add_argument("--name", required=True)
    args = parser.parse_args()
    os.umask(0o077)
    root = private_root(args.root)
    with locked(root):
        if args.command == "create":
            create(args, root)
            return
        data = load(root)
        if args.command == "start":
            start(root, data, args.install)
        elif args.command == "status":
            print(json.dumps({"qemu": running(root), "lab": data}))
        elif args.command == "shutdown":
            qmp(root, "system_powerdown")
        elif args.command == "enable-secure-boot":
            enable_secure_boot(root, data)
        elif args.command == "refresh-seed":
            refresh_seed(root)
        elif args.command == "snapshot":
            snapshot(root, args.name)
        elif args.command == "restore":
            restore(root, args.name)
        elif args.command == "recover-restore":
            recover_restore(root, args.name)
        elif args.command == "boot-key":
            qmp(root, "send-key", {"keys": [{"type": "qcode", "data": "ret"}]})
        elif args.command == "ssh":
            ssh(root, data, args.guest_command)
        elif args.command == "copy-in":
            copy_in(root, data, args.source, args.name)


if __name__ == "__main__":
    main()
