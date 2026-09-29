#!/usr/bin/env python3
"""Journaled upgrade of one existing named instance; never downgrade a database."""
import argparse
import ctypes
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import secrets
import stat
import subprocess
import sys
import time
import tomllib
import urllib.request
from urllib.parse import unquote, urlsplit

sys.dont_write_bytecode = True


def bootstrap():
    # The entrypoint is independently authenticated by the operator. Authenticate
    # ownership of sibling Python before importing any of it as root.
    if __name__ != "__main__":
        return
    if os.geteuid() != 0 or not sys.flags.isolated or not sys.flags.no_site:
        raise SystemExit("Use root with python3 -I -S and authenticated root-owned tooling")
    directory = Path(__file__).absolute().parent
    for path in (directory, *directory.parents):
        info = path.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise SystemExit("Installer directory must be root-owned and not writable by others")
    for name in ("upgrade.py", "console.py", "instance.py", "release.py"):
        info = (directory / name).lstat()
        if not (stat.S_ISREG(info.st_mode) and info.st_uid == 0 and info.st_nlink == 1
                and stat.S_IMODE(info.st_mode) == 0o644 and info.st_size <= 1024 * 1024):
            raise SystemExit("Installer modules must be authenticated root-owned regular files")


bootstrap()
SPEC = importlib.util.spec_from_file_location("memory_console", Path(__file__).with_name("console.py"))
console = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(console)
instance, release = console.instance, console.release
require, checksum = release.require, console.checksum
ENV = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "SYSTEMD_PAGER": ""}
PHASES = {"planned", "stopped", "switch_intent", "switched", "start_intent", "complete",
          "rollback_intent", "rolled_back"}


def command(argv, extra_env=None, timeout=30):
    try:
        result = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, env={**ENV, **(extra_env or {})}, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise release.ReleaseError("Bounded host operation failed; inspect the retained journal") from error
    require(result.returncode == 0 and len(result.stdout) <= 65536,
            "Host operation failed; output suppressed to protect credentials")
    return result.stdout


def config_binding(data, name):
    try:
        config = tomllib.loads(data.decode())
        url = urlsplit(config["database"]["url"])
        require(url.scheme in {"postgres", "postgresql"} and url.hostname in {"127.0.0.1", "::1"}
                and url.username and url.password and not url.query and not url.fragment
                and re.fullmatch(r"/[A-Za-z_][A-Za-z0-9_]{0,62}", url.path),
                "Upgrade requires an explicit loopback PostgreSQL database")
        service = config["service"]
        require(service["web_root"] == f"/opt/memory-layer/instances/{name}/release/web",
                "Web root must belong to the selected named instance")
        bind = service["bind_addr"]
        require(isinstance(bind, str) and re.fullmatch(r"127\.0\.0\.1:[0-9]{4,5}", bind)
                and 1024 <= int(bind.split(":")[1]) <= 65535, "Expected a loopback service endpoint")
        return {"PGHOST": url.hostname, "PGPORT": str(url.port or 5432), "PGDATABASE": url.path[1:],
                "PGUSER": unquote(url.username), "PGPASSWORD": unquote(url.password),
                "PGCONNECT_TIMEOUT": "5", "PGOPTIONS": "-c statement_timeout=5000"}, bind
    except (KeyError, ValueError, TypeError, UnicodeError) as error:
        raise release.ReleaseError("Invalid instance configuration; contents suppressed") from error


class Host:
    def unit(self, name):
        return f"memory-layer@{instance.instance_name(name)}.service"

    def inspect(self, name):
        fields = {"Id", "Names", "LoadState", "ActiveState", "SubState", "MainPID", "ControlPID",
                  "Job", "FragmentPath", "DropInPaths", "Transient"}
        output = command(["/usr/bin/systemctl", "show", "--no-pager",
                          "--property=" + ",".join(sorted(fields)), "--", self.unit(name)])
        pairs = [line.split("=", 1) for line in output.decode("ascii").splitlines()]
        require(all(len(pair) == 2 for pair in pairs), "Malformed selected unit state")
        state = dict(pairs)
        require(len(pairs) == len(state) and set(state) == fields, "Incomplete selected unit state")
        require(state["Id"] == self.unit(name) and state["Names"] == self.unit(name)
                and state["LoadState"] == "loaded" and state["Transient"] == "no",
                "Expected one existing concrete named service")
        paths = [state["FragmentPath"], *state["DropInPaths"].split()]
        require(paths[0] in {"/lib/systemd/system/memory-layer@.service",
                             "/usr/lib/systemd/system/memory-layer@.service"}, "Unexpected selected service template")
        bindings = {}
        for value in paths:
            path = Path(value).resolve(strict=True)
            require(str(path).startswith(("/usr/lib/systemd/system/", "/etc/systemd/system/",
                                          "/run/systemd/system/")), "Unexpected systemd configuration path")
            bindings[str(path)] = checksum(console.protected_bytes(path, 65536, mode=0o644))
        require(bindings[str(Path(paths[0]).resolve())] == console.UNIT_SHA256,
                "Service template differs from the reviewed named-instance launcher")
        return state, bindings

    def stopped(self, name):
        state, _ = self.inspect(name)
        require(state["ActiveState"] in {"inactive", "failed"} and state["MainPID"] == "0"
                and state["ControlPID"] == "0" and state["Job"] in {"", "0"},
                "Selected service still has processes or a pending job")

    def stop(self, name):
        command(["/usr/bin/systemctl", "stop", "--", self.unit(name)], timeout=120)
        self.stopped(name)

    def start(self, name):
        command(["/usr/bin/systemctl", "start", "--", self.unit(name)], timeout=120)

    def database(self, config, name, fenced):
        environment, _ = config_binding(config, name)
        # Read-only and explicit connection. Never print or pass a URL/password
        # as argv. The caller must stop all other writers during the cutover.
        sql = ("SELECT COALESCE(MAX(version),0), COUNT(*) FILTER (WHERE NOT success), "
               "(SELECT COUNT(*) FROM pg_stat_activity WHERE datname=current_database() "
               "AND pid<>pg_backend_pid()) FROM _sqlx_migrations;")
        result = command(["/usr/bin/psql", "-X", "-q", "-A", "-t", "-w", "-v", "ON_ERROR_STOP=1",
                          "-c", sql], environment).decode("ascii").strip()
        require(re.fullmatch(r"[0-9]+\|[0-9]+\|[0-9]+", result), "Invalid database schema observation")
        schema, failed, peers = map(int, result.split("|"))
        require(failed == 0, "Database has unsuccessful migrations")
        require(not fenced or peers == 0, "Other database sessions remain; stop all selected-database writers")
        return schema

    def health(self, config, name, version):
        _, bind = config_binding(config, name)
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args, **kwargs):
                return None
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
        for _ in range(30):
            try:
                with opener.open("http://" + bind + "/healthz", timeout=2) as response:
                    body = response.read(16385)
                    require(len(body) <= 16384, "Health response exceeds limit")
                    value = json.loads(body)
                    if (isinstance(value, dict) and value.get("status") == "ok"
                            and value.get("database") == "up" and value.get("version") == version):
                        state, _ = self.inspect(name)
                        require(state["ActiveState"] == "active" and state["MainPID"].isdigit()
                                and int(state["MainPID"]) > 0, "Selected service is not active")
                        return
            except (OSError, ValueError):
                pass
            time.sleep(1)
        raise release.ReleaseError("Candidate health/version not verified; retain candidate and recover forward")


def tree_files(root):
    """Hash concrete root-owned release bytes, rejecting links and extras."""
    observed = {}
    def walk(path, prefix):
        descriptor = console.trusted_directory(path)
        try:
            require(stat.S_IMODE(os.fstat(descriptor).st_mode) == 0o755, "Unexpected installed directory mode")
            names = os.listdir(descriptor)
            require(names, "Empty or unexpected installed directory")
            for name in names:
                relative = prefix + name
                info = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    walk(path / name, relative + "/")
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1
                            and info.st_uid == os.geteuid(), "Linked or untrusted release entry")
                    fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=descriptor)
                    with os.fdopen(fd, "rb") as stream:
                        require(release.inode_identity(os.fstat(stream.fileno())) == release.inode_identity(info),
                                "Release entry changed while reading")
                        observed[relative] = {"sha256": hashlib.file_digest(stream, "sha256").hexdigest(),
                                             "size_bytes": info.st_size, "mode": stat.S_IMODE(info.st_mode)}
        finally:
            os.close(descriptor)
    walk(root, "")
    return observed


def expected_files(report):
    return {key: {**value, "mode": 0o755 if key == "bin/memory" else 0o644}
            for key, value in report["files"].items()}


def atomic_exchange(parent, left, right):
    # One atomic exchange switches the binary and complete Web tree together.
    fd = console.trusted_directory(parent)
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        operation = getattr(libc, "renameat2", None)
        require(operation is not None, "Atomic renameat2 is required")
        operation.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        operation.restype = ctypes.c_int
        if operation(fd, os.fsencode(left), fd, os.fsencode(right), 2) != 0:
            raise OSError(ctypes.get_errno(), "Release exchange failed")
        os.fsync(fd)
    finally:
        os.close(fd)


class Upgrade:
    def __init__(self, layout=None, host=None, fault=lambda _: None):
        self.layout = layout or console.Layout()
        self.host, self.fault = host or Host(), fault
        self.lock = self.fd = None

    def close(self):
        for fd in (self.fd, self.lock):
            if fd is not None:
                os.close(fd)
        self.fd = self.lock = None

    def open(self, name, operation, create=False):
        self.name = instance.instance_name(name)
        require(isinstance(operation, str) and console.IDENTIFIER.fullmatch(operation), "Expected 32-hex operation ID")
        self.operation = operation
        parent = self.layout.root / "var/lib/memory-layer-upgrades" / name
        console.ensure_directory(parent)
        parent_fd = console.trusted_directory(parent, private=True)
        try:
            self.lock = os.open("lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600, dir_fd=parent_fd)
            info = os.fstat(self.lock)
            require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_uid == os.geteuid(),
                    "Invalid instance upgrade lock")
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if create:
                for child in parent.iterdir():
                    if child.name != "lock":
                        require(console.IDENTIFIER.fullmatch(child.name), "Unexpected upgrade journal entry")
                        events = sorted(child.glob("event-*.json"))
                        require(events and console.canonical_read(events[-1])["phase"] in {"complete", "rolled_back"},
                                "An unfinished operation exists for this instance; recover it first")
                os.mkdir(operation, 0o700, dir_fd=parent_fd)
                os.fsync(parent_fd)
            self.root = parent / operation
            self.fd = console.trusted_directory(self.root, private=True)
        finally:
            os.close(parent_fd)
        self.active = self.layout.release / name / "release"
        self.retained = self.active.parent / ("release-upgrade-" + operation)
        self.config_dir = self.layout.config / name

    def events(self):
        events, previous = [], "0" * 64
        paths = sorted(self.root.glob("event-*.json"))
        require(len(paths) < 128, "Upgrade journal limit exceeded")
        for index, path in enumerate(paths):
            value = console.canonical_read(path)
            require(path.name == f"event-{index:04}.json" and set(value) == {"sequence", "previous", "plan", "phase"}
                    and value["sequence"] == index and value["previous"] == previous
                    and value["plan"] == self.plan_digest and value["phase"] in PHASES, "Changed upgrade journal")
            previous = checksum(release.canonical(value))
            events.append(value)
        return events

    def append(self, phase):
        events = self.events()
        value = {"sequence": len(events), "previous": checksum(release.canonical(events[-1])) if events else "0" * 64,
                 "plan": self.plan_digest, "phase": phase}
        console.atomic_new(self.fd, f"event-{len(events):04}.json", release.canonical(value))
        self.fault("journal_" + phase)

    def copy_prepared(self, source, signer, schema):
        report = instance.verify_prepared(source, signer, self.name, schema)
        console.native_header(source, report)
        source = Path(source)
        copied = instance.prepare(argparse.Namespace(manifest=str(source / "manifest.json"),
            signature=str(source / "manifest.sig"), public_key=str(source / "signing-public-key.asc"),
            trusted_signing_fingerprint=signer, architecture=report["architecture"], schema_current=schema,
            artifacts_dir=str(source), staging_parent=str(self.root), instance=self.name,
            release_archive=report["archive"]["name"]))
        console.sync_directories(Path(copied["preparation_directory"]))
        return Path(copied["preparation_directory"]).name, report

    def begin(self, args):
        self.open(args.instance, args.operation, create=True)
        config = console.protected_bytes(self.config_dir / "memory-layer.toml", console.CONFIG_LIMIT)
        require(checksum(config) == args.config_sha256, "Existing configuration differs from reviewed digest")
        config_binding(config, self.name)
        schema = self.host.database(config, self.name, False)
        require(schema == args.schema_current, "Observed database schema differs from reviewed preparation")
        prior_path, prior = self.copy_prepared(args.previous, args.trusted_signing_fingerprint, schema)
        next_path, candidate = self.copy_prepared(args.prepared, args.trusted_signing_fingerprint, schema)
        require(int(candidate["release_version"].rsplit(".", 1)[1]) > int(prior["release_version"].rsplit(".", 1)[1]),
                "Upgrade requires a strictly newer release candidate")
        require(prior["architecture"] == candidate["architecture"], "Architecture changed")
        require(tree_files(self.active) == expected_files(prior), "Installed tree differs from signed previous release")
        digest = console.protected_bytes(self.config_dir / "memory.sha256", 128)
        require(digest == (prior["files"]["bin/memory"]["sha256"] + "\n").encode(), "Previous digest credential differs")
        state, bindings = self.host.inspect(self.name)
        require(state["ActiveState"] in {"active", "inactive", "failed"}, "Selected service is transitioning")
        console.atomic_new(self.fd, "configuration.toml", config)
        console.atomic_new(self.fd, "previous-memory.sha256", digest)
        plan = {"format": "memory-instance-upgrade-v1", "instance": self.name, "operation": self.operation,
                "signer": args.trusted_signing_fingerprint, "schema_before": schema,
                "previous": prior_path, "candidate": next_path, "config_sha256": checksum(config),
                "unit_files": bindings, "initial_state": state["ActiveState"]}
        console.atomic_new(self.fd, "plan.json", release.canonical(plan))
        self.load(args.trusted_signing_fingerprint)
        self.append("planned")
        return self.run("finish")

    def load(self, signer):
        self.plan = console.canonical_read(self.root / "plan.json")
        require(set(self.plan) == {"format", "instance", "operation", "signer", "schema_before", "previous",
                                  "candidate", "config_sha256", "unit_files", "initial_state"}
                and self.plan["format"] == "memory-instance-upgrade-v1" and self.plan["instance"] == self.name
                and self.plan["operation"] == self.operation and self.plan["signer"] == signer,
                "Invalid selected upgrade plan")
        self.plan_digest = checksum(release.canonical(self.plan))
        for key in ("previous", "candidate"):
            require(re.fullmatch(r"memory-release-[a-f0-9]{32}", self.plan[key]), "Invalid authenticated snapshot")
            report = instance.verify_prepared(self.root / self.plan[key], signer, self.name, self.plan["schema_before"])
            console.native_header(self.root / self.plan[key], report)
            setattr(self, key, report)
        self.config = console.protected_bytes(self.root / "configuration.toml", console.CONFIG_LIMIT)
        require(checksum(self.config) == self.plan["config_sha256"], "Changed configuration backup")
        self.guard()
        self.events()

    def guard(self):
        require(console.protected_bytes(self.config_dir / "memory-layer.toml", console.CONFIG_LIMIT) == self.config,
                "Instance configuration changed during upgrade")
        _, bindings = self.host.inspect(self.name)
        require(bindings == self.plan["unit_files"], "Service template or sandbox drop-in changed during upgrade")

    def stage(self):
        # A partial stage from process loss is retained under a random name.
        # Only a complete verified tree is published at the operation's name.
        if self.retained.exists():
            return
        target = self.active.parent / (".upgrade-copy-" + secrets.token_hex(16))
        target.mkdir(mode=0o755)
        for relative, info in expected_files(self.candidate).items():
            path = target / relative
            path.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, "wb") as output, release.open_regular(
                    self.root / self.plan["candidate"] / "release" / relative) as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    output.write(chunk)
                output.flush()
                os.fchmod(output.fileno(), info["mode"])
                os.fsync(output.fileno())
            self.fault("copy_file")
        # mkdir is subject to the caller's private umask; installed trees need
        # directory traversal by DynamicUser, whereas journal snapshots do not.
        for directory, _, _ in os.walk(target, topdown=False):
            Path(directory).chmod(0o755)
        require(tree_files(target) == expected_files(self.candidate), "Candidate staging verification failed")
        console.sync_directories(target)
        parent = console.trusted_directory(self.active.parent)
        try:
            instance.rename_noreplace(parent, target.name, self.retained.name)
            os.fsync(parent)
        finally:
            os.close(parent)
        self.fault("staged")

    def switch(self, forward):
        self.guard()
        self.host.stopped(self.name)
        active, retained = tree_files(self.active), tree_files(self.retained)
        old, new = expected_files(self.previous), expected_files(self.candidate)
        before, after = (old, new) if forward else (new, old)
        require((active == before and retained == after) or (active == after and retained == before),
                "Ambiguous release trees; preserve and inspect")
        if active == before:
            atomic_exchange(self.active.parent, self.active.name, self.retained.name)
            self.fault("exchanged")
        parent = console.trusted_directory(self.config_dir)
        try:
            current = console.protected_bytes(self.config_dir / "memory.sha256", 128)
            allowed = [(report["files"]["bin/memory"]["sha256"] + "\n").encode()
                       for report in (self.previous, self.candidate)]
            require(current in allowed, "Digest credential changed unexpectedly")
            report = self.candidate if forward else self.previous
            temporary = ".upgrade-digest-" + secrets.token_hex(16)
            release.write_new(parent, temporary, (report["files"]["bin/memory"]["sha256"] + "\n").encode())
            os.replace(temporary, "memory.sha256", src_dir_fd=parent, dst_dir_fd=parent)
            os.fsync(parent)
        finally:
            os.close(parent)
        self.fault("digest_replaced")

    def run(self, action):
        events = self.events()
        phase = events[-1]["phase"] if events else "planned"
        self.guard()
        if action == "inspect":
            return self.result("recorded_" + phase)
        if action == "rollback":
            require(not any(event["phase"] in {"start_intent", "complete"} for event in events),
                    "Candidate may have migrated or written data; recover forward, never restore a stale executable")
            if phase != "rolled_back":
                self.host.stop(self.name)
                schema = self.host.database(self.config, self.name, True)
                require(schema == self.plan["schema_before"], "Schema changed; rollback is prohibited")
                self.append("rollback_intent")
                if self.retained.exists():
                    self.switch(False)
                else:
                    require(tree_files(self.active) == expected_files(self.previous), "Previous release changed")
                self.append("rolled_back")
            return self.result("rolled_back_stopped")
        require(phase not in {"rollback_intent", "rolled_back"}, "Rollback cannot be resumed as an upgrade")
        if phase in {"planned", "stopped"}:
            self.stage()
            self.host.stop(self.name)
            self.guard()
            require(self.host.database(self.config, self.name, True) == self.plan["schema_before"],
                    "Schema changed before switch")
            if phase == "planned":
                self.append("stopped")
            self.append("switch_intent")
            phase = "switch_intent"
        if phase == "switch_intent":
            self.host.stopped(self.name)
            require(self.host.database(self.config, self.name, True) == self.plan["schema_before"],
                    "Schema changed during switch")
            self.switch(True)
            self.append("switched")
            phase = "switched"
        if phase in {"switched", "start_intent", "complete"}:
            if phase == "switched":
                self.host.stopped(self.name)
            require(tree_files(self.active) == expected_files(self.candidate), "Candidate bytes changed")
            require(console.protected_bytes(self.config_dir / "memory.sha256", 128) ==
                    (self.candidate["files"]["bin/memory"]["sha256"] + "\n").encode(), "Candidate digest changed")
            schema = self.host.database(self.config, self.name, phase == "switched")
            bounds = self.candidate["schema_compatibility"]
            require(bounds["floor"] <= schema <= bounds["head"], "Database outside candidate signed compatibility")
            if phase == "switched":
                self.append("start_intent")
            # Once this barrier is durable startup may migrate, publish or
            # accept writes. Recovery retries this exact candidate only.
            if phase != "complete":
                self.host.start(self.name)
                self.fault("started")
            self.host.health(self.config, self.name, self.candidate["release_version"])
            require(self.host.database(self.config, self.name, False) == bounds["head"],
                    "Candidate did not reach the signed schema head")
            self.guard()
            if phase != "complete":
                self.append("complete")
        return self.result("complete")

    def result(self, phase):
        return {"format": "memory-instance-upgrade-result-v1", "instance": self.name, "operation": self.operation,
                "status": phase, "release_version": self.candidate["release_version"],
                "manifest_sha256": self.candidate["manifest_sha256"], "config_sha256": self.plan["config_sha256"],
                "journal_directory": str(self.root), "rollback_permitted": not any(
                    event["phase"] in {"start_intent", "complete"} for event in self.events())}


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    for name in ("upgrade", "inspect", "recover"):
        item = commands.add_parser(name)
        for flag in ("instance", "operation", "trusted-signing-fingerprint"):
            item.add_argument("--" + flag, required=True)
        if name == "upgrade":
            for flag in ("prepared", "previous", "config-sha256"):
                item.add_argument("--" + flag, required=True)
            item.add_argument("--schema-current", type=int, required=True)
        if name == "recover":
            item.add_argument("--action", choices=("finish", "rollback"), required=True)
    return root


def main():
    args = parser().parse_args()
    os.environ.clear()
    os.environ.update(ENV)
    os.chdir("/")
    os.umask(0o077)
    upgrade = Upgrade()
    try:
        if args.command == "upgrade":
            result = upgrade.begin(args)
        else:
            upgrade.open(args.instance, args.operation)
            upgrade.load(args.trusted_signing_fingerprint)
            result = upgrade.run("inspect" if args.command == "inspect" else args.action)
        print(json.dumps(result, sort_keys=True))
        return 0
    except (release.ReleaseError, OSError, ValueError, TypeError, KeyError) as error:
        message = str(error) if isinstance(error, release.ReleaseError) else "Upgrade host or journal validation failed"
        print(json.dumps({"status": "rejected", "reason": message}), file=sys.stderr)
        return 1
    finally:
        upgrade.close()


if __name__ == "__main__":
    sys.exit(main())
