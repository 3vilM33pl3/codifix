#!/usr/bin/python3 -I
"""Run the CA TUI supervisor as the worker and broker only the fixed install hooks."""
from pathlib import Path
import hashlib
import json
import os
import re
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time

MAX_ARTIFACT = 128 * 1024 * 1024
PEER_CREDENTIALS = struct.Struct("3i")
CALLBACKS = {b"install", b"verify"}


def read_artifact(path, digest, worker_uid):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != worker_uid or info.st_size > MAX_ARTIFACT:
            raise ValueError("invalid staged TUI executable")
        data = source.read(MAX_ARTIFACT + 1)
    if len(data) > MAX_ARTIFACT or hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("staged TUI checksum mismatch")
    return data


def release_for_effect(db, instance, mandate, specialist):
    # Values come from the root-owned CA binding. UUIDs and the specialist ID
    # are fixed at enrollment, never supplied in a socket message.
    rows = db.rows(
        "SELECT u.release FROM agency_upgrades u JOIN agency_tasks t ON t.id=u.task_id "
        "JOIN agency_mandates m ON m.id=t.mandate_id "
        f"WHERE u.state='installing' AND u.instance_id='{instance}' AND t.mandate_id='{mandate}' "
        "AND t.requester='homelab' AND t.contract->>'status'='working' "
        "AND t.payload#>>'{resource,kind}'='tui_installation' "
        f"AND t.contract->>'specialist_id'='{specialist}' "
        "AND NOT m.paused AND NOT m.revoked "
        "AND (m.document->>'expires_at')::timestamptz > clock_timestamp() "
        "AND (t.payload->>'deadline')::timestamptz > clock_timestamp() "
        "AND (t.execution_lease_until IS NULL OR t.execution_lease_until > clock_timestamp()) "
        "AND EXISTS (SELECT 1 FROM agency_attempts a JOIN external_effects e ON e.id=a.id "
        "WHERE a.task_id=t.id AND e.source='agency-upgrade' AND e.kind='file_replace' "
        f"AND e.actor_id='{specialist}' AND e.resource='tui-upgrade:' || (u.release->>'version') "
        "AND e.status='pending')"
    )
    if len(rows) != 1:
        raise ValueError("no unique authorized CA TUI file replacement")
    release = rows[0]
    if (not re.fullmatch(r"[0-9a-f]{64}", release.get("sha256", "")) or
            not re.fullmatch(r"[A-Za-z0-9.+-]{1,64}", release.get("version", ""))):
        raise ValueError("invalid CA TUI release receipt")
    return release


def peer_callback(connection, supervisor_pid, identity):
    pid, uid, gid = PEER_CREDENTIALS.unpack(
        connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, PEER_CREDENTIALS.size)
    )
    if (uid, gid) != (identity["user"], identity["group"]):
        return None
    try:
        status = (Path("/proc") / str(pid) / "status").read_text().splitlines()
    except (FileNotFoundError, ProcessLookupError):
        return None
    if f"PPid:\t{supervisor_pid}" not in status:
        return None
    action = connection.recv(32)
    return action if action in CALLBACKS else None


def broker(listener, process, identity, install, verify):
    installed = None
    verified = False
    deadline = time.monotonic() + 390
    while process.poll() is None:
        if time.monotonic() >= deadline:
            raise TimeoutError("CA TUI installer timed out")
        try:
            connection, _ = listener.accept()
        except TimeoutError:
            continue
        with connection:
            connection.settimeout(3)
            try:
                action = peer_callback(connection, process.pid, identity)
                if action == b"install" and installed is None:
                    installed = install()
                    connection.sendall(b"ok")
                elif action == b"verify" and installed is not None and not verified:
                    verify(installed)
                    verified = True
                    connection.sendall(b"ok")
            except (ValueError, OSError, subprocess.SubprocessError) as error:
                # Scope failures stay in the unit journal. The task handler
                # records the fixed installation or footer verification step.
                print(f"CA TUI broker callback failed: {type(error).__name__}", file=sys.stderr)


def run(enrollment):
    console = enrollment.console
    cfg, env, identity, _ = console.running()
    dsn = env[cfg["database_env"]]
    supervisor_env = console.BASE_ENV | {cfg["database_env"]:dsn}
    if "SSL_CERT_FILE" in env:
        supervisor_env["SSL_CERT_FILE"] = env["SSL_CERT_FILE"]
    db = console.WorkerQuery(identity, supervisor_env | {"CODIFIX_RC3_DATABASE":dsn})
    group = identity["group"]

    with tempfile.TemporaryDirectory(prefix="codifix-ca-tui-", dir="/run") as directory:
        work = Path(directory)
        os.chown(work, 0, group)
        work.chmod(0o750)
        staging = work / "stage"
        staging.mkdir(mode=0o700)
        os.chown(staging, identity["user"], group)
        staged = staging / "codifix"
        address = work / "broker.sock"
        store = work / "silo-reader.conf"
        profile = work / "profile.json"
        parameters = enrollment.profile()
        verifier = parameters["tui"]["verify_command"]
        parameters["artifact"] = str(staged)
        parameters["tui"]["fetch_command"][-1] = str(store)
        callback = ["/usr/bin/python3", "-I", str(enrollment.ROOT/"tui-supervisor.py"), str(address)]
        parameters["install_command"] = callback + ["install"]
        parameters["tui"]["verify_command"] = callback + ["verify"]
        # The secret is readable only by root and the existing worker group.
        # The entire directory and its sockets disappear after this run.
        for file, content in (
            (store, (enrollment.ROOT/"silo-reader.conf").read_bytes()),
            (profile, console.js(parameters)),
        ):
            file.write_bytes(content)
            os.chown(file, 0, group)
            file.chmod(0o640)

        def install():
            try:
                release = release_for_effect(db, cfg["instance_id"], enrollment.MANDATE, enrollment.SPECIALIST)
                data = read_artifact(staged, release["sha256"], identity["user"])
                console.atomic(enrollment.INSTALLED, data, 0o755)
            except console.Stop as error:
                raise ValueError("CA refused the TUI installation") from error
            return release

        def verify(release):
            try:
                console.command([*verifier, str(enrollment.INSTALLED), release["version"]], timeout=35)
            except console.Stop as error:
                raise ValueError("CA could not verify the installed TUI footer") from error

        with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as listener:
            listener.bind(str(address))
            os.chown(address, 0, group)
            address.chmod(0o660)
            listener.listen(4)
            listener.settimeout(0.2)
            process = subprocess.Popen(
                [console.ACTIVE, "--config", console.CONFIG, "upgrade-once", "--profile", profile],
                env=supervisor_env, stdin=subprocess.DEVNULL, **identity,
            )
            try:
                broker(listener, process, identity, install, verify)
                console.check(process.wait() == 0, "CA TUI supervisor failed under the worker identity")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()


def callback(address, action):
    request = action.encode("ascii")
    if request not in CALLBACKS:
        raise ValueError("unknown CA TUI callback")
    with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as connection:
        connection.settimeout(55)
        connection.connect(address)
        connection.sendall(request)
        if connection.recv(16) != b"ok":
            raise ValueError("CA TUI broker denied the callback")


if __name__ == "__main__":
    # The final arguments appended by the Rust supervisor are intentionally
    # ignored. The root broker chooses the staged and installed paths itself.
    callback(sys.argv[1], sys.argv[2])
