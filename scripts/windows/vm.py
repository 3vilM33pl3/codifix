#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Disposable native Windows lab. No production mounts, passwords or shell eval.

QEMU/KVM runs as the invoking user. Only SSH is forwarded, on host loopback.
Cold snapshots include the disk, UEFI variables and TPM state together.
Never fall back to software emulation or touch a VM not created by this tool.
"""
import argparse
import base64
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import secrets
import shutil
import socket
import stat
import subprocess
import time
import uuid
from urllib.parse import urlparse
import xml.etree.ElementTree as ET

FORMAT = "memory-windows-lab-v1"
DEFAULT_ROOT = Path("/data/VMs/memory-windows-native/lab")
SCRIPT_DIR = Path(__file__).resolve().parent
QGA_MAX_RESPONSE = 48 * 1024 * 1024  # Two bounded 16 MiB captured streams + base64.
QGA_COPY_LIMIT = 512 * 1024 * 1024
QGA_CHUNK = 256 * 1024
ACTIVE_DISK_FORMAT = "memory-windows-active-disk-v1"
ACTIVE_DISK_BASE = Path("/home/olivier/.local/share/memory-windows-lab")


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
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as output:
        json.dump(data, output, indent=2)
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def load(root):
    data = json.loads((root / "lab.json").read_text())
    if data.get("format") != FORMAT or data.get("root") != str(root):
        raise ValueError("not a lab created at this location by this tool")
    return data


def private_disk_path(path, missing=False):
    """Validate the dedicated active-disk storage, never resolve aliases."""
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts or not path.is_relative_to(ACTIVE_DISK_BASE) or path == ACTIVE_DISK_BASE:
        raise ValueError("active disk must remain inside the dedicated NVMe lab directory")
    if any(not part or any(not char.isascii() or not (char.isalnum() or char in "._-") for char in part)
           for part in path.relative_to(ACTIVE_DISK_BASE).parts):
        raise ValueError("active disk names must not contain QEMU option delimiters or ambiguous characters")
    for parent in [path, *path.parents]:
        if parent.is_symlink():
            raise ValueError("active disk path cannot contain symbolic links")
        if parent.is_relative_to(ACTIVE_DISK_BASE) and parent.exists():
            info = parent.lstat()
            if info.st_uid != os.geteuid() or info.st_mode & 0o077:
                raise ValueError("active disk storage must be operator-owned and private")
    if path.exists():
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise ValueError("active disk must be a regular single-linked file")
    elif not missing:
        raise ValueError("configured active disk is missing; no legacy fallback")
    return path


def disk_information(path):
    result = run("qemu-img", "info", "--output=json", path, capture_output=True, text=True)
    info = json.loads(result.stdout)
    specific = info.get("format-specific", {}).get("data", {})
    if info.get("format") != "qcow2" or info.get("virtual-size") != 200 * 1024 ** 3 or any(
            key in info or key in specific for key in ("backing-filename", "full-backing-filename", "data-file")):
        raise ValueError("active disk must be a standalone 200GiB qcow2 image")
    return {"format": "qcow2", "virtual_size": info["virtual-size"]}


def active_disk(root, missing=False):
    descriptor = root / "active-disk.json"
    if not descriptor.exists() and not descriptor.is_symlink():
        return root / "disk.qcow2"  # Existing labs remain unchanged.
    info = descriptor.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077 or info.st_nlink != 1 or info.st_size > 8192:
        raise ValueError("active disk descriptor must be a private regular file")
    saved = json.loads(descriptor.read_text())
    if saved.get("format") != ACTIVE_DISK_FORMAT or saved.get("root") != str(root) or saved.get("virtual_size") != 200 * 1024 ** 3:
        raise ValueError("foreign or incompatible active disk descriptor")
    path = private_disk_path(saved.get("path", ""), missing)
    if path.exists():
        disk_information(path)
    return path


def relocate_disk(root, directory):
    """Cold exact-copy relocation; original HDD state remains recoverable."""
    require_stopped(root)
    require_no_pending_state(root)
    if (root / "active-disk.json").exists() or (root / "active-disk.json").is_symlink():
        raise ValueError("active disk already configured; no implicit relocation/adoption")
    source = root / "disk.qcow2"
    info = source.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid():
        raise ValueError("legacy disk must be an operator-owned regular single-linked file")
    geometry = disk_information(source)
    destination = Path(directory) / "disk.qcow2"
    private_disk_path(destination, missing=True)
    if not ACTIVE_DISK_BASE.exists():
        ACTIVE_DISK_BASE.mkdir(mode=0o700)
    if destination.parent.exists():
        raise ValueError("relocation requires a new dedicated destination directory")
    destination.parent.mkdir(mode=0o700)
    operation = root / f"relocate-{time.time_ns()}"
    operation.mkdir(mode=0o700)
    for entry in ("nvram.fd", "tpm", "lab.json"):
        run("cp", "--reflink=auto", "--sparse=always", "-a", root / entry, operation / entry)
    recovery_files = [Path("nvram.fd"), Path("lab.json")]
    recovery_files.extend(path.relative_to(root) for path in (root / "tpm").rglob("*") if path.is_file())
    if len(recovery_files) > 128 or any(path.is_symlink() for path in (root / "tpm").rglob("*")):
        raise ValueError("unexpected TPM recovery layout; original remains active")
    recovery_hashes = {}
    for relative in recovery_files:
        original_hash = digest(root / relative)
        if digest(operation / relative) != original_hash:
            raise ValueError("firmware/TPM recovery copy differs; original remains active")
        with (operation / relative).open("rb") as copied:
            os.fsync(copied.fileno())
        recovery_hashes[str(relative)] = original_hash
    before = digest(source)
    write_json(operation / "intent.json", {"format": ACTIVE_DISK_FORMAT, "root": str(root),
               "source": str(source), "destination": str(destination), "source_sha256": before,
               "recovery_sha256": recovery_hashes, "original_retained": True})
    run("cp", "--reflink=auto", "--sparse=always", "-a", source, destination)
    destination.chmod(0o600)
    with destination.open("rb") as copied:
        os.fsync(copied.fileno())
    private_disk_path(destination)
    if digest(source) != before or digest(destination) != before or disk_information(destination) != geometry:
        raise ValueError("relocation copy differs; original remains active and copy is retained")
    descriptor = {"format": ACTIVE_DISK_FORMAT, "root": str(root), "path": str(destination),
                  "virtual_size": geometry["virtual_size"], "initial_sha256": before,
                  "recovery_directory": str(operation), "inactive_original": str(source)}
    write_json(root / "active-disk.json", descriptor)
    write_json(operation / "complete.json", {"activated": str(destination), "original_retained": True})
    print(json.dumps(descriptor))


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


def qga(root, execute, arguments=None):
    """One non-retrying RPC on a newly synchronized private QGA connection."""
    try:
        return _qga_once(root, execute, arguments)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"guest-agent transport failed ({type(error).__name__}); a dispatched command may be uncertain, do not resend automatically") from None


def _qga_once(root, execute, arguments=None):
    target = root / "guest-agent.sock"
    info = target.lstat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise ValueError("guest-agent socket must be operator-owned and private")
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(15)
        connection.connect(str(target))
        with connection.makefile("rwb") as stream:
            nonce = secrets.randbits(63)
            stream.write(b'\xff' + json.dumps({"execute": "guest-sync-delimited", "arguments": {"id": nonce}}).encode() + b'\n')
            stream.flush()
            scanned = 0
            while True:
                line = stream.readline(QGA_MAX_RESPONSE + 1)
                scanned += len(line)
                if not line or scanned > QGA_MAX_RESPONSE:
                    raise ValueError("guest-agent synchronization failed")
                if b'\xff' in line:
                    response = json.loads(line.rsplit(b'\xff', 1)[1])
                    if response.get("return") != nonce:
                        raise ValueError("guest-agent synchronization identity mismatch")
                    break
            message = {"execute": execute}
            if arguments is not None:
                message["arguments"] = arguments
            stream.write(json.dumps(message).encode() + b'\n')
            stream.flush()
            line = stream.readline(QGA_MAX_RESPONSE + 1)
            if not line or len(line) > QGA_MAX_RESPONSE:
                raise ValueError("guest-agent response unavailable/oversized; outcome may be uncertain, do not resend automatically")
            response = json.loads(line)
            if "error" in response:
                # Agent error descriptions can contain paths or command output.
                raise ValueError(f"guest agent rejected {execute}; inspect the operation without blind retry")
            if "return" not in response:
                raise ValueError("invalid guest-agent response")
            return response["return"]


def guest_exec(root, executable, arguments, capture=False):
    path = PureWindowsPath(executable)
    if not path.is_absolute() or len(path.drive) != 2 or path.drive[1] != ':' or path.suffix.lower() != '.exe':
        raise ValueError("guest-exec requires an explicit absolute native executable, not PATH lookup or a script")
    if len(arguments) > 128 or any(not isinstance(arg, str) or '\0' in arg for arg in [executable, *arguments]):
        raise ValueError("invalid guest execution arguments")
    if sum(len(arg.encode('utf-16-le')) // 2 + 3 for arg in [executable, *arguments]) > 32760:
        raise ValueError("guest command exceeds Windows argument limit")
    result = qga(root, "guest-exec", {"path": str(path), "arg": arguments, "capture-output": capture})
    pid = result.get("pid")
    if type(pid) is not int or pid <= 0:
        raise ValueError("invalid guest execution PID; inspect uncertain launch, do not resend")
    return {"pid": pid, "capture_output": capture, "execution_identity": "guest-agent service (normally SYSTEM), not an interactive user"}


def guest_stage_start(root, stage, operation_id, source_root=None):
    """Dispatch one fixed interactive lab stage, without changing machine policy."""
    stages = {"BuildTools", "PostgresPrepare", "PostgresStart", "PdsPrepare", "PdsVerify", "PiBuild", "ProductCheck", "ProductBuild", "NativeDb", "NativeApi", "NativeEval"}
    if stage not in stages:
        raise ValueError("unknown fixed guest stage")
    try:
        operation = uuid.UUID(operation_id)
    except (ValueError, AttributeError, TypeError):
        raise ValueError("canonical nonzero stage operation UUID required") from None
    if operation.int == 0 or str(operation) != operation_id.lower():
        raise ValueError("canonical nonzero stage operation UUID required")
    if source_root is not None:
        if stage not in {"PdsPrepare", "PiBuild", "ProductCheck", "ProductBuild", "NativeDb", "NativeEval"}:
            raise ValueError("SourceRoot applies only to explicit copied-source stages")
        # Reuse the strict native lab-component validator, then bind a fixed
        # source subtree rather than accepting script/command arguments.
        if not source_root.startswith("C:\\MemoryLab\\source") or '/' in source_root or str(PureWindowsPath(source_root)) != source_root:
            raise ValueError("PDS source must be within C:\\MemoryLab\\source")
        suffix = source_root[len("C:\\MemoryLab\\source"):]
        if suffix and not suffix.startswith('\\'):
            raise ValueError("PDS source must be within C:\\MemoryLab\\source")
        guest_input_target("C:\\MemoryLab\\inputs\\source" + suffix)
    elif stage in {"PdsPrepare", "PiBuild", "ProductCheck", "ProductBuild", "NativeDb", "NativeEval"}:
        raise ValueError("Copied-source stages require an explicit source directory")
    operation_id = str(operation)
    run_level = "Limited" if stage in {"NativeApi", "PiBuild", "ProductCheck", "ProductBuild", "NativeDb", "PdsVerify", "NativeEval"} else "Highest"
    runner = r"C:\MemoryLab\inputs\guest-stage.ps1"
    runner_hash = digest(SCRIPT_DIR / "guest-stage.ps1")
    log = rf"C:\MemoryLab\stage-start-{operation_id}.log"
    task = f"MemoryLab-Start-{operation_id}"
    guard = ("if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') { throw 'Disposable lab required' }; "
             f"$runner='{runner}'; if ((Get-FileHash -LiteralPath $runner -Algorithm SHA256).Hash.ToLowerInvariant() -cne '{runner_hash}') "
             "{ throw 'Guest stage runner differs from host source; no execution' }; ")
    command = f"& $runner -Action Start -Stage {stage} -OperationId '{operation_id}'"
    if source_root is not None:
        command += f" -SourceRoot '{source_root}'"
    inner = ("$ErrorActionPreference='Stop'; try { " + guard + command + f" *> '{log}'"
             " } catch { $_ | Out-String | Add-Content -LiteralPath '" + log + "'; exit 1 }")
    encoded = base64.b64encode(inner.encode('utf-16-le')).decode('ascii')
    outer = ("$ErrorActionPreference='Stop'; " + guard +
             f"$task='{task}'; $log='{log}'; "
             "if (Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue) { throw 'Launch task exists; inspect status rather than replaying' }; "
             "foreach ($path in @('C:\\MemoryLab','C:\\MemoryLab\\inputs')) { "
             "if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse lab path denied' } }; "
             "$file=[IO.File]::Open($log,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None); $file.Dispose(); "
             "$sid=([Security.Principal.NTAccount]::new('MEMORY-WINLAB\\memorylab')).Translate([Security.Principal.SecurityIdentifier]); "
             "$acl=[Security.AccessControl.FileSecurity]::new(); $acl.SetAccessRuleProtection($true,$false); "
             "$acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')); "
             "foreach($id in @($sid.Value,'S-1-5-18','S-1-5-32-544')) { "
             "$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($id),'FullControl','Allow')) }; "
             "Set-Acl -LiteralPath $log -AclObject $acl; "
             f"$encoded='{encoded}'; $arguments='-NoProfile -ExecutionPolicy Bypass -EncodedCommand '+$encoded; "
             "$action=New-ScheduledTaskAction -Execute 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe' -Argument $arguments; "
             f"$principal=New-ScheduledTaskPrincipal -UserId 'MEMORY-WINLAB\\memorylab' -LogonType Interactive -RunLevel {run_level}; "
             "$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 5) -MultipleInstances IgnoreNew; "
             "Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings | Out-Null; "
             "Start-ScheduledTask -TaskName $task")
    arguments = ["-NoProfile", "-ExecutionPolicy", "Bypass", "-EncodedCommand",
                 base64.b64encode(outer.encode('utf-16-le')).decode('ascii')]
    dispatched = guest_exec(root, r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe", arguments, capture=True)
    return {"pid": dispatched["pid"], "status": "dispatch_requested", "stage": stage,
            "operation_id": operation_id, "task_name": task, "log": log,
            "runner_sha256": runner_hash, "stage_execution_identity": rf"MEMORY-WINLAB\memorylab (Interactive, {run_level})",
            "machine_execution_policy_changed": False}


def status_summary(result, show_output=False):
    if not isinstance(result, dict) or type(result.get("exited")) is not bool:
        raise ValueError("invalid guest execution status")
    summary = {key: result[key] for key in ["exited", "exitcode", "signal", "out-truncated", "err-truncated"] if key in result}
    for wire, label in [("out-data", "stdout"), ("err-data", "stderr")]:
        if wire not in result:
            continue
        raw = base64.b64decode(result[wire], validate=True)
        if len(raw) > 16 * 1024 * 1024:
            raise ValueError("oversized guest output")
        summary[label] = {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
        if show_output:
            # The caller prints JSON, never raw control codes or shell output.
            summary[label]["text"] = raw.decode('utf-8', errors='replace')
    return summary


def guest_status(root, pid, show_output=False, saved_status=None):
    if pid <= 0:
        raise ValueError("positive guest-exec PID required")
    if saved_status is not None:
        name = Path(saved_status).name
        if name != saved_status or not name.startswith(f"qga-status-{pid}-") or not name.endswith('.json'):
            raise ValueError("saved status must be this PID's lab receipt filename")
        path = root / name
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077 or info.st_size > QGA_MAX_RESPONSE:
            raise ValueError("saved status must be a bounded private operator-owned receipt")
        result = json.loads(path.read_bytes())
    else:
        result = qga(root, "guest-exec-status", {"pid": pid})
        if result.get("exited"):
            # QGA consumes a completed result. Preserve it privately so an
            # initial metadata-only read does not lose later requested output.
            name = f"qga-status-{pid}-{secrets.token_hex(8)}.json"
            path = root / name
            with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), 'w', encoding='utf-8') as output:
                json.dump(result, output)
                output.flush()
                os.fsync(output.fileno())
    summary = status_summary(result, show_output)
    if result.get("exited"):
        summary["saved_status"] = name
    return summary


def guest_input_target(value):
    if '/' in value or '\0' in value:
        raise ValueError("guest input requires an unambiguous Windows lab path")
    path = PureWindowsPath(value)
    if len(path.parts) < 4 or tuple(part.casefold() for part in path.parts[:3]) != ('c:\\', 'memorylab', 'inputs'):
        raise ValueError("guest-copy-in is restricted to C:\\MemoryLab\\inputs\\...")
    for part in path.parts[3:]:
        stem = part.split('.', 1)[0].upper()
        if not part or len(part) > 120 or not part[0].isascii() or not part[0].isalnum() or part.endswith(('.', ' ')) or not all(c.isascii() and (c.isalnum() or c in '-_. ') for c in part):
            raise ValueError("unsafe guest input component")
        if stem in {'CON', 'PRN', 'AUX', 'NUL'} or any(stem == f'{prefix}{number}' for prefix in ['COM', 'LPT'] for number in range(1, 10)):
            raise ValueError("guest input cannot name a Windows device")
    if len(str(path)) > 240:
        raise ValueError("guest input path too long")
    return str(path)


def guest_copy_in(root, source, target):
    target = guest_input_target(target)
    # O_NOFOLLOW + one retained source handle: no source re-open between hash
    # and upload. A changed input is rejected before reporting a complete copy.
    with os.fdopen(os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb') as input_file:
        before = os.fstat(input_file.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > QGA_COPY_LIMIT:
            raise ValueError("source must be a regular file of at most 512 MiB")
        initial = hashlib.sha256(); hashed = 0
        while chunk := input_file.read(QGA_CHUNK):
            hashed += len(chunk)
            if hashed > QGA_COPY_LIMIT:
                raise ValueError("source grew beyond copy limit")
            initial.update(chunk)
        if hashed != before.st_size:
            raise ValueError("source changed before copy")
        expected = initial.hexdigest()
        input_file.seek(0)
        # QGA Windows file-open does not offer CREATE_NEW. Use a fixed,
        # path-validated preparation command, then open EXISTING via r+b.
        # No user script/source contents are inserted into this command.
        script = "$ErrorActionPreference='Stop'; $p='" + target + "'; $d=[IO.Path]::GetDirectoryName($p); " \
            "if (-not (Test-Path -LiteralPath 'C:\\MemoryLab' -PathType Container)) { throw 'Missing lab root' }; " \
            "$parts=$d.Substring(3).Split('\\'); $cur='C:\\'; foreach($part in $parts) { $cur=[IO.Path]::Combine($cur,$part); " \
            "if (-not (Test-Path -LiteralPath $cur)) { [IO.Directory]::CreateDirectory($cur) | Out-Null }; " \
            "if ((Get-Item -LiteralPath $cur -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse path denied' } }; " \
            "$f=[IO.File]::Open($p,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None); $f.Dispose()"
        started = guest_exec(root, r'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], capture=False)
        deadline = time.monotonic() + 30
        while True:
            prepared = qga(root, 'guest-exec-status', {'pid': started['pid']})
            if prepared.get('exited'):
                if prepared.get('exitcode') != 0:
                    raise ValueError("guest destination preparation failed; existing files are never overwritten")
                break
            if time.monotonic() >= deadline:
                raise ValueError("destination preparation uncertain; inspect before retry")
            time.sleep(0.1)
        handle = qga(root, 'guest-file-open', {'path': target, 'mode': 'r+b'})
        if type(handle) is not int or handle < 0:
            raise ValueError("invalid guest file handle; inspect retained input")
        total = 0
        try:
            while chunk := input_file.read(QGA_CHUNK):
                if total + len(chunk) > before.st_size:
                    raise ValueError("source grew during copy; partial input retained")
                result = qga(root, 'guest-file-write', {'handle': handle, 'buf-b64': base64.b64encode(chunk).decode('ascii'), 'count': len(chunk)})
                if type(result.get('count')) is not int or result.get('count') != len(chunk):
                    raise ValueError("incomplete guest write; partial input retained, no automatic retry")
                total += len(chunk)
            qga(root, 'guest-file-flush', {'handle': handle})
        finally:
            qga(root, 'guest-file-close', {'handle': handle})
        after = os.fstat(input_file.fileno())
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns) or total != before.st_size:
            raise ValueError("source changed during copy; partial guest input retained")
        handle = qga(root, 'guest-file-open', {'path': target, 'mode': 'rb'})
        if type(handle) is not int or handle < 0:
            raise ValueError("invalid guest read-back handle")
        actual = hashlib.sha256(); received = 0
        try:
            while True:
                result = qga(root, 'guest-file-read', {'handle': handle, 'count': QGA_CHUNK})
                chunk = base64.b64decode(result.get('buf-b64', ''), validate=True)
                if type(result.get('count')) is not int or type(result.get('eof')) is not bool or result.get('count') != len(chunk) or len(chunk) > QGA_CHUNK or received + len(chunk) > total:
                    raise ValueError("invalid guest read-back; input not verified")
                actual.update(chunk); received += len(chunk)
                if result.get('eof'):
                    break
                if not chunk:
                    raise ValueError("guest read-back made no progress")
        finally:
            qga(root, 'guest-file-close', {'handle': handle})
        if received != total or actual.hexdigest() != expected:
            raise ValueError("guest read-back digest mismatch; retain input for inspection")
        return {"target": target, "bytes": total, "sha256": expected, "read_back_verified": True, "overwrite": False}


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


def require_no_pending_state(root):
    """Do not boot or change disk ownership around incomplete state changes."""
    if (root / "secure-boot-nvram.partial").exists():
        raise ValueError("interrupted firmware update must be reconciled before boot or disk relocation")
    if any(not (path / "complete.json").exists() for path in root.glob("restore-*") if path.is_dir()):
        raise ValueError("interrupted restore must be reconciled with recover-restore before boot or disk relocation")


def start(root, data, install):
    require_stopped(root)
    require_no_pending_state(root)
    disk = active_disk(root)
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
               "-drive", f"file={disk},format=qcow2,if=none,id=osdisk",
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
    # QEMU's daemonization can reset its umask, so a private child umask alone
    # does not guarantee private UNIX sockets. Protect the exact QGA endpoint
    # before reporting startup success; the operator-owned 0700 lab directory
    # prevents another user from substituting it during this bounded wait.
    run(*command, umask=0o077)
    guest_socket = root / "guest-agent.sock"
    deadline = time.monotonic() + 5
    while True:
        try:
            observed = guest_socket.lstat()
        except FileNotFoundError:
            if time.monotonic() >= deadline:
                raise ValueError("QEMU started without its private guest-agent socket; inspect the running lab before retrying") from None
            time.sleep(0.05)
            continue
        if not stat.S_ISSOCK(observed.st_mode) or observed.st_uid != os.geteuid():
            raise ValueError("QEMU guest-agent socket has an unexpected type/owner; inspect the running lab")
        guest_socket.chmod(0o600, follow_symlinks=False)
        verified = guest_socket.lstat()
        if (verified.st_dev, verified.st_ino) != (observed.st_dev, observed.st_ino) or verified.st_mode & 0o077:
            raise ValueError("QEMU guest-agent socket changed while protecting it; inspect the running lab")
        break
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
        selected = active_disk(root) if entry == "disk.qcow2" else root / entry
        run("cp", "--reflink=auto", "--sparse=always", "-a", selected, target / entry)
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
    disk = active_disk(root)
    disk_stage = operation
    if disk != root / "disk.qcow2":
        disk_stage = disk.parent / operation.name
        disk_stage.mkdir(mode=0o700)
    entries = ["disk.qcow2", "nvram.fd", "tpm"]
    if (source / "lab.json").exists():
        saved = json.loads((source / "lab.json").read_text())
        if saved.get("format") != FORMAT or saved.get("root") != str(root):
            raise ValueError("foreign snapshot firmware configuration")
        entries.append("lab.json")
    write_json(operation / "intent.json", {"format": FORMAT, "snapshot": name,
                                         "root": str(root), "entries": entries,
                                         "active_disk": str(disk), "disk_stage": str(disk_stage)})
    for entry in entries:
        staging = disk_stage if entry == "disk.qcow2" else operation
        run("cp", "--reflink=auto", "--sparse=always", "-a", source / entry, staging / f"new-{entry}")
    if disk != root / "disk.qcow2":
        # Legacy snapshots may have inherited a permissive file mode inside
        # their private snapshot directory. The new active storage is stricter.
        (disk_stage / "new-disk.qcow2").chmod(0o600)
        private_disk_path(disk_stage / "new-disk.qcow2")
        disk_information(disk_stage / "new-disk.qcow2")
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
    disk = active_disk(root, missing=True)
    disk_stage = operation if disk == root / "disk.qcow2" else disk.parent / operation.name
    if intent.get("active_disk", str(root / "disk.qcow2")) != str(disk) or intent.get("disk_stage", str(operation)) != str(disk_stage):
        raise ValueError("restore active disk binding changed; inspect retained state")
    for entry in entries:
        staging = disk_stage if entry == "disk.qcow2" else operation
        previous = staging / f"previous-{entry}"
        replacement = staging / f"new-{entry}"
        target = disk if entry == "disk.qcow2" else root / entry
        if entry == "disk.qcow2" and disk != root / "disk.qcow2":
            private_disk_path(previous, missing=True)
            private_disk_path(replacement, missing=True)
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
    relocation = sub.add_parser("relocate-disk", help="cold exact copy to dedicated private NVMe storage; retain original")
    relocation.add_argument("--destination-directory", type=Path, required=True)
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
    sub.add_parser("guest-info", help="QGA version/enabled commands; no SSH required")
    execute_args = sub.add_parser("guest-exec", help="QGA async native executable; no implicit shell")
    execute_args.add_argument("--capture-output", action="store_true")
    execute_args.add_argument("executable")
    execute_args.add_argument("guest_arguments", nargs=argparse.REMAINDER)
    status_args = sub.add_parser("guest-exec-status", help="QGA status; output text requires explicit --show-output")
    status_args.add_argument("pid", type=int)
    status_args.add_argument("--show-output", action="store_true")
    status_args.add_argument("--saved-status", help="private lab receipt filename; does not contact guest")
    guest_copy_args = sub.add_parser("guest-copy-in", help="QGA create-new bounded input with read-back hash")
    guest_copy_args.add_argument("source", type=Path)
    guest_copy_args.add_argument("--target", required=True)
    stage_args = sub.add_parser("guest-stage-start", help="start one hash-bound fixed stage as the interactive lab user")
    stage_args.add_argument("stage", choices=["BuildTools", "PostgresPrepare", "PostgresStart", "PdsPrepare", "PdsVerify", "PiBuild", "ProductCheck", "ProductBuild", "NativeDb", "NativeApi", "NativeEval"])
    stage_args.add_argument("--operation-id", required=True)
    stage_args.add_argument("--source-root")
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
        elif args.command == "relocate-disk":
            relocate_disk(root, args.destination_directory)
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
        elif args.command == "guest-info":
            print(json.dumps(qga(root, "guest-info")))
        elif args.command == "guest-exec":
            arguments = args.guest_arguments
            if arguments[:1] == ['--']:
                arguments = arguments[1:]
            print(json.dumps(guest_exec(root, args.executable, arguments, args.capture_output)))
        elif args.command == "guest-exec-status":
            print(json.dumps(guest_status(root, args.pid, args.show_output, args.saved_status)))
        elif args.command == "guest-copy-in":
            print(json.dumps(guest_copy_in(root, args.source, args.target)))
        elif args.command == "guest-stage-start":
            print(json.dumps(guest_stage_start(root, args.stage, args.operation_id, args.source_root)))


if __name__ == "__main__":
    main()
