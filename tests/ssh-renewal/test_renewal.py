#!/usr/bin/env python3
"""Disposable Step CA, OpenBao KV and real SSH login. All keys and tokens are synthetic."""

import contextlib
import getpass
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "renew", ROOT / "crates/codifix-agency-runtime/ssh/renew.py"
)
renew = importlib.util.module_from_spec(spec)
spec.loader.exec_module(renew)


def run(*args, env=None):
    proc = subprocess.run(
        list(map(str, args)),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        timeout=60,
    )
    if proc.returncode:
        raise RuntimeError(f"{args[0]} {args[1]} failed: {proc.stderr.decode()[:350]}")
    return proc.stdout.decode().strip()


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait(check, attempts=100):
    last = None
    for _ in range(attempts):
        try:
            return check()
        except Exception as exc:
            last = exc
            time.sleep(0.15)
    raise last


class Renewal(unittest.TestCase):
    def test_verified_host_install_and_rollback(self):
        with contextlib.ExitStack() as stack:
            d = Path(stack.enter_context(tempfile.TemporaryDirectory()))
            ca_name = f"codifix-test-step-{uuid4().hex}"
            pwd = secrets.token_hex(24)
            (d / "provisioner").write_text(pwd)
            ca_port = free_port()
            run(
                "docker",
                "run",
                "-d",
                "--rm",
                "--name",
                ca_name,
                "-p",
                f"127.0.0.1:{ca_port}:9000",
                "-e",
                "DOCKER_STEPCA_INIT_NAME=RenewalFixture",
                "-e",
                "DOCKER_STEPCA_INIT_DNS_NAMES=localhost,127.0.0.1",
                "-e",
                "DOCKER_STEPCA_INIT_SSH=true",
                "-e",
                f"DOCKER_STEPCA_INIT_PASSWORD={pwd}",
                "smallstep/step-ca:0.28.3",
            )
            stack.callback(
                subprocess.run,
                ["docker", "rm", "-f", ca_name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            root = d / "root.crt"
            wait(
                lambda: run(
                    "docker", "cp", f"{ca_name}:/home/step/certs/root_ca.crt", root
                )
            )
            run(
                "docker",
                "cp",
                f"{ca_name}:/home/step/certs/ssh_host_ca_key.pub",
                d / "host-ca.pub",
            )
            ca_url = f"https://127.0.0.1:{ca_port}"
            env = dict(os.environ, STEPPATH=str(d / "step"))
            wait(
                lambda: run(
                    "step", "ca", "health", "--ca-url", ca_url, "--root", root, env=env
                )
            )
            for name in ("host", "login", "wrong-login"):
                run("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", d / name)

            def sign(duration, keyfile):
                run(
                    "step",
                    "ssh",
                    "certificate",
                    "monolith.test",
                    keyfile,
                    "--sign",
                    "--host",
                    "--principal",
                    "monolith.test",
                    "--no-agent",
                    "--not-after",
                    duration,
                    "--issuer",
                    "admin",
                    "--ca-url",
                    ca_url,
                    "--root",
                    root,
                    "--provisioner-password-file",
                    d / "provisioner",
                    env=env,
                )

            sign("1h", d / "host.pub")
            port = free_port()
            sshd = d / "sshd_config"
            sshd.write_text(
                f"Port {port}\nListenAddress 127.0.0.1\nPidFile {d}/sshd.pid\nHostKey {d}/host\nHostCertificate {d}/host-cert.pub\nAuthorizedKeysFile {d}/login.pub\nUsePAM no\nPasswordAuthentication no\nStrictModes no\n"
            )
            daemon = subprocess.Popen(
                [shutil.which("sshd"), "-D", "-e", "-f", sshd],
                stderr=stack.enter_context(open(d / "sshd.log", "wb")),
            )
            stack.callback(daemon.wait, 10)
            stack.callback(daemon.terminate)
            bao_port = free_port()
            bao_url = f"http://127.0.0.1:{bao_port}"
            root_token = secrets.token_hex(32)
            bao = subprocess.Popen(
                [
                    "bao",
                    "server",
                    "-dev",
                    f"-dev-root-token-id={root_token}",
                    f"-dev-listen-address=127.0.0.1:{bao_port}",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            stack.callback(bao.wait, 10)
            stack.callback(bao.terminate)
            bao_env = dict(os.environ, BAO_ADDR=bao_url, BAO_TOKEN=root_token)
            wait(lambda: run("bao", "status", env=bao_env))
            for role, cap in (("signer", "create"), ("host", "read")):
                (d / f"{role}.hcl").write_text(
                    f'path "secret/data/renewals/*" {{ capabilities=["{cap}"] }}'
                )
                run("bao", "policy", "write", role, d / f"{role}.hcl", env=bao_env)
                os.environ[f"RENEW_TEST_{role}"] = json.loads(
                    run(
                        "bao",
                        "token",
                        "create",
                        "-format=json",
                        f"-policy={role}",
                        env=bao_env,
                    )
                )["auth"]["client_token"]
                stack.callback(os.environ.pop, f"RENEW_TEST_{role}")
            p = dict(
                host="monolith.test",
                principals=["monolith.test"],
                host_public_key=str(d / "host.pub"),
                ca_public_key=str(d / "host-ca.pub"),
                fingerprint=renew.fingerprint(
                    renew.ser.load_ssh_public_key((d / "host.pub").read_bytes())
                ),
                address="127.0.0.1",
                port=port,
                requester="home",
                operators=["olivier"],
                stage_dir=str(d),
                bao_url=bao_url,
                bao_prefix="secret/data/renewals",
                bao_token_env="RENEW_TEST_signer",
                window_utc=["00:00", "23:59"],
                sshd_config=str(sshd),
                sshd_pidfile=str(d / "sshd.pid"),
                host_certificate=str(d / "host-cert.pub"),
                login_user=getpass.getuser(),
                login_identity=str(d / "login"),
                install_requesters=["home"],
            )
            request = wait(lambda: renew.inspect(p))

            def handoff(req, expiry):
                task = str(uuid4())
                body = dict(operation="review", requester="home", request=req, id=task)
                renew.sign(p, body)
                keyfile = d / f"{task}.pub"
                shutil.copyfile(d / "host.pub", keyfile)
                sign(expiry, keyfile)
                with self.assertRaisesRegex(renew.Failure, "unauthorized_operator"):
                    renew.sign(p, dict(body, operation="handoff", approver="unknown"))
                p["bao_token_env"] = "RENEW_TEST_signer"
                result = renew.sign(
                    p, dict(body, operation="handoff", approver="olivier")
                )
                self.assertEqual(result["renewal_id"], task)
                p["bao_token_env"] = "RENEW_TEST_host"
                return dict(operation="install", requester="home", renewal_id=task)

            with self.assertRaisesRegex(
                renew.Failure, "unauthorized_host_or_principals"
            ):
                renew.validate_request(p, dict(request, principals=["unapproved.test"]))
            with self.assertRaisesRegex(renew.Failure, "unauthorized_requester"):
                renew.sign(
                    p, dict(operation="review", requester="other", request=request)
                )
            first = handoff(request, "2h")
            saved = (d / "host-cert.pub").read_bytes()
            wrong = dict(p, login_identity=str(d / "wrong-login"))
            with self.assertRaisesRegex(renew.Failure, "fresh_login_failed"):
                renew.install_locked(wrong, first)
            self.assertEqual(saved, (d / "host-cert.pub").read_bytes())
            wait(lambda: renew.login(p, renew.served(p)))
            ok = renew.install_locked(p, first)
            self.assertTrue(ok["fresh_login"])
            self.assertEqual(ok["approved_by"], "olivier")
            self.assertGreater(ok["expiry"], request["current_expiry"])
            self.assertEqual(renew.served(p).valid_before, ok["expiry"])
            with self.assertRaisesRegex(renew.Failure, "fresh_login_failed"):
                renew.login(p, renew.certificate(p, saved))
            self.assertEqual(renew.install_locked(p, first), ok)
            with self.assertRaisesRegex(renew.Failure, "outside_maintenance_window"):
                minute = (
                    renew.dt.datetime.now(renew.dt.timezone.utc)
                    + renew.dt.timedelta(minutes=3)
                ).strftime("%H:%M")
                renew.install_locked(dict(p, window_utc=[minute, minute]), first)
            if os.environ.get("RENEW_AGENCY_BINARY"):
                from agency_roundtrip import agency_roundtrip

                agency_roundtrip(d, p, ca_url, root, stack, run, wait, free_port)
            print(
                "VERIFIED Step CA existing host public key; scoped Bao handoff; failed login restored prior certificate; successful fresh trusted login"
            )


if __name__ == "__main__":
    unittest.main()
