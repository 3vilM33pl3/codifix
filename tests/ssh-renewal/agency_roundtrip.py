"""Exercise the release binary and isolated agency databases against the live fixture."""

import base64
import datetime as dt
import http.server
import json
import os
import secrets
import subprocess
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from uuid import uuid4

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec, utils


def agency_roundtrip(d, p, ca_url, step_root, stack, run, wait, port):
    binary = os.environ["RENEW_AGENCY_BINARY"]
    step_pass = d / "provisioner"
    home, ca = str(uuid4()), str(uuid4())
    home_mandate, ca_mandate = str(uuid4()), str(uuid4())
    home_port, ca_port, proxy_port = port(), port(), port()
    hp, cp = d / "home.json", d / "ca.json"
    tls = d / "tls.pem"
    run(
        "openssl",
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-days",
        "1",
        "-nodes",
        "-subj",
        "/CN=localhost",
        "-addext",
        "subjectAltName=IP:127.0.0.1",
        "-addext",
        "basicConstraints=critical,CA:FALSE",
        "-keyout",
        d / "tls.key",
        "-out",
        tls,
    )

    def spawn(argv, env=None):
        proc = subprocess.Popen(
            argv, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
        stack.callback(proc.wait, 10)
        stack.callback(proc.terminate)

    spawn(
        [
            "socat",
            f"OPENSSL-LISTEN:{proxy_port},bind=127.0.0.1,cert={tls},key={d}/tls.key,verify=0,reuseaddr,fork",
            f"TCP:127.0.0.1:{ca_port}",
        ]
    )

    # The full PDS issuance and denial suite lives under tests/federation-pds.
    # Here a scoped PDS fixture signs real ES256 service JWTs for private XRPC.
    def encoded(b):
        return base64.urlsafe_b64encode(b).decode().rstrip("=")

    signer = ec.generate_private_key(ec.SECP256R1())

    def did_key(key):
        raw = b"\x80\x24" + key.public_key().public_bytes(
            serialization.Encoding.X962, serialization.PublicFormat.CompressedPoint
        )
        alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
        value = int.from_bytes(raw, "big")
        result = ""
        while value:
            value, i = divmod(value, 58)
            result = alphabet[i] + result
        return "did:key:z" + result

    class Pds(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            path = urllib.parse.urlsplit(self.path)
            assert (
                path.path == "/xrpc/com.atproto.server.getServiceAuth"
                and self.headers["Authorization"] == "Bearer fixture"
            )
            q = urllib.parse.parse_qs(path.query)
            claims = dict(
                iss="did:web:home.test",
                aud=q["aud"][0],
                lxm=q["lxm"][0],
                exp=int(time.time()) + 60,
                jti=str(uuid4()),
            )
            data = (
                encoded(b'{"alg":"ES256"}') + "." + encoded(json.dumps(claims).encode())
            )
            r, s = utils.decode_dss_signature(
                signer.sign(data.encode(), ec.ECDSA(hashes.SHA256()))
            )
            order = int(
                "FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551", 16
            )
            signature = r.to_bytes(32) + min(s, order - s).to_bytes(32)
            reply = json.dumps(dict(token=data + "." + encoded(signature))).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(reply)

    fake = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Pds)
    threading.Thread(target=fake.serve_forever, daemon=True).start()
    stack.callback(fake.server_close)
    stack.callback(fake.shutdown)
    pds = f"http://127.0.0.1:{fake.server_port}"
    caller, operator, foreign = [secrets.token_hex(30) for _ in range(3)]

    def peer(id, instance, env, operator=False, mandates=(), endpoint=None):
        return dict(
            id=id,
            instance_id=instance,
            token_env=env,
            operator=operator,
            mandates=list(mandates),
            endpoint=endpoint,
        )

    def configuration(instance, port, role, cap, peers, social, db_env):
        return dict(
            instance_id=instance,
            listen=f"127.0.0.1:{port}",
            public_url="https://agency.example.test",
            database_env=db_env,
            specialists=[
                dict(
                    schema_version=1,
                    instance_id=instance,
                    specialist_id=role,
                    name=role,
                    capabilities=cap,
                )
            ],
            peers=peers,
            social=social,
        )

    home_caps = ["ssh_host_inspect", "ssh_host_install"]
    ca_caps = ["ssh_host_sign"]

    def social(did, peers):
        return dict(
            did=f"did:web:{did}.test",
            pds=pds,
            access_token_env="TEST_PDS_ACCESS",
            peers=peers,
        )

    home_cfg = configuration(
        home,
        home_port,
        "homelab",
        home_caps,
        [
            peer("olivier", home, "OP", True),
            peer("alert", home, "ALERT", mandates=[home_mandate]),
            peer("ca", ca, "", endpoint=f"https://127.0.0.1:{proxy_port}"),
        ],
        social(
            "home",
            {
                "ca": dict(
                    did="did:web:ca.test",
                    signing_key=did_key(ec.generate_private_key(ec.SECP256R1())),
                )
            },
        ),
        "RENEW_HOME_DB",
    )
    ca_cfg = configuration(
        ca,
        ca_port,
        "authority",
        ca_caps,
        [
            peer("olivier", ca, "OP", True),
            peer("other", ca, "OTHER", True),
            peer("home", home, "", mandates=[ca_mandate]),
        ],
        social(
            "ca", {"home": dict(did="did:web:home.test", signing_key=did_key(signer))}
        ),
        "RENEW_CA_DB",
    )
    hp.write_text(json.dumps(home_cfg))
    cp.write_text(json.dumps(ca_cfg))
    coordinator = dict(
        home_requester="alert",
        home_specialist="homelab",
        home_mandate=home_mandate,
        ca_peer="ca",
        ca_specialist="authority",
        ca_mandate=ca_mandate,
        renew_before_seconds=7 * 86400,
        deadline_seconds=1800,
    )
    base_env = dict(
        os.environ,
        OP=operator,
        OTHER=foreign,
        ALERT=caller,
        TEST_PDS_ACCESS="fixture",
        SSL_CERT_FILE=str(tls),
        STEPPATH=str(d / "step"),
    )

    def env(profile, name):
        path = d / f"{name}-profiles.json"
        path.write_text(json.dumps({"monolith.test": profile}))
        return dict(base_env, CODIFIX_SSH_RENEWAL_CONFIG=str(path))

    he = env(
        dict(
            p,
            bao_token_env="RENEW_TEST_host",
            install_requesters=["alert"],
            coordinator=coordinator,
        ),
        "home",
    )
    ce = env(
        dict(
            p,
            bao_token_env="RENEW_TEST_signer",
            step_url=ca_url,
            step_root=str(step_root),
            step_issuer="admin",
            step_duration="4h",
        ),
        "ca",
    )

    def api(port, path, token, body=None):
        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/{path}",
            headers={
                "Authorization": f"Bearer {token}",
                "Content-Type": "application/json",
                "A2A-Version": "1.0",
            },
            data=json.dumps(body).encode() if body is not None else None,
        )
        with urllib.request.urlopen(req, timeout=15) as response:
            return json.load(response)

    for cfg, en, db, instance, grant, special, cap, listener in [
        (hp, he, "RENEW_HOME_DB", home, home_mandate, "homelab", home_caps, home_port),
        (cp, ce, "RENEW_CA_DB", ca, ca_mandate, "authority", ca_caps, ca_port),
    ]:
        spawn([binary, "--config", str(cfg), "serve"], en)
        wait(lambda: api(listener, "health", operator))
        project = str(uuid4())
        run(
            "psql",
            "-XqAt",
            en[db].split("?", 1)[0],
            "-c",
            f"INSERT INTO projects(id,slug,name,root_path) VALUES('{project}','{project}','ssh fixture','/fixture')",
        )
        now = dt.datetime.now(dt.timezone.utc)
        api(
            listener,
            "v1/mandates",
            operator,
            dict(
                project_id=project,
                mandate=dict(
                    id=grant,
                    parent_id=None,
                    issuer_instance_id=instance,
                    subject_instance_id=instance,
                    specialist_id=special,
                    capabilities=cap,
                    resources=[dict(kind="ssh_host", id="monolith.test")],
                    destinations=[],
                    expires_at=(now + dt.timedelta(hours=2)).isoformat(),
                    status="active",
                    budget=dict(micro_usd=0, calls=0, turns=200, delegation_depth=1),
                ),
            ),
        )

    def advance(event):
        return json.loads(
            run(
                binary,
                "--config",
                hp,
                "ssh-renew",
                "--host",
                "monolith.test",
                "--event",
                event,
                env=he,
            )
        )

    def until(event, predicate):
        latest = None
        for _ in range(45):
            latest = advance(event)
            if predicate(latest):
                return latest
            if latest.get("status") in ["failed", "TASK_STATE_FAILED"]:
                break
            time.sleep(0.25)
        raise AssertionError(f"unexpected renewal progress: {latest}")

    def pending(event):
        return until(event, lambda v: v["status"] == "TASK_STATE_INPUT_REQUIRED")[
            "ca_task"
        ]

    event = str(uuid4())
    task = pending(event)
    res = subprocess.run(
        [
            binary,
            "--config",
            str(cp),
            "ssh-decision",
            "--peer",
            "other",
            "--task",
            task,
            "--approve",
        ],
        env=ce,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert res.returncode != 0
    for bearer, replacement in (
        (foreign, None),
        (operator, {"kind": "ssh_host_sign", "host": "other.test"}),
    ):
        answer = {"answer": "approve"}
        if replacement:
            answer["replacement_payload"] = replacement
        wire = {
            "message": {
                "messageId": str(uuid4()),
                "role": "ROLE_USER",
                "contextId": task,
                "taskId": task,
                "parts": [{"data": answer}],
            },
            "configuration": {"returnImmediately": True},
        }
        try:
            api(ca_port, "message:send", bearer, wire)
        except urllib.error.HTTPError as exc:
            with exc:
                assert exc.code == 409
        else:
            raise AssertionError("unexpected operator admission")
    assert advance(event)["status"] == "TASK_STATE_INPUT_REQUIRED"
    run(
        binary,
        "--config",
        cp,
        "ssh-decision",
        "--peer",
        "olivier",
        "--task",
        task,
        "--reject",
        "outside review",
        env=ce,
    )
    denied = until(event, lambda v: v["status"] == "TASK_STATE_REJECTED")
    assert denied["evidence"]["reason"] == "outside review"
    event = str(uuid4())
    task = pending(event)
    run(
        binary,
        "--config",
        cp,
        "ssh-decision",
        "--peer",
        "olivier",
        "--task",
        task,
        "--approve",
        "--step-password-file",
        step_pass,
        env=ce,
    )
    result = until(
        event, lambda v: v["phase"] == "installation" and v["status"] == "completed"
    )
    assert (
        result["evidence"]["fresh_login"]
        and result["evidence"]["approved_by"] == "olivier"
        and result["ca_task"] == task
    )
    assert advance(event) == result
    print(
        "VERIFIED workers + scoped DID XRPC + operator denial/approval + Step/OpenBao/fresh login through ssh-renew"
    )
