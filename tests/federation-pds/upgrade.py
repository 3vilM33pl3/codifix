"""Exercise rollout with real processes in two isolated test databases and a private PDS."""
from contextlib import ExitStack
from datetime import datetime, timedelta, timezone
from http.client import HTTPConnection
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import urllib.request
import uuid

ID = lambda: str(uuid.uuid4())
HERE = Path(__file__).resolve()
RELEASE = HERE.parents[2] / "scripts/agency/release-rollout.py"


def port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def write(path, data):
    path.write_text(json.dumps(data))
    return path


def tls_peer(stack, target, traffic):
    class Handler(BaseHTTPRequestHandler):
        def dispatch(self):
            conn = HTTPConnection("127.0.0.1", target, timeout=30)
            try:
                data = self.rfile.read(int(self.headers.get("content-length", 0)))
                conn.request(self.command, self.path, data, dict(self.headers))
                reply = conn.getresponse()
                traffic.append((self.path.split("?")[0], reply.status))
                body = reply.read()
                self.send_response(reply.status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            finally:
                conn.close()
        do_GET = do_POST = dispatch

        def log_message(self, *_):
            pass

    web = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    certdir = Path(os.environ["SSL_CERT_FILE"]).parent
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(certdir / "agency.pem", certdir / "agency-key.pem")
    web.socket = tls.wrap_socket(web.socket, server_side=True)
    threading.Thread(target=web.serve_forever, daemon=True).start()
    stack.callback(web.server_close)
    stack.callback(web.shutdown)
    return f"https://127.0.0.1:{web.server_port}"


def wait_json(url, expected, timeout=15):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=1) as response:
                value = json.load(response)
            if all(value.get(key) == val for key, val in expected.items()):
                return value
        except OSError:
            pass
        time.sleep(.1)
    raise RuntimeError(f"Timed out waiting for {url} {expected}")


def stop(process):
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=10)


def launch(argv, env, log):
    return subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL,
                            stdout=log, stderr=log)


def main(root, stack):
    binary = root / "candidate-agency"
    shutil.copy2(os.environ.get("CODIFIX_TEST_RELEASE_CANDIDATE") or os.environ["CANDIDATE_AGENCY"], binary)
    version = os.environ["CANDIDATE_VERSION"]
    previous = Path(os.environ["CODIFIX_TEST_PRIOR_AGENCY"])
    pds = json.loads(Path(os.environ["CODIFIX_FEDERATION_PDS_FIXTURE"]).read_text())
    ids = [ID(), ID()]
    projects = [ID(), ID()]
    ports = [port(), port()]
    traffic = []
    peers = [tls_peer(stack, p, traffic) for p in ports]
    urls = [f"http://127.0.0.1:{p}" for p in ports]
    did = [pds["repository"], pds["recipient_did"]]
    keys = [pds["signing_key"], pds["recipient_key"]]
    tokens = [pds["token"], pds["recipient_token"]]
    names = ["publisher", "ca"]
    caps = [["delegate", "inspect", "upgrade-help"], ["upgrade_candidate"]]
    grants = [ID(), ID()]
    resources = [{"kind":"peer","id":"ca"}, {"kind":"installation","id":ids[1]}]
    probe_target = {"kind":"service","id":"upgrade-check"}
    expiry = (datetime.now(timezone.utc) + timedelta(minutes=20)).isoformat()
    deadline = (datetime.now(timezone.utc) + timedelta(minutes=8)).isoformat()
    configs, envs = [], []
    local_token = secrets.token_hex(24)
    for i in range(2):
        other = 1-i
        env = dict(os.environ, FIXTURE_DB=os.environ[["PUBLISHER_DB","CA_DB"][i]],
                   LOCAL_OPERATOR_TOKEN=local_token, FIXTURE_PDS=tokens[i])
        cfg = {"instance_id":ids[i],"listen":f"127.0.0.1:{ports[i]}","public_url":peers[i],
               "database_env":"FIXTURE_DB","specialists":[{"schema_version":1,"instance_id":ids[i],
               "specialist_id":names[i],"name":names[i],"capabilities":caps[i]}],
               "peers":[{"id":"operator","instance_id":ids[i],"operator":True,"token_env":"LOCAL_OPERATOR_TOKEN"},
                        {"id":names[other],"instance_id":ids[other],"operator":False,"token_env":"UNUSED",
                         "endpoint":peers[other],"mandates":[grants[i]]}],
               "social":{"did":did[i],"pds":pds["base"],"access_token_env":"FIXTURE_PDS",
                         "peers":{names[other]:{"did":did[other],"signing_key":keys[other]}}}}
        if i == 0:
            cfg["public_profile"]={"$type":"dev.codifix.agency.v1.capability","schemaVersion":1,
                "instanceId":ids[i],"specialistId":names[i],"displayName":"Release support","summary":"Upgrade assistance",
                "capabilities":["upgrade-help"],"agentCardUrl":"https://agency.example.com/card",
                "publishedAt":datetime.now(timezone.utc).isoformat()}
        configs.append(write(root/(names[i]+".json"),cfg))
        envs.append(env)
        subprocess.run(["psql","-X","-q","-v","ON_ERROR_STOP=1",env["FIXTURE_DB"],"-c",
            f"INSERT INTO projects(id,slug,name,root_path) VALUES('{projects[i]}','upgrade','Upgrade','/synthetic')"],check=True)
    log = stack.enter_context((root/"process.log").open("wb"))
    bao_port = port()
    bao_env = dict(os.environ, HOME=str(root))
    bao = launch(["bao","server","-dev",f"-dev-listen-address=127.0.0.1:{bao_port}"],bao_env,log)
    stack.callback(stop, bao)
    bao_health = f"http://127.0.0.1:{bao_port}/v1/sys/health"
    wait_json(bao_health,{"sealed":False,"initialized":True})

    # Exercise a separate scratch installation before issuing the rollout receipt.
    cfg = json.loads(configs[1].read_text())
    scratch = port()
    cfg["listen"] = f"127.0.0.1:{scratch}"
    scratch_cfg = write(root/"scratch.json", cfg)
    import hashlib
    hash_file = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    # Tests are repeated by the real certification entry point, including an actual
    # install and version readback. The output hashes bind the checks to this binary.
    envs[1].update(SCRATCH_CONFIG=str(scratch_cfg), SCRATCH_PORT=str(scratch),
                   SCRATCH_VERSION=version, SCRATCH_PRIOR=str(previous), SCRATCH_DIR=str(root))
    def helper(name, content):
        path=root/name
        path.write_text("#!/usr/bin/env python3\n"+content)
        path.chmod(0o700)
        return path
    artifact_test=helper("artifact-test",'''import subprocess,sys,os
subprocess.run([sys.argv[1],"--config",os.environ["SCRATCH_CONFIG"],"check"],check=True)
''')
    upgrade_test=helper("test-upgrade",'''import json,os,shutil,subprocess,sys,time,urllib.request
p=os.path.join(os.environ["SCRATCH_DIR"],"scratch-agency")
for src in [os.environ["SCRATCH_PRIOR"],sys.argv[1]]:
    shutil.copy2(src,p)
    child=subprocess.Popen([p,"--config",os.environ["SCRATCH_CONFIG"],"serve"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            try:
                with urllib.request.urlopen("http://127.0.0.1:"+os.environ["SCRATCH_PORT"]+"/health",timeout=1) as r: state=json.load(r)
                assert state["status"]=="ok"
                if src==sys.argv[1]: assert state["version"]==os.environ["SCRATCH_VERSION"]
                print(state)
                break
            except OSError: time.sleep(.1)
        else: raise RuntimeError("no startup")
    finally: child.terminate();child.wait(timeout=10)
''')
    def certify(receipt, url_path, upgrade_gate):
        return subprocess.run([RELEASE, "certify", "--version", version,
                               "--release-url", peers[0] + url_path,
                               "--artifact", binary, "--artifact-test", artifact_test,
                               "--upgrade-test", upgrade_gate, "--receipt", receipt],
                              env=envs[1], stdout=log, stderr=log)

    rejected = root/"rejected.json"
    rejected_gate = helper("reject-upgrade", "raise SystemExit(1)")
    assert certify(rejected, "/rejected", rejected_gate).returncode != 0
    assert not rejected.exists(), "a failed test issued a release receipt"
    receipt_path=root/"tested.json"
    certify(receipt_path, "/candidate", upgrade_test).check_returncode()
    release=json.loads(receipt_path.read_text())
    assert hash_file(binary)==release["sha256"]
    for field in ("artifact_test_sha256", "upgrade_test_sha256"):
        log_path = root / (release[field] + ".log")
        assert hash_file(log_path) == release[field] and log_path.stat().st_mode & 0o077 == 0
    pub=launch([binary,"--config",configs[0],"serve"],envs[0],log)
    stack.callback(stop,pub)
    active = root/"ca-agency"
    shutil.copy2(previous,active)
    ca=launch([active,"--config",configs[1],"serve"],envs[1],log)
    stack.callback(stop,ca)
    pid=root/"ca.pid"
    pid.write_text(str(ca.pid))
    def cleanup_restarted():
        try: os.kill(int(pid.read_text()),signal.SIGTERM)
        except ProcessLookupError: pass
    stack.callback(cleanup_restarted)
    for url in urls: wait_json(url+"/health",{"status":"ok"})
    budget=lambda depth: {"calls":30*(depth+1),"turns":30*(depth+1),"micro_usd":0,"delegation_depth":depth}
    def cli(i,*args):
        return json.loads(subprocess.check_output([binary,"--config",configs[i],*args],env=envs[i]))
    for i in range(2):
        mandate={"id":grants[i],"issuer_instance_id":ids[i],"subject_instance_id":ids[i],
            "specialist_id":names[i],"capabilities":caps[i],"resources":[resources[i]]+([probe_target] if i==0 else []),
            "destinations":[],"expires_at":expiry,"status":"active",
            "budget":{"calls":300,"turns":300,"micro_usd":0,"delegation_depth":4}}
        cli(i,"grant","--peer","operator","--file",write(root/f"grant{i}.json",{"project_id":projects[i],"mandate":mandate}))
    def request(i,cap,criterion,resource,depth):
        return {"request_id":ID(),"mandate_id":grants[i],"specialist_id":names[i],"objective":"Verify release candidate",
                "acceptance_criteria":[criterion],"capability":cap,"resource":resource,"deadline":deadline,
                "budget":budget(depth),"payload":{"kind":cap}}
    delegate=request(0,"delegate","delegated_result_reported",resources[0],2)
    delegate["payload"].update(peer="ca",request=request(1,"upgrade_candidate","upgrade_verified",resources[1],1))
    rollout=write(root/"rollout.json",{"delegation":delegate,"agency_config":str(configs[0]),"agency_binary":str(binary),
        "pds":pds["base"],"did":did[0],"help_capability":"upgrade-help","pds_token_env":"FIXTURE_PDS","local_peer":"operator"})
    installer=helper("install",'''import os,shutil,signal,subprocess,sys
from pathlib import Path
p=Path(os.environ["CA_PID"]);os.kill(int(p.read_text()),signal.SIGTERM)
active=Path(os.environ["CA_ACTIVE"]);tmp=active.with_suffix(".new");shutil.copy2(sys.argv[1],tmp);tmp.replace(active)
import time
for _ in range(100):
    try:
        import socket
        with socket.create_connection(("127.0.0.1",int(os.environ["CA_PORT"])),timeout=.1): pass
    except OSError: break
    time.sleep(.05)
child=subprocess.Popen([active,"--config",os.environ["CA_CFG"],"serve"],start_new_session=True,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
p.write_text(str(child.pid))
''')
    envs[1].update(CA_PID=str(pid),CA_ACTIVE=str(active),CA_PORT=str(ports[1]),CA_CFG=str(configs[1]))
    profile=write(root/"supervisor.json",{"publisher_peer":"publisher","release_receipt":str(receipt_path),
        "artifact":str(binary),"installed_artifact":str(active),"install_command":[str(installer)],"companion_health":urls[1]+"/health",
        "openbao_health":bao_health})
    def run_rollout(receipt, supervisor_profile, output, expected_exit):
        with output.open("wb") as stream:
            job = launch([RELEASE, "deploy", "--receipt", receipt, "--rollout", rollout],
                         envs[0], stream)
            stack.callback(stop, job)
            handled = False
            for _ in range(100):
                handled = cli(1, "upgrade-once", "--profile", supervisor_profile)["handled"]
                if handled:
                    break
                if job.poll() is not None:
                    break
                time.sleep(.2)
            else:
                raise AssertionError("no upgrade received")
            status = job.wait(timeout=65)
        if status != expected_exit or not handled:
            # Keep process.log and unstructured child stderr private. Only
            # report structured publisher updates and CA verification fields.
            updates = []
            for line in output.read_text().splitlines():
                try:
                    update = json.loads(line)
                except ValueError:
                    continue
                if isinstance(update, dict) and "task_id" in update:
                    updates.append({key: update.get(key) for key in ("task_id", "status", "question")})
            rows = subprocess.check_output([
                "psql", "-XAt", envs[1]["FIXTURE_DB"], "-c",
                "SELECT json_build_object('state', state, 'step', report->>'step', "
                "'installed_version', report->>'installed_version', 'peer_message', report->'peer_message') "
                "FROM agency_upgrades ORDER BY created_at"
            ], text=True).splitlines()
            raise AssertionError(f"rollout exited {status}, expected {expected_exit}, handled={handled}; "
                                 f"publisher={updates}; ca={[json.loads(row) for row in rows]}; xrpc={traffic[-12:]}")

    def availability():
        return cli(0, "discover", "--pds", pds["base"], "--did", did[0],
                   "--record-key", ids[0].replace("-", "") + "-publisher")["capabilities"]

    run_rollout(receipt_path, profile, root/"success.out", 0)
    ca.wait(timeout=10)
    result=subprocess.check_output(["psql","-XAt",envs[1]["FIXTURE_DB"],"-c","select report from agency_upgrades where state='verified'"])
    verified = json.loads(result)
    assert verified["installed_version"] == version
    completed = json.loads((root/"success.out").read_text().splitlines()[-1])
    assert completed["status"] == "completed"
    print(f"Publisher {completed['task_id']} completed, CA {version} verified; "
          f"OpenBao ready={verified['openbao_ready']}, peer message={verified['peer_message']}.", flush=True)
    assert hash_file(active)==release["sha256"]
    assert not availability()

    # A distinct certified release reaches the CA, but its OpenBao readiness
    # endpoint returns companion health instead of initialized/unsealed evidence.
    second_receipt = root/"next-tested.json"
    certify(second_receipt, "/next-candidate", upgrade_test).check_returncode()
    failed_profile = json.loads(profile.read_text())
    failed_profile.update(release_receipt=str(second_receipt), openbao_health=urls[1]+"/health")
    failed_output = root/"failed.out"
    run_rollout(second_receipt, write(root/"fail-health.json", failed_profile), failed_output, 1)
    updates = [json.loads(line) for line in failed_output.read_text().splitlines()]
    last = updates[-1]
    assert last["status"] == "input_required", last
    expected = "Upgrade stopped during openbao_health. Codifix Agent assistance is required."
    assert last["question"]["question"] == expected, last
    publisher_task = cli(0, "task", "--peer", "operator", "--id", last["task_id"])
    assert publisher_task["contract"]["clarification"]["question"] == expected
    failed_ca = json.loads(subprocess.check_output([
        "psql", "-XAt", envs[1]["FIXTURE_DB"], "-c",
        "SELECT json_build_object('id', task_id, 'report', report) FROM agency_upgrades WHERE state='failed'"
    ]))
    assert failed_ca["report"] == {"step": "openbao_health", "assistance_requested": True}
    ca_task = cli(1, "task", "--peer", "operator", "--id", failed_ca["id"])
    assert ca_task["contract"]["clarification"]["question"] == expected
    assert not availability(), "public availability remained after requesting private assistance"
    print(f"Verified release {version}, rejected certification, private assistance and withdrawal.")


if __name__=="__main__":
    with tempfile.TemporaryDirectory(prefix="codifix-upgrade-") as directory, ExitStack() as stack:
        main(Path(directory),stack)
