#!/usr/bin/env python3
"""Launch the single-operator Linear workflow on a private local Codifix stack."""
import argparse
import base64
from contextlib import ExitStack
from datetime import datetime, timedelta, timezone
import fcntl
from getpass import getpass
from http.client import HTTPConnection, HTTPException
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener
from uuid import UUID, uuid4


ROOT = Path(__file__).resolve().parents[2]
LINEAR = "https://api.linear.app/graphql"
PG_IMAGE = "pgvector/pgvector@sha256:b513c48566e4777cd3370451400000c78eda6a08131d68f4c31d4a76f27bd8fd"
ROLES = ("homelab", "linear")
CREDENTIAL = "CODIFIX_OPERATOR"
AGENCY_ROUTES = {
    "/xrpc/dev.codifix.agency.sendMessage": "POST",
    "/xrpc/dev.codifix.agency.getTask": "GET",
    "/xrpc/dev.codifix.agency.findRequest": "GET",
    "/xrpc/dev.codifix.agency.renewLease": "POST",
    "/xrpc/dev.codifix.agency.cancelTask": "POST",
}
LIMIT = 1_048_576
DOCKER_GROUP = None


class SetupError(Exception):
    pass


def require(condition, reason):
    if not condition:
        raise SetupError(reason)


def save(path, content):
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        file.write(content)
        temp = Path(file.name)
    temp.replace(path)


def write(path, data):
    save(path, json.dumps(data, indent=2) + "\n")
    return path


def read(path):
    return json.loads(path.read_text())


def docker(*args, **options):
    command = ["docker", *map(str, args)]
    if DOCKER_GROUP:
        command = [*DOCKER_GROUP, shlex.join(command)]
    return subprocess.run(command, check=True, **options)


def docker_access():
    try:
        subprocess.run(["docker", "info", "--format", "{{.ServerVersion}}"],
                       check=True, capture_output=True)
        return None
    except subprocess.CalledProcessError as e:
        for helper in ("sg", "newgrp"):
            if not shutil.which(helper):
                continue
            prefix = [helper, "docker", "-c"]
            if subprocess.run([*prefix, "docker info --format '{{.ServerVersion}}'"],
                              capture_output=True, stdin=subprocess.DEVNULL).returncode == 0:
                return prefix
        raise SetupError("Docker is unavailable for this account.") from e


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise SetupError("Unexpected redirect from a configured service.")


def request(url, *, body=None, token=None, limit=LIMIT):
    headers = {} if token is None else {"Authorization": token}
    raw = None
    if body is not None:
        raw = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    client = build_opener(NoRedirect(), ProxyHandler({}))
    try:
        with client.open(Request(url, raw, headers), timeout=8) as response:
            data = response.read(limit + 1)
    except HTTPError as e:
        raise SetupError(f"{urlsplit(url).hostname} returned HTTP {e.code}.") from e
    require(len(data) <= limit, "Service response exceeded its size limit.")
    return json.loads(data)


def select(options, label):
    require(options, f"No {label} is visible to this Linear key.")
    if len(options) == 1:
        return options[0]
    for index, item in enumerate(options, 1):
        print(f"{index}. {item['name']}", file=sys.stderr)
    try:
        selected = int(input(f"Choose a {label} by number: "))
    except ValueError as e:
        raise SetupError("Expected a choice by number.") from e
    require(1 <= selected <= len(options), "Choice is outside the list.")
    return options[selected - 1]


def linear_scope(state, endpoint, token):
    path = state / "linear-scope.json"
    if path.exists():
        scope = read(path)
        require(scope["endpoint"] == endpoint, "This installation is bound to a different Linear endpoint.")
        result = request(endpoint, token=token, body={
            "query": "query Project($id: String!) { project(id: $id) { id teams(first: 100) { nodes { id } } } }",
            "variables": {"id": scope["project_id"]},
        })
        project = (result.get("data") or {}).get("project") or {}
        require(not result.get("errors") and project.get("id") == scope["project_id"]
                and any(t["id"] == scope["team_id"] for t in project["teams"]["nodes"]),
                "Cannot read the configured Linear project and team with this key.")
        return scope
    projects, cursor = [], None
    for _ in range(20):
        result = request(endpoint, token=token, body={
            "query": "query Projects($after: String) { projects(first: 50, after: $after) { nodes { id name archivedAt teams(first: 30) { nodes { id name } } } pageInfo { hasNextPage endCursor } } }",
            "variables": {"after": cursor},
        })
        require(not result.get("errors") and (result.get("data") or {}).get("projects"),
                "Linear project lookup failed. Check the API key's team and read access.")
        connection = result["data"]["projects"]
        projects.extend(p for p in connection["nodes"] if not p.get("archivedAt") and p["teams"]["nodes"])
        if not connection["pageInfo"]["hasNextPage"]:
            break
        cursor = connection["pageInfo"]["endCursor"]
    project = select(projects, "Linear project")
    team = select(project["teams"]["nodes"], "Linear team")
    scope = {"endpoint": endpoint, "project_id": str(UUID(project["id"])),
             "team_id": str(UUID(team["id"])), "project_name": project["name"], "team_name": team["name"]}
    write(path, scope)
    return scope


def atomically_refresh_session(pds, name, account, state):
    reply = request(pds + "/xrpc/com.atproto.server.createSession",
                    body={"identifier": account["did"], "password": account["password"]})
    require(reply["did"] == account["did"], f"PDS returned the wrong identity for {name}.")
    token = reply["accessJwt"]
    part = token.split(".")[1]
    claims = json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))
    require(claims["sub"] == account["did"] and claims["exp"] > time.time() + 600,
            f"PDS returned a stale session for {name}.")
    save(state / f"{name}.jwt", token + "\n")
    return claims["exp"]


def start_session_refresh(pds, accounts, state):
    expiry = {name: atomically_refresh_session(pds, name, accounts[name], state) for name in ROLES}
    stop = threading.Event()

    def refresh():
        while not stop.wait(30):
            for name in ROLES:
                if expiry[name] < time.time() + 900:
                    try:
                        expiry[name] = atomically_refresh_session(pds, name, accounts[name], state)
                    except (SetupError, OSError, ValueError):
                        save(state / "session-status", f"PDS session renewal delayed for {name}.\n")
    thread = threading.Thread(target=refresh, daemon=True)
    thread.start()
    return stop


def identity(state):
    path = state / "installation.json"
    if not path.exists():
        data = {
            "id": uuid4().hex[:16], "postgres_password": secrets.token_hex(32),
            "roles": {
                role: {key: str(uuid4()) for key in ("instance", "project", "mandate")}
                | {"operator": secrets.token_hex(32), "password": secrets.token_hex(32)}
                for role in ROLES
            },
        }
        write(path, data)
    return read(path)


def start_database(installation):
    name = "codifix-linear-db-" + installation["id"]
    label = "org.codifix.linear.installation"
    try:
        info = docker("inspect", name, capture_output=True, text=True)
    except subprocess.CalledProcessError:
        values = {"POSTGRES_PASSWORD": installation["postgres_password"]}
        docker("run", "-d", "--restart", "unless-stopped", "--name", name,
               "--label", f"{label}={installation['id']}", "-p", "127.0.0.1::5432",
               "--volume", name + ":/var/lib/postgresql/data",
               "--env", "POSTGRES_PASSWORD", PG_IMAGE, env=os.environ | values, stdout=subprocess.DEVNULL)
    else:
        container = json.loads(info.stdout)[0]
        require(container["Config"]["Labels"].get(label) == installation["id"],
                "Database container belongs to another installation.")
        if not container["State"]["Running"]:
            docker("start", name, stdout=subprocess.DEVNULL)
    for _ in range(60):
        try:
            docker("exec", name, "pg_isready", "-h", "127.0.0.1", "-U", "postgres",
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            break
        except subprocess.CalledProcessError:
            time.sleep(1)
    else:
        raise SetupError("Private PostgreSQL did not become ready.")
    address = docker("port", name, "5432/tcp", capture_output=True, text=True).stdout.strip()
    require(address.startswith("127.0.0.1:"), "Private PostgreSQL is not loopback-only.")
    return name, address


def pg(container, database, statement):
    return docker("exec", "-i", "-u", "postgres", container, "psql",
                  "-XAt", "-v", "ON_ERROR_STOP=1", "-d", database,
                  input=statement, capture_output=True, text=True).stdout.strip()


def tls_pair(state):
    root = state / "peer-root.pem"
    key = state / "peer-key.pem"
    leaf = state / "peer-leaf.pem"
    if all(p.is_file() for p in (root, key, leaf)):
        return root, key, leaf
    ca_key = state / "peer-root-key.pem"
    csr = state / "peer.csr"
    ext = state / "peer-ext.txt"
    save(ext, "basicConstraints=critical,CA:FALSE\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n")
    for arguments in (
        ["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "3650",
         "-subj", "/CN=Codifix Linear private peer root", "-addext", "basicConstraints=critical,CA:TRUE",
         "-keyout", ca_key, "-out", root],
        ["req", "-new", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=Codifix Linear peers",
         "-keyout", key, "-out", csr],
        ["x509", "-req", "-in", csr, "-CA", root, "-CAkey", ca_key,
         "-set_serial", "0x" + secrets.token_hex(16), "-days", "3650", "-extfile", ext, "-out", leaf],
    ):
        subprocess.run(["openssl", *map(str, arguments)], check=True, capture_output=True)
    return root, key, leaf


class PeerProxy(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, status, data, content_type="application/json"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def dispatch(self):
        route = urlsplit(self.path).path
        if AGENCY_ROUTES.get(route) != self.command or self.path.startswith("//") or len(self.path) > 4096:
            self.send(404, b"{}")
            return
        try:
            self.connection.settimeout(10)
            count = int(self.headers.get("Content-Length", "0"))
            if not 0 <= count <= LIMIT or self.headers.get("Transfer-Encoding"):
                self.send(413, b"{}")
                return
            body = self.rfile.read(count) if count else None
            headers = {k: self.headers[k] for k in ("Authorization", "Content-Type") if k in self.headers}
            with ExitStack() as stack:
                conn = HTTPConnection("127.0.0.1", self.server.upstream, timeout=10)
                stack.callback(conn.close)
                conn.request(self.command, self.path, body, headers)
                response = conn.getresponse()
                data = response.read(LIMIT + 1)
                if len(data) > LIMIT or 300 <= response.status < 400:
                    self.send(502, b"{}")
                else:
                    self.send(response.status, data, response.getheader("Content-Type", "application/json"))
        except (HTTPException, OSError, ValueError):
            self.send(502, b"{}")

    do_GET = do_POST = dispatch


def start_ingress(stack, upstream, key, leaf):
    server = ThreadingHTTPServer(("127.0.0.1", 0), PeerProxy)
    server.upstream = upstream
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2
    ctx.load_cert_chain(leaf, key)
    server.socket = ctx.wrap_socket(server.socket, server_side=True, do_handshake_on_connect=False)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    stack.callback(server.server_close)
    stack.callback(server.shutdown)
    return f"https://127.0.0.1:{server.server_port}"


def local_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def wait_health(proc, url, component, log):
    for _ in range(80):
        if proc.poll() is not None:
            break
        try:
            value = request(url)
            if value.get("status") == "ok":
                return
        except (SetupError, OSError, ValueError):
            pass
        time.sleep(.25)
    raise SetupError(f"{component} did not become ready. See {log}.")


def launch(stack, argv, environment, log):
    with log.open("ab") as stream:
        process = subprocess.Popen(argv, env=environment, stdin=subprocess.DEVNULL, stdout=stream, stderr=stream)

    def stop():
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    stack.callback(stop)
    return process


def bootstrap(cli, state, container, role, records, url):
    log = state / f"{role}-schema.log"
    database = "codifix_" + role
    require(records["password"].isalnum(), "Invalid database credential in installation state.")
    statement = rf"""
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L','{database}','{records["password"]}')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='{database}') \gexec
SELECT format('CREATE DATABASE %I OWNER %I','{database}','{database}')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname='{database}') \gexec
REVOKE CONNECT ON DATABASE {database} FROM PUBLIC;
"""
    pg(container, "postgres", statement)
    pg(container, database, "CREATE EXTENSION IF NOT EXISTS vector;")
    port = local_port()
    config = state / f"{role}-schema.toml"
    save(config, f'[service]\nbind_addr="127.0.0.1:{port}"\napi_token="{secrets.token_hex(32)}"\n'
                 f'[database]\nurl="{url}"\n[cluster]\nenabled=false\n[offline]\nenabled=false\n'
                 '[background_work]\nenabled=false\n[reinforcement]\nenabled=false\n[consolidation]\nenabled=false\n')
    with ExitStack() as bootstrap_stack:
        proc = launch(bootstrap_stack, [cli, "--config", config, "service", "run"],
                      {"PATH": os.environ["PATH"], "HOME": str(state), "CODIFIX_PROFILE": "prod"}, log)
        wait_health(proc, f"http://127.0.0.1:{port}/healthz", f"{role} database", log)
    pid = str(UUID(records["project"]))
    pg(container, database, f"""INSERT INTO projects(id,slug,name,root_path)
VALUES ('{pid}','linear-workflow','Private Linear workflow','/synthetic/linear-workflow')
ON CONFLICT (id) DO NOTHING;""")


def budget(calls, turns, depth):
    return {"calls": calls, "turns": turns, "micro_usd": 0, "delegation_depth": depth}


def configure(state, roles, accounts, pds, ports, ingress, scope):
    for role in ROLES:
        here = roles[role]
        other = "linear" if role == "homelab" else "homelab"
        capability = "delegate" if role == "homelab" else "linear_track"
        peer = {
            "id": other, "instance_id": roles[other]["instance"], "endpoint": ingress[other],
            "mandates": [here["mandate"]] if role == "linear" else [],
        }
        config = {
            "instance_id": here["instance"], "listen": f"127.0.0.1:{ports[role]}",
            "public_url": ingress[role], "database_env": "CODIFIX_PRIVATE_DATABASE",
            "specialists": [{
                "schema_version": 1, "instance_id": here["instance"], "specialist_id": role,
                "name": "Linear Agent" if role == "linear" else "Homelab Agent",
                "capabilities": [capability],
            }],
            "peers": [{"id": "operator", "instance_id": here["instance"], "operator": True,
                       "token_env": CREDENTIAL}, peer],
            "social": {"did": accounts[role]["did"], "pds": pds,
                       "access_token_file": str(state / f"{role}.jwt"),
                       "peers": {other: {"did": accounts[other]["did"], "signing_key": accounts[other]["signing_key"]}}},
        }
        if role == "linear":
            config["linear"] = {"endpoint": scope["endpoint"], "token_env": "CODIFIX_LINEAR_TOKEN",
                "require_origin_confirmation": True,
                "tracking": {here["mandate"]: {"project_id": scope["project_id"], "team_id": scope["team_id"]}}}
        write(state / (role + ".json"), config)


def binary_paths(arguments):
    if arguments.bin_dir:
        base = Path(arguments.bin_dir).resolve()
    else:
        subprocess.run(["cargo", "build", "--locked", "-p", "codifix-cli",
                        "--no-default-features", "--features", "codifix-cli/tui,codifix-cli/embedded-service-postgres",
                        "-p", "codifix-agency-runtime"], cwd=ROOT, check=True)
        metadata = subprocess.check_output(["cargo", "metadata", "--offline", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT)
        base = Path(json.loads(metadata)["target_directory"]) / "debug"
    for name in ("codifix", "codifix-agency"):
        require((base / name).is_file() and os.access(base / name, os.X_OK), f"Missing {base / name}.")
    return base / "codifix", base / "codifix-agency"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", type=Path, default=Path.home() / ".local/state/codifix-linear")
    parser.add_argument("--bin-dir", type=Path, help="Use a staged Codifix CLI and agency binary.")
    parser.add_argument("--fixture-endpoint", help=argparse.SUPPRESS)
    args = parser.parse_args()
    global DOCKER_GROUP
    DOCKER_GROUP = docker_access()
    state = args.state.resolve()
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    require(state.stat().st_uid == os.getuid() and state.stat().st_mode & 0o077 == 0,
            "Use a directory owned by this account with permissions 0700.")
    with (state / "launch.lock").open("w") as lock, ExitStack() as stack:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as e:
            raise SetupError("This Linear TUI installation is already running.") from e
        endpoint = LINEAR
        if args.fixture_endpoint:
            url = urlsplit(args.fixture_endpoint)
            require(url.scheme == "http" and url.hostname == "127.0.0.1" and url.port
                    and not url.username and not url.password and url.path == "/graphql",
                    "The test provider must be a loopback GraphQL endpoint.")
            require(state != Path.home() / ".local/state/codifix-linear",
                    "The test provider needs a separate state directory.")
            endpoint = args.fixture_endpoint
        key = state / "linear-key"
        token = (os.environ.get("CODIFIX_LINEAR_TOKEN") or
                 (key.read_text().strip() if key.exists() else "") or
                 getpass("Linear API key (Read and Create issues for the selected team): ")).strip()
        require(token and len(token) <= 4096 and token.isascii() and not any(c.isspace() for c in token),
                "Supply a valid Linear API key.")
        scope = linear_scope(state, endpoint, token)
        save(key, token + "\n")
        print(f"Linear destination: {scope['project_name']} / {scope['team_name']}", file=sys.stderr)
        cli, agency = binary_paths(args)
        install = identity(state)
        roles = install["roles"]
        pg_container, pg_address = start_database(install)
        pds_command = ["bash", str(Path(__file__).parent / "private-pds/install.sh"), str(state / "private-pds")]
        if DOCKER_GROUP:
            pds_command = [*DOCKER_GROUP, shlex.join(pds_command)]
        accounts_path = subprocess.check_output(pds_command, text=True).strip()
        settings = read(Path(accounts_path))
        pds, accounts = settings["pds_url"], settings["accounts"]
        refresh = start_session_refresh(pds, accounts, state)
        stack.callback(refresh.set)
        root, tls_key, leaf = tls_pair(state)
        trust_bundle = state / "tls-trust.pem"
        system_ca = Path("/etc/ssl/certs/ca-certificates.crt")
        require(system_ca.is_file(), "The system TLS CA bundle is missing.")
        save(trust_bundle, system_ca.read_text().rstrip() + "\n" + root.read_text())
        ports, ingress = {}, {}
        for role in ROLES:
            ports[role] = local_port()
            ingress[role] = start_ingress(stack, ports[role], tls_key, leaf)
        configure(state, roles, accounts, pds, ports, ingress, scope)
        for role in ROLES:
            here = roles[role]
            database = "codifix_" + role
            url = f"postgresql://{database}:{here['password']}@{pg_address}/{database}"
            bootstrap(cli, state, pg_container, role, here, url)
            env = {"PATH": os.environ["PATH"], "HOME": str(state), "SSL_CERT_FILE": str(trust_bundle),
                   "CODIFIX_PRIVATE_DATABASE": url, CREDENTIAL: here["operator"]}
            if role == "linear":
                env["CODIFIX_LINEAR_TOKEN"] = token
            config = state / (role + ".json")
            log = state / (role + ".log")
            subprocess.run([agency, "--config", config, "check"], check=True,
                           env=env, stdout=subprocess.DEVNULL)
            proc = launch(stack, [agency, "--config", config, "serve"], env, log)
            wait_health(proc, f"http://127.0.0.1:{ports[role]}/health", role, log)
            capability = "linear_track" if role == "linear" else "delegate"
            resource = {"kind": "linear_project", "id": scope["project_id"]} if role == "linear" else {"kind": "peer", "id": "linear"}
            grant = {"project_id": here["project"], "mandate": {
                "id": here["mandate"], "issuer_instance_id": here["instance"], "subject_instance_id": here["instance"],
                "specialist_id": role, "capabilities": [capability], "resources": [resource], "destinations": [],
                "expires_at": (datetime.now(timezone.utc) + timedelta(days=180)).isoformat(), "status": "active",
                "budget": budget(10000, 10000, int(role == "homelab")),
            }}
            existing = pg(pg_container, database, f"SELECT count(*) FROM agency_mandates WHERE id='{here['mandate']}';")
            if existing == "0":
                path = write(state / (role + "-grant.json"), grant)
                subprocess.run([agency, "--config", config, "grant", "--peer", "operator", "--file", path],
                               check=True, env=env, stdout=subprocess.DEVNULL)
        profile = write(state / "tui.json", {
            "endpoint": f"http://127.0.0.1:{ports['homelab']}", "peer": "operator", "token_env": CREDENTIAL,
            "local_specialist": "homelab", "local_mandate_id": roles["homelab"]["mandate"],
            "remote_peer": "linear", "remote_specialist": "linear", "remote_mandate_id": roles["linear"]["mandate"],
            "linear_project_id": scope["project_id"],
        })
        cli_env = {k: v for k, v in os.environ.items() if k != "CODIFIX_LINEAR_TOKEN"}
        cli_env[CREDENTIAL] = roles["homelab"]["operator"]
        return subprocess.run([cli, "linear-tui", "--profile", profile], env=cli_env).returncode


if __name__ == "__main__":
    os.umask(0o077)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        sys.exit(main())
    except (SetupError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Codifix Linear setup stopped. {error}", file=sys.stderr)
        sys.exit(1)
