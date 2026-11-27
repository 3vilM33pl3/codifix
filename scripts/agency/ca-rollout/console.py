#!/usr/bin/python3 -I
"""Run one enrolled companion or TUI upgrade through a local CA broker."""
from argparse import ArgumentParser
import ctypes
from ctypes import c_char_p, c_int, c_void_p
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import pwd
import re
import socket
import stat
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit
from uuid import UUID

SYSTEMCTL = "/usr/bin/systemctl"
PYTHON = "/usr/bin/python3"
BASE_ENV = {"PATH": "/usr/bin:/bin", "LANG": "C"}
DB_ENV = "CODIFIX_CA_WORKER_DATABASE"
MAX_ARTIFACT = 128 * 1024 * 1024
MAX_RECEIPT = 1024 * 1024
SLUG = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}")
UNIT = re.compile(r"[A-Za-z0-9][A-Za-z0-9@_.-]{0,127}\.service")
ENV_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
DIGEST = re.compile(r"[0-9a-f]{64}")
VERSION = re.compile(r"[A-Za-z0-9.+-]{1,64}")
COMMON = {"unit", "worker_binary", "worker_config", "instance_id", "publisher_peer",
          "publisher_instance_id", "publisher_did", "mandate_id", "specialist_id", "companion_health"}
MODES = {"companion": {"bundle", "openbao_health"},
         "tui": {"installed_tui", "launcher", "operator", "rclone_config", "object_origin"}}


class Stop(Exception):
    """Local configuration or authorization refused."""


def check(ok, reason):
    if not ok:
        raise Stop(reason)


def secure_path(value, directory=False):
    path = Path(value)
    check(path.is_absolute() and ".." not in path.parts, "Expected an absolute protected path")
    for ancestor in reversed(path.parents):
        info = ancestor.lstat()
        check(stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022,
              "Protected paths require root-owned, non-writable ancestors")
    info = path.lstat()
    is_expected = stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)
    check(is_expected and info.st_uid == 0 and not info.st_mode & 0o022,
          "Expected a protected root-owned path")
    return path


def read_protected(value, limit=MAX_RECEIPT, private=False):
    path = secure_path(value)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        info = os.fstat(source.fileno())
        check(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o022,
              "Expected a protected root-owned file")
        check(not private or not info.st_mode & 0o077, "Expected a root-only settings or credential file")
        check(info.st_size <= limit, "Protected file exceeds its size limit")
        data = source.read(limit + 1)
    check(len(data) <= limit, "Protected file exceeds its size limit")
    return data


def validated_url(value, path, *, local=False):
    url = urlsplit(value)
    check(not url.username and not url.password and not url.query and not url.fragment and url.path == path,
          "Unexpected service URL structure")
    if local:
        check(url.scheme == "http" and url.hostname in {"127.0.0.1", "::1"} and url.port,
              "Companion health requires an explicit loopback address and port")
    else:
        check(url.scheme == "https" and url.hostname and not any(c.isspace() for c in value),
              "Expected an explicit HTTPS service URL")
    return url


def validate_settings(settings, mode):
    check(type(settings) is dict and set(settings) == COMMON | MODES[mode]
          and all(isinstance(v, str) and v for v in settings.values()), "Invalid CA settings fields")
    check(UNIT.fullmatch(settings["unit"]), "Invalid companion service unit")
    for name in ("publisher_peer", "specialist_id"):
        check(SLUG.fullmatch(settings[name]), "Invalid CA rollout identity")
    for name in ("instance_id", "publisher_instance_id", "mandate_id"):
        check(str(UUID(settings[name])) == settings[name], "Expected a canonical non-nil UUID")
        check(UUID(settings[name]).int, "Expected a canonical non-nil UUID")
    check(re.fullmatch(r"did:[A-Za-z0-9][A-Za-z0-9.:%_/-]{1,253}", settings["publisher_did"]),
          "Invalid enrolled publisher DID")
    validated_url(settings["companion_health"], "/health", local=True)
    if mode == "companion":
        validated_url(settings["openbao_health"], "/v1/sys/health")
    else:
        validated_url(settings["object_origin"], "/")
        check(SLUG.fullmatch(settings["operator"]), "Invalid TUI operator")
    return settings


def read_settings(path, mode):
    return validate_settings(json.loads(read_protected(path, private=True)), mode)


def command(argv, env=BASE_ENV, timeout=30, **identity):
    result = subprocess.run(argv, env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout, **identity)
    check(result.returncode == 0, f"{Path(argv[0]).name} failed")
    return result.stdout


def running(settings):
    binary = secure_path(settings["worker_binary"])
    worker_config = secure_path(settings["worker_config"])
    unit = settings["unit"]
    pid = int(command([SYSTEMCTL, "show", "-p", "MainPID", "--value", "--", unit]))
    check(pid > 1, "The reviewed companion service is not running")
    proc = Path("/proc") / str(pid)
    argv = (proc / "cmdline").read_bytes().rstrip(b"\0").split(b"\0")
    check(len(argv) == 4 and argv[1:] == [b"--config", os.fsencode(worker_config), b"serve"]
          and (proc / "exe").samefile(binary), "Running service does not match the reviewed worker")
    cfg = json.loads(read_protected(worker_config))
    check(cfg["instance_id"] == settings["instance_id"], "Unexpected running companion instance")
    peer = [peer for peer in cfg["peers"] if peer["id"] == settings["publisher_peer"]]
    check(len(peer) == 1 and not peer[0].get("operator")
          and peer[0]["instance_id"] == settings["publisher_instance_id"]
          and settings["mandate_id"] in peer[0].get("mandates", [])
          and cfg["social"]["peers"][settings["publisher_peer"]]["did"] == settings["publisher_did"],
          "Running companion lacks the reviewed private publisher binding")
    specialists = [item for item in cfg["specialists"] if item["specialist_id"] == settings["specialist_id"]]
    check(len(specialists) == 1 and specialists[0]["instance_id"] == settings["instance_id"]
          and "upgrade_candidate" in specialists[0]["capabilities"], "Companion lacks the reviewed installer specialist")
    fields = dict(line.split(":", 1) for line in (proc / "status").read_text().splitlines() if ":" in line)
    identity = {"user": int(fields["Uid"].split()[1]), "group": int(fields["Gid"].split()[1]),
                "extra_groups": [int(value) for value in fields["Groups"].split()]}
    check(identity["user"] != 0 and identity["group"] != 0 and 0 not in identity["extra_groups"],
          "The companion needs a dedicated unprivileged worker identity")
    live_env = dict(os.fsdecode(value).split("=", 1) for value in (proc / "environ").read_bytes().split(b"\0") if b"=" in value)
    database_env = cfg["database_env"]
    check(ENV_NAME.fullmatch(database_env) and database_env not in BASE_ENV and database_env != "SSL_CERT_FILE"
          and live_env.get(database_env), "Missing reviewed worker database environment")
    env = BASE_ENV | {database_env: live_env[database_env]}
    if "SSL_CERT_FILE" in live_env:
        env["SSL_CERT_FILE"] = str(secure_path(live_env["SSL_CERT_FILE"]))
    check(int(command([SYSTEMCTL, "show", "-p", "MainPID", "--value", "--", unit])) == pid,
          "Companion changed while checking the worker binding")
    return identity, env, live_env[database_env]


def read_sql(dsn, sql):
    """Run the fixed console query as the native worker using read-only libpq."""
    lib = ctypes.CDLL("libpq.so.5")
    for name, return_type, args in (
        ("PQconnectdb", c_void_p, [c_char_p]), ("PQstatus", c_int, [c_void_p]),
        ("PQexec", c_void_p, [c_void_p, c_char_p]),
        ("PQexecParams", c_void_p, [c_void_p, c_char_p, c_int, c_void_p, c_void_p, c_void_p, c_void_p, c_int]), ("PQresultStatus", c_int, [c_void_p]),
        ("PQntuples", c_int, [c_void_p]), ("PQgetvalue", c_char_p, [c_void_p, c_int, c_int]),
        ("PQclear", None, [c_void_p]), ("PQfinish", None, [c_void_p]),
    ):
        fn = getattr(lib, name)
        fn.restype, fn.argtypes = return_type, args
    connection = lib.PQconnectdb(dsn.encode())
    check(connection, "Cannot connect to the companion database")
    try:
        check(lib.PQstatus(connection) == 0, "Cannot connect to the companion database")
        result = lib.PQexec(connection, b"BEGIN READ ONLY; SET LOCAL statement_timeout='5s'")
        try:
            check(result and lib.PQresultStatus(result) == 1, "Cannot start a read-only companion query")
        finally:
            if result:
                lib.PQclear(result)
        result = lib.PQexecParams(connection, sql, 0, None, None, None, None, 0)
        try:
            check(result and lib.PQresultStatus(result) == 2, "Cannot read companion rollout state")
            return [json.loads(lib.PQgetvalue(result, row, 0)) for row in range(lib.PQntuples(result))]
        finally:
            if result:
                lib.PQclear(result)
    finally:
        lib.PQfinish(connection)


class WorkerQuery:
    def __init__(self, identity, env, dsn):
        self.identity = identity
        self.env = env | {DB_ENV: dsn}

    def rows(self, sql):
        result = subprocess.run(
            [PYTHON, "-I", str(Path(__file__).resolve()), "--read-db"],
            input=sql.encode(), capture_output=True, env=self.env, timeout=15, **self.identity,
        )
        check(result.returncode == 0, "Unable to read rollout state as the companion worker")
        return json.loads(result.stdout)


def load_release(bundle):
    directory = secure_path(bundle, directory=True)
    receipt_path = directory / "tested.json"
    receipt = json.loads(read_protected(receipt_path))
    receipt_fields = {"version", "url", "sha256", "artifact_test_sha256", "upgrade_test_sha256"}
    check(type(receipt) is dict and set(receipt) == receipt_fields and VERSION.fullmatch(receipt["version"]),
          "Invalid reviewed release receipt")
    for name in ("sha256", "artifact_test_sha256", "upgrade_test_sha256"):
        check(DIGEST.fullmatch(receipt[name]), "Invalid rollout digest")
    for name in ("artifact_test_sha256", "upgrade_test_sha256"):
        expected = receipt[name]
        check(hashlib.sha256(read_protected(directory / (expected + ".log"))).hexdigest() == expected,
              "Release test evidence does not match the reviewed digest")
    binary = read_protected(directory / "codifix-agency", MAX_ARTIFACT)
    check(hashlib.sha256(binary).hexdigest() == receipt["sha256"], "Staged companion digest differs")
    return receipt, binary


def atomic_install(destination, data):
    path = secure_path(destination)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            stream.write(data)
            stream.flush()
            os.fchmod(stream.fileno(), 0o755)
            os.fsync(stream.fileno())
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def write_for_worker(path, data, group, mode=0o640):
    with path.open("xb") as stream:
        stream.write(data)
        os.fchown(stream.fileno(), 0, group)
        os.fchmod(stream.fileno(), mode)


def module(name):
    path = secure_path(Path(__file__).resolve().with_name(name + ".py"))
    spec = importlib.util.spec_from_file_location(name, path)
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    return helper


def run(configuration, mode):
    check(os.geteuid() == 0, "Run as the local CA administrator")
    secure_path(Path(__file__).absolute())
    settings = read_settings(configuration, mode)
    supervisor = module("tui-supervisor")
    secure_path("/run", directory=True)
    lock_id = hashlib.sha256(settings["unit"].encode()).hexdigest()[:16]
    lock = os.open(f"/run/codifix-ca-rollout-{lock_id}.lock", os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(lock, "wb") as stream:
        info = os.fstat(stream.fileno())
        check(stat.S_ISREG(info.st_mode) and info.st_uid == 0 and not info.st_mode & 0o077,
              "Unexpected rollout lock ownership")
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        supervise(settings, mode, supervisor)


def supervise(settings, mode, supervisor):
    identity, env, dsn = running(settings)
    db = WorkerQuery(identity, env, dsn)
    group = identity["group"]
    with tempfile.TemporaryDirectory(prefix="codifix-ca-rollout-", dir="/run") as directory:
        work = Path(directory)
        os.chown(work, 0, group)
        work.chmod(0o750)
        address = work / "broker.sock"
        stage = work / "artifact"
        profile = work / "profile.json"
        kind = "installation" if mode == "companion" else "tui_installation"
        broker_path = Path(supervisor.__file__).resolve()
        callback = [PYTHON, "-I", str(broker_path), str(address)]
        reviewed_receipt = None
        if mode == "companion":
            reviewed_receipt, binary = load_release(settings["bundle"])
            receipt = work / "tested.json"
            write_for_worker(stage, binary, group)
            write_for_worker(receipt, json.dumps(reviewed_receipt).encode(), group)
            destination = settings["worker_binary"]
            extra = {"release_receipt": str(receipt), "companion_health": settings["companion_health"],
                     "openbao_health": settings["openbao_health"]}
        else:
            stage.mkdir(mode=0o700)
            os.chown(stage, identity["user"], group)
            stage /= "codifix"
            credentials = work / "reader.conf"
            config = read_protected(settings["rclone_config"], private=True)
            supervisor.validate_rclone(config, settings["object_origin"])
            write_for_worker(credentials, config, group)
            destination = secure_path(settings["installed_tui"])
            check(not destination.samefile(settings["worker_binary"]), "TUI destination aliases the companion executable")
            hooks = str(secure_path(Path(__file__).resolve().with_name("tui.py")))
            launcher = str(secure_path(settings["launcher"]))
            operator = pwd.getpwnam(settings["operator"])
            check(operator.pw_uid != 0, "The TUI operator must be unprivileged")
            extra = {"tui": {"object_origin": settings["object_origin"],
                             "fetch_command": [PYTHON, "-I", hooks, "fetch", str(credentials)],
                             "verify_command": callback + ["verify"]}}
        parameters = {"publisher_peer": settings["publisher_peer"], "artifact": str(stage),
                      "installed_artifact": str(destination), "install_command": callback + ["install"], **extra}
        write_for_worker(profile, json.dumps(parameters).encode(), group)

        def install():
            release = supervisor.release_for_effect(db, settings, kind)
            if mode == "companion":
                check(release == reviewed_receipt, "Companion release differs from the locally reviewed receipt")
                data = binary
            else:
                data = supervisor.read_artifact(stage, release["sha256"], identity["user"])
            atomic_install(destination, data)
            if mode == "companion":
                command([SYSTEMCTL, "restart", "--", settings["unit"]], timeout=90)
            return release

        def verify(release):
            check(mode == "tui", "Companion installs cannot invoke the TUI verifier")
            command([PYTHON, "-I", hooks, "verify", launcher, settings["companion_health"], str(destination), release["version"]],
                    env=BASE_ENV | {"HOME": operator.pw_dir, "USER": operator.pw_name},
                    timeout=35, user=operator.pw_uid, group=operator.pw_gid, extra_groups=[])

        with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as listener:
            listener.bind(str(address))
            os.chown(address, 0, group)
            address.chmod(0o660)
            listener.listen(4)
            listener.settimeout(0.2)
            process = subprocess.Popen(
                [settings["worker_binary"], "--config", settings["worker_config"], "upgrade-once", "--profile", profile],
                env=env, stdin=subprocess.DEVNULL, **identity,
            )
            try:
                supervisor.broker(listener, process, identity, install, verify)
                check(process.wait() == 0, "CA upgrade supervision failed")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()


def main(argv=None, mode="companion"):
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("--settings", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        run(args.settings, mode)
    except (Stop, OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        detail = str(error) if isinstance(error, Stop) else type(error).__name__
        print(f"CA rollout stopped: {detail}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--read-db"]:
        check(os.geteuid() != 0, "Run the database inspection as the companion worker")
        print(json.dumps(read_sql(os.environ[DB_ENV], sys.stdin.buffer.read(65536))))
    else:
        raise SystemExit(main())
