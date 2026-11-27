#!/usr/bin/python3 -I
"""Run the CA TUI supervisor as the worker and broker only the fixed install hooks."""
from configparser import ConfigParser, Error as ConfigError
from pathlib import Path
import hashlib
import os
import re
import socket
import stat
import struct
import subprocess
import sys
import time
from urllib.parse import urlsplit

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


def validate_rclone(config, origin):
    parser = ConfigParser(interpolation=None)
    try:
        parser.read_string(config.decode())
    except (ConfigError, UnicodeError):
        raise ValueError("invalid private reader configuration") from None
    if parser.sections() != ["release"] or parser.defaults() or parser["release"].get("type") != "s3":
        raise ValueError("expected exactly one reviewed S3 release remote")
    remote = parser["release"]
    configured = urlsplit(remote.get("endpoint", ""))
    reviewed = urlsplit(origin)
    if (configured.scheme != "https" or configured.hostname != reviewed.hostname
            or (configured.port or 443) != (reviewed.port or 443)
            or configured.username is not None or configured.password is not None
            or configured.path not in {"", "/"} or configured.query or configured.fragment
            or not remote.getboolean("force_path_style", fallback=True)
            or remote.getboolean("env_auth", fallback=False)):
        raise ValueError("release remote differs from the reviewed HTTPS origin")


def release_for_effect(db, settings, kind):
    # Every interpolated value is a validated UUID or restricted identifier
    # from root-owned settings, or a constant selected by the local entry point.
    instance = settings["instance_id"]
    mandate = settings["mandate_id"]
    specialist = settings["specialist_id"]
    requester = settings["publisher_peer"]
    prefix = "tui-upgrade:" if kind == "tui_installation" else "upgrade:"
    rows = db.rows(
        "SELECT u.release FROM agency_upgrades u JOIN agency_tasks t ON t.id=u.task_id "
        "JOIN agency_mandates m ON m.id=t.mandate_id "
        f"WHERE u.state='installing' AND u.instance_id='{instance}' AND t.mandate_id='{mandate}' "
        f"AND t.requester='{requester}' AND t.contract->>'status'='working' "
        f"AND t.payload#>>'{{resource,kind}}'='{kind}' AND t.payload#>>'{{resource,id}}'='{instance}' "
        f"AND t.contract->>'specialist_id'='{specialist}' "
        "AND NOT m.paused AND NOT m.revoked "
        "AND (m.document->>'expires_at')::timestamptz > clock_timestamp() "
        "AND (t.payload->>'deadline')::timestamptz > clock_timestamp() "
        "AND (t.execution_lease_until IS NULL OR t.execution_lease_until > clock_timestamp()) "
        "AND EXISTS (SELECT 1 FROM agency_attempts a JOIN external_effects e ON e.id=a.id "
        "WHERE a.task_id=t.id AND e.source='agency-upgrade' AND e.kind='file_replace' "
        f"AND e.host_id='{instance}' AND e.actor_id='{specialist}' "
        f"AND e.resource='{prefix}' || (u.release->>'version') AND e.status='pending')"
    )
    if len(rows) != 1:
        raise ValueError("no unique authorized CA file replacement")
    release = rows[0]
    if (not re.fullmatch(r"[0-9a-f]{64}", release.get("sha256", "")) or
            not re.fullmatch(r"[A-Za-z0-9.+-]{1,64}", release.get("version", ""))):
        raise ValueError("invalid CA release receipt")
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
    attempted = False
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
                if action == b"install" and not attempted:
                    attempted = True
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
