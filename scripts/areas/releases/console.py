#!/usr/bin/env python3
"""Root-only inactive first installation. Never execute artifacts or mutate services."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import secrets
import stat
import subprocess
import sys
import tomllib
from urllib.parse import urlsplit

sys.dont_write_bytecode = True


def privileged_bootstrap():
    """Check sibling code BEFORE importing it. The entry script itself must
    already have been independently authenticated by the console operator.
    """
    if __name__ != "__main__" or os.geteuid() != 0:
        return
    try:
        if not (sys.flags.isolated and sys.flags.no_site):
            raise ValueError()
        directory = Path(__file__).absolute().parent
        descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
        try:
            for part in directory.parts[1:]:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
                os.close(descriptor)
                descriptor = child
                metadata = os.fstat(descriptor)
                if metadata.st_uid != 0 or (metadata.st_mode & 0o022 and not metadata.st_mode & stat.S_ISVTX):
                    raise ValueError()
            if os.fstat(descriptor).st_mode & 0o022:
                raise ValueError()
            for name in ("console.py", "instance.py", "release.py"):
                file_fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptor)
                try:
                    metadata = os.fstat(file_fd)
                    if not (stat.S_ISREG(metadata.st_mode) and metadata.st_uid == 0
                            and metadata.st_nlink == 1 and stat.S_IMODE(metadata.st_mode) == 0o644
                            and metadata.st_size <= 1024 * 1024):
                        raise ValueError()
                finally:
                    os.close(file_fd)
        finally:
            os.close(descriptor)
    except (OSError, ValueError):
        print('{"status":"rejected","activation_authorized":false,"reason":"Use isolated Python and independently authenticated root-owned installer modules"}', file=sys.stderr)
        raise SystemExit(1)


privileged_bootstrap()
SPEC = importlib.util.spec_from_file_location("memory_instance", Path(__file__).with_name("instance.py"))
instance = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(instance)
release = instance.release
require = release.require
IDENTIFIER = re.compile(r"[a-f0-9]{32}\Z")
CONFIG_LIMIT = 65536
MAX_EVENTS = 128
MAX_GENERATIONS = 5
UNIT_SHA256 = "418e629337592d207f11115226395015c11aa01223841692771b9e12f4db1dca"
PHASES = {"planned", "copying", "ready", "release_intent", "release_done",
          "config_intent", "installed", "retain_intent", "retained"}


def checksum(data):
    return hashlib.sha256(data).hexdigest()


def trusted_directory(path, private=False):
    """No symlink component, untrusted owner or writable ancestor. Sticky root
    temporary parents are safe only above the independently checked private leaf.
    The CLI requires root; ordinary-user ownership is supported for unit tests.
    """
    _, parts = release.absolute_parts(path)
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in parts:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
            metadata = os.fstat(descriptor)
            require(metadata.st_uid in {0, os.geteuid()}, "Untrusted directory owner")
            writable = metadata.st_mode & 0o022
            require(not writable or (metadata.st_uid == 0 and metadata.st_mode & stat.S_ISVTX),
                    "Writable installation ancestor")
        metadata = os.fstat(descriptor)
        require(metadata.st_uid == os.geteuid() and not metadata.st_mode & 0o022,
                "Unsafe selected directory")
        if private:
            require(stat.S_IMODE(metadata.st_mode) == 0o700, "Private directory required")
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def ensure_directory(path, mode=0o700):
    if not path.exists():
        ensure_directory(path.parent, 0o755)
        parent = trusted_directory(path.parent)
        try:
            os.mkdir(path.name, mode, dir_fd=parent)
            child = os.open(path.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
            try:
                os.fchmod(child, mode)
                os.fsync(child)
            finally:
                os.close(child)
            os.fsync(parent)
        finally:
            os.close(parent)
    os.close(trusted_directory(path, private=mode == 0o700))


def protected_bytes(path, maximum, mode=0o600):
    parent = trusted_directory(path.parent)
    try:
        descriptor = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
    finally:
        os.close(parent)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_uid == os.geteuid()
                and metadata.st_nlink == 1 and stat.S_IMODE(metadata.st_mode) == mode
                and metadata.st_size <= maximum, "Unsafe protected input")
        value = stream.read(maximum + 1)
        require(len(value) <= maximum, "Protected input exceeds bound")
        return value


def atomic_new(directory, name, contents):
    """Publish only a complete fsynced file. Interrupted temp files are retained."""
    temporary = ".pending-" + secrets.token_hex(16)
    release.write_new(directory, temporary, contents)
    instance.rename_noreplace(directory, temporary, name)
    os.fsync(directory)


def canonical_read(path, limit=65536):
    data = protected_bytes(path, limit)
    try:
        value = json.loads(data, object_pairs_hook=release.unique_object)
    except (ValueError, UnicodeError) as error:
        raise release.ReleaseError("Invalid journal metadata") from error
    require(release.canonical(value) == data, "Noncanonical journal metadata")
    return value


def sync_directories(path):
    # The signed snapshot contains no links; check again before traversal.
    descriptor = trusted_directory(path)
    try:
        for name in os.listdir(descriptor):
            metadata = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
            require(stat.S_ISREG(metadata.st_mode) or stat.S_ISDIR(metadata.st_mode),
                    "Unexpected snapshot entry")
            if stat.S_ISDIR(metadata.st_mode):
                sync_directories(path / name)
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def staging_config(data, name, database):
    """A narrow closed configuration, not an arbitrary Memory TOML validator."""
    try:
        value = tomllib.loads(data.decode("utf-8"))
        service = value["service"]
        require(set(service) == {"bind_addr", "api_token", "read_only", "web_root"},
                "Unexpected staging service settings")
        require(service["read_only"] is True and
                service["web_root"] == f"/opt/memory-layer/instances/{name}/release/web",
                "Staging must be read-only and use the selected release Web root")
        require(isinstance(service["bind_addr"], str) and
                re.fullmatch(r"127\.0\.0\.1:([1-9][0-9]{3,4})", service["bind_addr"]),
                "Staging needs an explicit unprivileged loopback port")
        require(1024 <= int(service["bind_addr"].rsplit(":", 1)[1]) <= 65535, "Invalid staging port")
        token = service["api_token"]
        require(isinstance(token, str) and 32 <= len(token) <= 512 and token.isascii()
                and not any(c.isspace() for c in token) and "REVIEW" not in token,
                "Explicit independent API credential required")
        require(set(value["database"]) == {"url"}, "Unexpected database settings")
        url = urlsplit(value["database"]["url"])
        require(url.scheme == "postgresql" and url.hostname in {"127.0.0.1", "::1"}
                and url.port is not None and 1 <= url.port <= 65535
                and url.username and url.password and url.path == "/" + database
                and not url.query and not url.fragment and "REVIEW" not in url.geturl(),
                "Explicit reviewed loopback database binding required")
        disabled = {
            "cluster": {"enabled": False}, "offline": {"enabled": False},
            "reinforcement": {"enabled": False}, "provenance": {"reverify_enabled": False},
            "mcp": {"enabled": False, "http_enabled": False, "require_token": True, "read_only": True},
            "features": {"llm_curation": False},
            "background_work": {"enabled": False, "internal_automation": False},
            "automation": {"enabled": False}, "embeddings": {"enabled": False, "create_enabled": False},
            "consolidation": {"enabled": False}, "procedural": {"enabled": False},
            "telemetry": {"enabled": False},
        }
        require(set(value) == {"service", "database", *disabled}, "Unexpected staging configuration section")
        for section, expected in disabled.items():
            require(value[section] == expected and all(type(v) is bool for v in value[section].values()),
                    "Automation and implicit disclosure must remain disabled")
    except (ValueError, TypeError, KeyError, UnicodeError) as error:
        # TOML parser diagnostics can include configuration/credential contents.
        raise release.ReleaseError("Invalid closed staging configuration") from error


def native_header(prepared, report):
    architecture = {"aarch64": "arm64", "x86_64": "amd64"}.get(platform.machine())
    require(architecture == report["architecture"], "Release architecture does not match this host")
    with release.open_regular(Path(prepared) / "release/bin/memory") as stream:
        header = stream.read(64)
    require(len(header) == 64 and header[:6] == b"\x7fELF\x02\x01"
            and header[18:20] == (b"\xb7\x00" if architecture == "arm64" else b"\x3e\x00"),
            "Signed executable does not have the expected native ELF header")


def check_systemd(name):
    unit = f"memory-layer@{instance.instance_name(name)}.service"
    fields = {"LoadState", "ActiveState", "SubState", "MainPID", "ControlPID", "Job",
              "Id", "Names", "FragmentPath", "DropInPaths", "Transient", "UnitFileState"}
    try:
        result = subprocess.run(["/usr/bin/systemctl", "show", "--no-pager",
            "--property=" + ",".join(sorted(fields)), "--", unit],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={"PATH": "/usr/bin:/bin", "LC_ALL": "C", "SYSTEMD_PAGER": ""}, timeout=15)
        require(len(result.stdout) <= 8192 and not result.stderr, "Cannot establish inactive systemd state")
        pairs = [line.split("=", 1) for line in result.stdout.decode("ascii").splitlines()]
        require(all(len(pair) == 2 for pair in pairs), "Invalid systemd state")
        values = dict(pairs)
        require(len(pairs) == len(values) and set(values) == fields, "Incomplete systemd state")
        require(result.returncode == 0 or (result.returncode == 1 and values["LoadState"] == "not-found"),
                "Systemd query failed")
        require(values["LoadState"] in {"loaded", "not-found"} and values["ActiveState"] == "inactive"
                and values["SubState"] == "dead" and values["MainPID"] == "0"
                and values["ControlPID"] == "0" and values["Job"] in {"", "0"}
                and values["Id"] == unit and values["Names"] == unit
                and values["Transient"] == "no" and not values["DropInPaths"],
                "Selected instance must be inactive with no processes or pending job")
        if values["LoadState"] == "not-found":
            require(not values["FragmentPath"] and not values["UnitFileState"], "Unexpected missing-unit state")
        else:
            require(values["UnitFileState"] == "disabled" and values["FragmentPath"] in {
                "/lib/systemd/system/memory-layer@.service", "/usr/lib/systemd/system/memory-layer@.service"},
                "First install requires the disabled reviewed template without overrides")
            # /lib may be Debian's root-owned usr-merge symlink. Resolve only
            # this fixed, allowlisted system file, not caller-controlled paths.
            fragment = Path(values["FragmentPath"]).resolve(strict=True)
            require(str(fragment) in {"/lib/systemd/system/memory-layer@.service",
                                      "/usr/lib/systemd/system/memory-layer@.service"},
                    "Unexpected systemd template location")
            require(checksum(protected_bytes(fragment, 16384, mode=0o644)) == UNIT_SHA256,
                    "Installed template differs from the reviewed activation-gated unit")
    except (OSError, UnicodeError, subprocess.TimeoutExpired) as error:
        raise release.ReleaseError("Cannot establish inactive systemd state") from error


class Layout:
    def __init__(self, root=Path("/")):
        # Library injection is for disposable tests. No corresponding CLI flag,
        # environment override or executable hook exists.
        self.root = root
        self.release = root / "opt/memory-layer/instances"
        # A singleton's service-owned directories must never be ancestors of
        # root-trusted configuration or installer state. Named service state
        # lives under <name>/service in three separate systemd-managed roots.
        self.config = root / "etc/memory-layer-instances"
        self.journal = root / "var/lib/memory-layer-installations"
        self.previous_state = [root / location for location in (
            "var/lib/memory-layer-instances", "var/cache/memory-layer-instances", "run/memory-layer-instances")]
        self.legacy = [root / location for location in (
            "etc/memory-layer/instances", "var/lib/memory-layer/instances",
            "var/cache/memory-layer/instances", "run/memory-layer/instances",
            "var/lib/memory-layer/installations")]


def path_absent(path):
    """Inspect only the fixed path, without adopting or following legacy links.

    Legacy ancestors may legitimately be service-owned; they are never trusted
    or modified. Inaccessible or symlinked ancestors fail closed, not as absent.
    """
    _, parts = release.absolute_parts(path)
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        os.stat(parts[-1], dir_fd=descriptor, follow_symlinks=False)
        return False
    except FileNotFoundError:
        return True
    finally:
        os.close(descriptor)


class Console:
    def __init__(self, layout=None, unit_check=check_systemd, fault=lambda _: None):
        self.layout = layout or Layout()
        self.unit_check = unit_check
        self.fault = fault
        self.handles = []

    def close(self):
        for descriptor in reversed(self.handles):
            os.close(descriptor)
        self.handles.clear()

    def open(self, name, create=False):
        self.name = instance.instance_name(name)
        self.check_absent_state()
        if create:
            for parent in (self.layout.release, self.layout.config, self.layout.journal):
                ensure_directory(parent, 0o700 if parent == self.layout.journal else 0o755)
            self.root = self.layout.journal / name
            try:
                os.mkdir(self.root, 0o700)
                parent = trusted_directory(self.layout.journal, private=True)
                os.fsync(parent)
                os.close(parent)
            except FileExistsError as error:
                raise release.ReleaseError("Existing installer journal requires recovery or manual inspection") from error
        else:
            self.root = self.layout.journal / name
        self.root_fd = trusted_directory(self.root, private=True)
        self.handles.append(self.root_fd)
        lock = os.open("lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600, dir_fd=self.root_fd)
        self.handles.append(lock)
        metadata = os.fstat(lock)
        require(stat.S_ISREG(metadata.st_mode) and metadata.st_uid == os.geteuid()
                and metadata.st_nlink == 1 and stat.S_IMODE(metadata.st_mode) == 0o600, "Unsafe installer lock")
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise release.ReleaseError("Another installer owns this instance") from error
        require(instance.absent(self.root_fd, "plan.json") if create else not instance.absent(self.root_fd, "plan.json"),
                "Use recovery for an existing committed installation; uncommitted inputs require inspection")

    def check_absent_state(self):
        for parent in self.layout.legacy:
            require(path_absent(parent / self.name),
                    "Existing legacy instance or journal requires separate reviewed migration")
        for parent in self.layout.previous_state:
            require(path_absent(parent / self.name),
                    "Existing instance runtime/state is outside first-install scope")

    def guard(self):
        for parent in (self.layout.release, self.layout.config):
            descriptor = trusted_directory(parent)
            os.close(descriptor)
        ancestor = self.layout.release
        while ancestor != self.layout.root:
            require(ancestor.stat().st_mode & 0o001, "Release ancestors must be searchable by DynamicUser")
            ancestor = ancestor.parent
        self.check_absent_state()
        # Existing activation, even on an otherwise stopped service, excludes
        # this first-install-only workflow. Do not remove the marker to bypass it.
        path = self.layout.config / self.name
        if path.exists() or path.is_symlink():
            descriptor = trusted_directory(path, private=True)
            try:
                require(instance.absent(descriptor, "activation-reviewed"), "Previously activated instance is outside scope")
            finally:
                os.close(descriptor)
        self.unit_check(self.name)

    def events(self):
        all_names = os.listdir(self.root_fd)
        require(len(all_names) <= 512 and all(name in {"lock", "plan.json", "configuration.toml"}
                or re.fullmatch(r"(?:\.pending-|memory-release-|trees-)[a-f0-9]{32}(?:\.json)?", name)
                or re.fullmatch(r"event-[0-9]{4}\.json", name) for name in all_names),
                "Unexpected or excessive installer journal entries")
        names = sorted(name for name in all_names if re.fullmatch(r"event-[0-9]{4}\.json", name))
        require(len(names) <= MAX_EVENTS, "Installer event limit reached")
        previous = "0" * 64
        result = []
        for index, name in enumerate(names):
            require(name == f"event-{index:04}.json", "Installer journal has a gap")
            event = canonical_read(self.root / name)
            require(set(event) == {"sequence", "previous", "plan_sha256", "phase", "generation"}
                    and type(event["sequence"]) is int and event["sequence"] == index
                    and event["previous"] == previous and event["plan_sha256"] == self.plan_digest
                    and event["phase"] in PHASES and
                    (event["generation"] is None or (isinstance(event["generation"], str)
                     and IDENTIFIER.fullmatch(event["generation"]))), "Changed installer journal")
            previous = checksum(release.canonical(event))
            result.append(event)
        return result

    def append(self, phase, generation=None):
        events = self.events()
        require(len(events) < MAX_EVENTS, "Installer event limit reached")
        previous = checksum(release.canonical(events[-1])) if events else "0" * 64
        value = {"sequence": len(events), "previous": previous, "plan_sha256": self.plan_digest,
                 "phase": phase, "generation": generation}
        atomic_new(self.root_fd, f"event-{len(events):04}.json", release.canonical(value))
        self.fault("journal_" + phase)

    def load(self, signer):
        self.plan = canonical_read(self.root / "plan.json")
        expected = {"format", "instance", "transaction", "snapshot", "signer", "schema_current_asserted",
                    "manifest_sha256", "config_sha256", "config_size", "database_name", "architecture"}
        require(set(self.plan) == expected and self.plan["format"] == "memory-console-install-v1"
                and self.plan["instance"] == self.name and self.plan["signer"] == signer
                and release.natural(self.plan["schema_current_asserted"])
                and release.natural(self.plan["config_size"]) and self.plan["config_size"] <= CONFIG_LIMIT
                and release.digest(self.plan["config_sha256"])
                and isinstance(self.plan["database_name"], str)
                and re.fullmatch(r"[a-zA-Z_][a-zA-Z0-9_]{0,62}", self.plan["database_name"])
                and IDENTIFIER.fullmatch(self.plan["transaction"])
                and isinstance(self.plan["snapshot"], str)
                and re.fullmatch(r"memory-release-[a-f0-9]{32}", self.plan["snapshot"]), "Invalid installation plan")
        self.plan_digest = checksum(release.canonical(self.plan))
        self.snapshot = self.root / self.plan["snapshot"]
        os.close(trusted_directory(self.snapshot, private=True))
        self.prepared = instance.verify_prepared(self.snapshot, signer, self.name, self.plan["schema_current_asserted"])
        require(self.prepared["manifest_sha256"] == self.plan["manifest_sha256"]
                and self.prepared["architecture"] == self.plan["architecture"]
                and self.prepared["schema_current_asserted"] == self.plan["schema_current_asserted"],
                "Changed release identity or schema assertion")
        self.config = protected_bytes(self.root / "configuration.toml", CONFIG_LIMIT)
        require(checksum(self.config) == self.plan["config_sha256"] and len(self.config) == self.plan["config_size"],
                "Reviewed configuration changed")
        staging_config(self.config, self.name, self.plan["database_name"])
        native_header(self.snapshot, self.prepared)
        self.guard()
        self.events()

    def inventory(self, role):
        marker = release.canonical({"format": "memory-console-install-owner-v1", "plan_sha256": self.plan_digest})
        values = {".memory-install.json": (marker, 0o600)}
        if role == "release":
            result = {"release/" + name: {**value, "mode": 0o755 if name == "bin/memory" else 0o644}
                      for name, value in self.prepared["files"].items()}
        else:
            values.update({"memory-layer.toml": (self.config, 0o600),
                           "memory.sha256": ((self.prepared["files"]["bin/memory"]["sha256"] + "\n").encode(), 0o600)})
            result = {}
        result.update({name: {"sha256": checksum(data), "size_bytes": len(data), "mode": mode}
                       for name, (data, mode) in values.items()})
        return result, values

    def verify(self, path, role):
        expected, _ = self.inventory(role)
        found = {}
        directory_mode = 0o755 if role == "release" else 0o700
        def walk(directory, prefix):
            descriptor = trusted_directory(directory)
            try:
                require(stat.S_IMODE(os.fstat(descriptor).st_mode) == directory_mode, "Changed installation directory mode")
                for name in os.listdir(descriptor):
                    relative = prefix + name
                    metadata = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
                    if stat.S_ISDIR(metadata.st_mode):
                        require(any(p.startswith(relative + "/") for p in expected), "Unexpected installation directory")
                        walk(directory / name, relative + "/")
                    else:
                        require(relative in expected, "Unexpected installation file")
                        value = expected[relative]
                        parent = trusted_directory(directory)
                        try:
                            file_fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
                        finally:
                            os.close(parent)
                        with os.fdopen(file_fd, "rb") as stream:
                            current = os.fstat(stream.fileno())
                            require(stat.S_ISREG(current.st_mode) and current.st_nlink == 1
                                    and current.st_uid == os.geteuid()
                                    and release.inode_identity(metadata) == release.inode_identity(current)
                                    and current.st_size == value["size_bytes"], "Changed installation file binding")
                            found[relative] = {"sha256": hashlib.file_digest(stream, "sha256").hexdigest(),
                                "size_bytes": current.st_size, "mode": stat.S_IMODE(current.st_mode)}
            finally:
                os.close(descriptor)
        walk(path, "")
        require(found == expected, "Installed bytes or permissions differ from authenticated inputs")

    def materialize(self, role, generation):
        parent = getattr(self.layout, role)
        name = self.stage_name(generation)
        parent_fd = trusted_directory(parent)
        try:
            os.mkdir(name, 0o700, dir_fd=parent_fd)
            os.fsync(parent_fd)
        finally:
            os.close(parent_fd)
        path = parent / name
        expected, values = self.inventory(role)
        directories = {path}
        for relative, info in expected.items():
            target = path / relative
            target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            directories.update([target.parent, *[p for p in target.parents if p != path and path in p.parents]])
            fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "wb") as destination:
                if relative in values:
                    destination.write(values[relative][0])
                else:
                    with release.open_regular(self.snapshot / relative) as source:
                        for block in iter(lambda: source.read(1024 * 1024), b""):
                            destination.write(block)
                destination.flush()
                os.fchmod(destination.fileno(), info["mode"])
                os.fsync(destination.fileno())
            self.fault("copy_file")
        for directory in sorted(directories, key=lambda p: len(p.parts), reverse=True):
            fd = trusted_directory(directory)
            os.fchmod(fd, 0o755 if role == "release" else 0o700)
            os.fsync(fd)
            os.close(fd)
        self.verify(path, role)
        self.fault("staged_" + role)

    def stage_name(self, generation):
        require(isinstance(generation, str) and IDENTIFIER.fullmatch(generation), "Invalid stage identity")
        return f".incoming-{self.name}-{generation}"

    def tree_identity(self, role, name, generation):
        evidence = canonical_read(self.root / f"trees-{generation}.json")
        require(set(evidence) == {"plan_sha256", "generation", "release", "config"}
                and evidence["plan_sha256"] == self.plan_digest and evidence["generation"] == generation,
                "Changed stage identity evidence")
        expected = evidence[role]
        require(isinstance(expected, list) and len(expected) == 2
                and all(type(n) is int and n >= 0 for n in expected), "Invalid stage inode identity")
        current = (getattr(self.layout, role) / name).lstat()
        require(stat.S_ISDIR(current.st_mode) and list(release.inode_identity(current)) == expected,
                "Installed directory identity changed")

    def move(self, role, source, destination, generation):
        self.guard()
        parent = getattr(self.layout, role)
        descriptor = trusted_directory(parent)
        try:
            source_exists = not instance.absent(descriptor, source)
            destination_exists = not instance.absent(descriptor, destination)
            require(source_exists != destination_exists, "Ambiguous installation paths; preserve and inspect")
            if source_exists:
                self.tree_identity(role, source, generation)
                self.verify(parent / source, role)
                instance.rename_noreplace(descriptor, source, destination)
                self.fault("renamed_" + role)
                os.fsync(descriptor)
            self.tree_identity(role, destination, generation)
            self.verify(parent / destination, role)
            # Also fsync a reconciled rename whose previous process died before
            # its directory fsync or completion receipt.
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    def run(self, action):
        events = self.events()
        phase = events[-1]["phase"] if events else "planned"
        generation = events[-1]["generation"] if events else None
        if phase == "retained":
            require(action in {"inspect", "retain"}, "A retained first install cannot be restarted")
            for role in ("release", "config"):
                parent = getattr(self.layout, role)
                require(not (parent / self.name).exists() and not (parent / self.name).is_symlink(),
                        "Unexpected instance appeared after retention")
                retained = ".retained-" + self.name + "-" + self.plan["transaction"]
                self.tree_identity(role, retained, generation)
                self.verify(parent / retained, role)
            return self.result("retained_inactive")
        if action == "inspect":
            if phase == "installed":
                for role in ("release", "config"):
                    self.tree_identity(role, self.name, generation)
                    self.verify(getattr(self.layout, role) / self.name, role)
            return self.result(phase)
        require(phase != "retain_intent" or action == "retain", "Retention intent cannot be resumed as installation")
        if phase in {"planned", "copying"}:
            require(action == "finish", "No published installation to retain; incomplete stages are already preserved")
            require(sum(event["phase"] == "copying" for event in events) < MAX_GENERATIONS,
                    "Incomplete stage generation limit reached; preserve and inspect")
            generation = secrets.token_hex(16)
            self.append("copying", generation)
            for role in ("release", "config"):
                parent_fd = trusted_directory(getattr(self.layout, role))
                try:
                    require(instance.absent(parent_fd, self.name), "First installation refuses an existing instance")
                finally:
                    os.close(parent_fd)
                self.materialize(role, generation)
            identity = {"plan_sha256": self.plan_digest, "generation": generation}
            for role in ("release", "config"):
                identity[role] = list(release.inode_identity((getattr(self.layout, role) / self.stage_name(generation)).lstat()))
            atomic_new(self.root_fd, f"trees-{generation}.json", release.canonical(identity))
            self.append("ready", generation)
            phase = "ready"
        if action == "retain":
            # Normalize any ambiguous forward publish first; both complete
            # copies are authenticated and the activation marker is absent.
            if phase != "retain_intent":
                if phase != "installed":
                    self.publish(generation)
                self.append("retain_intent", generation)
            for role in ("release", "config"):
                self.move(role, self.name, ".retained-" + self.name + "-" + self.plan["transaction"], generation)
            self.append("retained", generation)
            return self.result("retained_inactive")
        if phase != "installed":
            self.publish(generation)
        else:
            for role in ("release", "config"):
                self.tree_identity(role, self.name, generation)
                self.verify(getattr(self.layout, role) / self.name, role)
        return self.result("installed_inactive")

    def publish(self, generation):
        self.append("release_intent", generation)
        self.move("release", self.stage_name(generation), self.name, generation)
        self.append("release_done", generation)
        self.append("config_intent", generation)
        self.move("config", self.stage_name(generation), self.name, generation)
        self.append("installed", generation)

    def result(self, status):
        return {"format": "memory-console-install-result-v1", "status": status, "instance": self.name,
                "transaction": self.plan["transaction"], "plan_sha256": self.plan_digest,
                "manifest_sha256": self.plan["manifest_sha256"], "configuration_sha256": self.plan["config_sha256"],
                "activation_authorized": False, "service_mutated": False, "database_contacted": False,
                "database_schema_verified": False, "native_abi_or_sandbox_proven": False,
                "journal_directory": str(self.root)}

    def install(self, args):
        require(re.fullmatch(r"[a-zA-Z_][a-zA-Z0-9_]{0,62}", args.database_name), "Invalid database name")
        require(release.digest(args.config_sha256), "Explicit reviewed configuration SHA-256 required")
        os.close(trusted_directory(Path(args.prepared), private=True))
        report = instance.verify_prepared(args.prepared, args.trusted_signing_fingerprint,
                                           args.instance, args.schema_current)
        require(report["schema_current_asserted"] == args.schema_current,
                "Schema assertion differs from the reviewed preparation")
        native_header(args.prepared, report)
        config = protected_bytes(Path(args.config), CONFIG_LIMIT)
        require(checksum(config) == args.config_sha256, "Configuration differs from reviewed bytes")
        staging_config(config, args.instance, args.database_name)
        self.open(args.instance, create=True)
        self.guard()
        for parent in (self.layout.release, self.layout.config):
            descriptor = trusted_directory(parent)
            try:
                require(instance.absent(descriptor, self.name), "First installation refuses existing instance paths")
            finally:
                os.close(descriptor)
        source = Path(args.prepared)
        copied = instance.prepare(argparse.Namespace(manifest=str(source / "manifest.json"),
            signature=str(source / "manifest.sig"), public_key=str(source / "signing-public-key.asc"),
            trusted_signing_fingerprint=args.trusted_signing_fingerprint, architecture=report["architecture"],
            schema_current=args.schema_current, artifacts_dir=str(source), staging_parent=str(self.root),
            instance=args.instance, release_archive=report["archive"]["name"]))
        sync_directories(Path(copied["preparation_directory"]))
        os.fsync(self.root_fd)
        atomic_new(self.root_fd, "configuration.toml", config)
        plan = {"format": "memory-console-install-v1", "instance": args.instance,
            "transaction": secrets.token_hex(16), "snapshot": Path(copied["preparation_directory"]).name,
            "signer": args.trusted_signing_fingerprint, "schema_current_asserted": args.schema_current,
            "manifest_sha256": report["manifest_sha256"], "config_sha256": args.config_sha256,
            "config_size": len(config), "database_name": args.database_name, "architecture": report["architecture"]}
        atomic_new(self.root_fd, "plan.json", release.canonical(plan))
        self.load(args.trusted_signing_fingerprint)
        self.append("planned")
        return self.run("finish")


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    install = commands.add_parser("install", help="First install only; never activate")
    for flag in ("prepared", "config", "config-sha256", "database-name", "instance", "trusted-signing-fingerprint"):
        install.add_argument("--" + flag, required=True)
    install.add_argument("--schema-current", type=int, required=True)
    for command in ("inspect", "recover"):
        item = commands.add_parser(command)
        item.add_argument("--instance", required=True)
        item.add_argument("--trusted-signing-fingerprint", required=True)
        if command == "recover":
            item.add_argument("--action", choices=("finish", "retain"), required=True)
    return root


def main():
    args = parser().parse_args()
    console = Console()
    try:
        require(os.geteuid() == 0 and sys.flags.isolated and sys.flags.no_site,
                "Use root with python3 -I -S and independently authenticated root-owned tooling")
        os.environ.clear()
        os.environ.update(PATH="/usr/bin:/bin", LC_ALL="C", TMPDIR="/tmp")
        os.chdir("/")
        os.umask(0o077)
        for name in ("console.py", "instance.py", "release.py"):
            path = Path(__file__).absolute().with_name(name)
            protected_bytes(path, 1024 * 1024, mode=0o644)
        if args.command == "install":
            result = console.install(args)
        else:
            console.open(args.instance)
            console.load(args.trusted_signing_fingerprint)
            result = console.run("inspect" if args.command == "inspect" else args.action)
        print(json.dumps(result, sort_keys=True))
        return 0
    except (release.ReleaseError, OSError, ValueError, TypeError, KeyError) as error:
        # Never emit configuration/parser/provider contents or caller paths.
        reason = str(error) if isinstance(error, release.ReleaseError) else "Installation validation or filesystem operation failed"
        print(json.dumps({"status": "rejected", "activation_authorized": False, "reason": reason}), file=sys.stderr)
        return 1
    finally:
        console.close()


if __name__ == "__main__":
    sys.exit(main())
