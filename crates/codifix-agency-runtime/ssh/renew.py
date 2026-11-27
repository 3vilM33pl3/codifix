"""Host-local certificate operations for codifix-agency. All paths come from an operator profile."""

import base64
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import ssl
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import (
    HTTPRedirectHandler,
    HTTPSHandler,
    ProxyHandler,
    Request,
    build_opener,
)
from uuid import UUID

from cryptography.hazmat.primitives import serialization as ser
from cryptography.hazmat.primitives.serialization import (
    SSHCertificate,
    SSHCertificateType,
)


class Failure(Exception):
    """A bounded, secret-free reason fit for the task history."""


def require(value, reason):
    if not value:
        raise Failure(reason)


def command(args, reason, timeout=15):
    try:
        proc = subprocess.run(
            args,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=timeout,
            check=False,
        )
        require(proc.returncode == 0 and len(proc.stdout) <= 65536, reason)
        return proc.stdout
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise Failure(reason) from exc


def public(key):
    return key.public_bytes(ser.Encoding.OpenSSH, ser.PublicFormat.OpenSSH).decode()


def fingerprint(key):
    blob = base64.b64decode(public(key).split()[1], validate=True)
    return "SHA256:" + base64.b64encode(hashlib.sha256(blob).digest()).decode().rstrip(
        "="
    )


def load_file(path):
    with open(path, "rb") as handle:
        value = handle.read(65537)
    require(0 < len(value) <= 65536, "invalid_public_file")
    return value


def validate_request(p, request):
    require(
        set(request)
        == {"host", "principals", "public_key", "fingerprint", "current_expiry"},
        "invalid_request",
    )
    key = ser.load_ssh_public_key(request["public_key"].encode())
    require(
        request["host"] == p["host"]
        and sorted(request["principals"]) == sorted(p["principals"])
        and p["host"] in p["principals"]
        and len(set(p["principals"])) == len(p["principals"]),
        "unauthorized_host_or_principals",
    )
    require(
        request["fingerprint"] == p["fingerprint"] == fingerprint(key),
        "host_key_mismatch",
    )
    require(
        type(request["current_expiry"]) is int and request["current_expiry"] > 0,
        "invalid_current_expiry",
    )
    return key


def certificate(p, raw, require_current=True):
    try:
        cert = ser.load_ssh_public_identity(raw)
        require(
            isinstance(cert, SSHCertificate) and cert.type == SSHCertificateType.HOST,
            "not_host_certificate",
        )
        cert.verify_cert_signature()
        signer = ser.load_ssh_public_key(load_file(p["ca_public_key"]))
        require(public(cert.signature_key()) == public(signer), "wrong_signer")
        require(fingerprint(cert.public_key()) == p["fingerprint"], "host_key_mismatch")
        require(
            sorted(v.decode() for v in cert.valid_principals) == sorted(p["principals"])
            and not cert.critical_options,
            "wrong_principals",
        )
        if require_current:
            require(
                cert.valid_after <= time.time() < cert.valid_before, "invalid_validity"
            )
        return cert
    except Failure:
        raise
    except Exception as exc:
        raise Failure("invalid_certificate") from exc


def served(p):
    host = p["address"]
    require(not host.startswith("-") and "\n" not in host, "invalid_target")
    key = ser.load_ssh_public_key(load_file(p["host_public_key"]))
    algorithm = public(key).split()[0]
    scan_type = {"ssh-ed25519": "ed25519", "ssh-rsa": "rsa"}.get(algorithm)
    if algorithm.startswith("ecdsa-sha2-"):
        scan_type = "ecdsa"
    require(scan_type, "unsupported_host_key")
    output = command(
        ["ssh-keyscan", "-T", "5", "-c", "-p", str(p["port"]), "-t", scan_type, host],
        "host_certificate_unavailable",
    )
    for line in output.splitlines():
        fields = line.split()
        if len(fields) >= 2 and b"-cert-v01@" in fields[-2]:
            return certificate(p, b" ".join(fields[-2:]), False)
    raise Failure("host_certificate_unavailable")


def inspect(p):
    cert = served(p)
    key = ser.load_ssh_public_key(load_file(p["host_public_key"]))
    req = dict(
        host=p["host"],
        principals=p["principals"],
        public_key=public(key),
        fingerprint=fingerprint(key),
        current_expiry=cert.valid_before,
    )
    validate_request(p, req)
    return req


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args):
        raise Failure("handoff_redirect_rejected")


def handoff(p, task_id, record=None):
    endpoint = p["bao_url"].rstrip("/")
    base = urlsplit(endpoint)
    require(
        base.scheme == "https"
        or (base.scheme == "http" and base.hostname in ("127.0.0.1", "::1")),
        "invalid_handoff_transport",
    )
    require(
        not base.username
        and not base.password
        and not base.query
        and not base.fragment,
        "invalid_handoff_endpoint",
    )
    path = p["bao_prefix"].strip("/")
    require(
        all(re.fullmatch(r"[a-zA-Z0-9_-]+", part) for part in path.split("/")),
        "invalid_handoff_path",
    )
    url = f"{endpoint}/v1/{path}/{UUID(task_id)}"
    context = ssl.create_default_context(cafile=p.get("bao_ca"))
    client = build_opener(ProxyHandler({}), HTTPSHandler(context=context), NoRedirect())
    headers = {
        "X-Vault-Token": os.environ[p["bao_token_env"]],
        "Content-Type": "application/json",
    }
    body = (
        None
        if record is None
        else json.dumps({"data": record, "options": {"cas": 0}}).encode()
    )
    try:
        with client.open(Request(url, data=body, headers=headers), timeout=10) as resp:
            value = resp.read(65537)
            require(len(value) <= 65536, "invalid_handoff")
            return json.loads(value)["data"]["data"] if record is None else None
    except (OSError, HTTPError, ValueError, KeyError) as exc:
        raise Failure("handoff_unavailable_or_already_written") from exc


def sign(p, body):
    require(body["requester"] == p["requester"], "unauthorized_requester")
    req = body["request"]
    validate_request(p, req)
    if body["operation"] == "review":
        return {
            "host": p["host"],
            "fingerprint": req["fingerprint"],
            "principals": req["principals"],
        }
    require(body["approver"] in p["operators"], "unauthorized_operator")
    approved = Path(p["stage_dir"]) / (str(UUID(body["id"])) + "-cert.pub")
    cert = certificate(p, load_file(approved))
    require(cert.valid_before > req["current_expiry"], "expiry_not_extended")
    record = dict(
        request=req,
        approved_by=body["approver"],
        certificate=cert.public_bytes().decode(),
        expiry=cert.valid_before,
        renewal_id=body["id"],
    )
    handoff(p, body["id"], record)
    return {k: record[k] for k in ("approved_by", "expiry", "renewal_id")}


def replace(path, content, stat):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as tmp:
        name = tmp.name
        try:
            tmp.write(content)
            tmp.flush()
            if os.geteuid() == 0:
                os.fchown(tmp.fileno(), stat.st_uid, stat.st_gid)
            os.fchmod(tmp.fileno(), stat.st_mode & 0o777)
            os.fsync(tmp.fileno())
            os.replace(name, path)
        finally:
            if os.path.exists(name):
                os.unlink(name)


def reload(p):
    if p.get("sshd_service"):
        require(
            re.fullmatch(r"[a-zA-Z0-9_.@-]+\.service", p["sshd_service"]),
            "invalid_service_unit",
        )
        command(["systemctl", "reload", p["sshd_service"]], "sshd_reload_failed")
    else:
        pid = int(Path(p["sshd_pidfile"]).read_text().strip())
        require(
            pid > 1 and Path(f"/proc/{pid}/exe").resolve().name == "sshd",
            "wrong_sshd_process",
        )
        os.kill(pid, signal.SIGHUP)


def login(p, cert):
    with (
        tempfile.TemporaryDirectory() as probe_dir,
        tempfile.NamedTemporaryFile(mode="w") as trust,
    ):
        directory = Path(probe_dir)
        expected = cert.public_bytes().split()[1].decode("ascii")
        marker = directory / "presented"
        verify = directory / "verify-presented"
        # OpenSSH supplies the full presented key blob during this connection's
        # HOSTNAME lookup. A fresh login under a different still-valid host
        # certificate must fail even if it has the same CA and principals.
        verify.write_text(
            "#!/bin/sh\n"
            'if [ "$1" = HOSTNAME ]; then\n'
            f"  [ \"$2\" = '{expected}' ] || exit 1\n"
            f"  : > {shlex.quote(str(marker))}\n"
            "fi\n"
        )
        verify.chmod(0o700)
        trust.write(f"@cert-authority {p['host']} {public(cert.signature_key())}\n")
        trust.flush()
        algo = cert.public_bytes().split()[0].decode()
        if algo.startswith("ssh-rsa-"):
            algo = "rsa-sha2-512-cert-v01@openssh.com,rsa-sha2-256-cert-v01@openssh.com"
        options = dict(
            BatchMode="yes",
            ControlMaster="no",
            ControlPath="none",
            StrictHostKeyChecking="yes",
            KnownHostsCommand=f"/bin/sh {shlex.quote(str(verify))} %I %K",
            UpdateHostKeys="no",
            UserKnownHostsFile=trust.name,
            GlobalKnownHostsFile="/dev/null",
            HostKeyAlias=p["host"],
            HostKeyAlgorithms=algo,
            IdentitiesOnly="yes",
            ConnectTimeout="5",
            PasswordAuthentication="no",
            KbdInteractiveAuthentication="no",
        )
        args = [
            "ssh",
            "-F",
            "/dev/null",
            "-p",
            str(p["port"]),
            "-l",
            p["login_user"],
            "-i",
            p["login_identity"],
        ]
        for key, value in options.items():
            args.extend(["-o", f"{key}={value}"])
        command(args + ["--", p["address"], "true"], "fresh_login_failed")
        require(marker.is_file(), "fresh_login_certificate_unverified")


def install_locked(p, body):
    record = handoff(p, body["renewal_id"])
    require(
        record["renewal_id"] == body["renewal_id"]
        and record["approved_by"] in p["operators"],
        "handoff_identity_mismatch",
    )
    validate_request(p, record["request"])
    cert = certificate(p, record["certificate"].encode())
    require(
        cert.valid_before == record["expiry"]
        and cert.valid_before > record["request"]["current_expiry"],
        "expiry_not_extended",
    )
    current = served(p)
    require(
        current.valid_before == record["request"]["current_expiry"]
        or current.public_bytes() == cert.public_bytes(),
        "served_certificate_changed",
    )
    minute = dt.datetime.now(dt.timezone.utc).strftime("%H:%M")
    start, end = p["window_utc"]
    require(
        all(re.fullmatch(r"(?:[01]\d|2[0-3]):[0-5]\d", v) for v in (start, end)),
        "invalid_maintenance_window",
    )
    allowed = (
        start <= minute <= end if start <= end else minute >= start or minute <= end
    )
    require(allowed, "outside_maintenance_window")
    path = Path(p["host_certificate"])
    require(not path.is_symlink() and path.is_file(), "invalid_certificate_destination")
    original = load_file(path)
    require(
        certificate(p, original, False).public_bytes() == current.public_bytes(),
        "disk_and_served_certificate_differ",
    )
    stat = path.stat()
    if current.public_bytes() == cert.public_bytes():
        login(p, cert)
    else:
        replace(path, cert.public_bytes() + b"\n", stat)
        try:
            command(["sshd", "-t", "-f", p["sshd_config"]], "sshd_validation_failed")
            reload(p)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                try:
                    if served(p).public_bytes() == cert.public_bytes():
                        break
                except Failure:
                    pass
                time.sleep(0.15)
            else:
                raise Failure("new_certificate_not_served")
            login(p, cert)
        except Exception:
            replace(path, original, stat)
            try:
                command(
                    ["sshd", "-t", "-f", p["sshd_config"]],
                    "restored_configuration_invalid",
                )
                reload(p)
            except Exception as exc:
                raise Failure("prior_certificate_restored_reload_failed") from exc
            raise
    return dict(
        host=p["host"],
        renewal_id=body["renewal_id"],
        approved_by=record["approved_by"],
        expiry=cert.valid_before,
        fingerprint=p["fingerprint"],
        fresh_login=True,
    )


def main():
    body = json.load(sys.stdin)
    config_path = os.environ["CODIFIX_SSH_RENEWAL_CONFIG"]
    profile = json.loads(load_file(config_path))[body["host"]]
    require(profile["host"] == body["host"], "wrong_host_profile")
    op = body["operation"]
    if op == "inspect":
        return inspect(profile)
    if op in ("review", "handoff"):
        return sign(profile, body)
    if op == "install":
        require(
            body["requester"] in profile["install_requesters"], "unauthorized_requester"
        )
        with open(profile["host_certificate"] + ".codifix-lock", "a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            return install_locked(profile, body)
    raise Failure("unknown_operation")


if __name__ == "__main__":
    try:
        print(json.dumps({"ok": main()}))
    except Failure as exc:
        print(json.dumps({"reason": str(exc)}))
    except Exception:
        print(json.dumps({"reason": "invalid_profile_or_certificate"}))
