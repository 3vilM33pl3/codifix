#!/usr/bin/env python3
"""Explicit, journaled native provisioning. No downloads, builds or legacy service control.

Python 3.11+, installed reviewed Codifix/PG/pgvector and native Mac acceptance are
prerequisites, not silently satisfied by this operator. Tests use Native fakes.
"""
import argparse
import json
import os
from pathlib import Path
import plistlib
import re
import secrets
import shlex
import sys
import uuid
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).absolute().parent))
import onboarding_files as files
from onboarding_files import Refused
from onboarding_native import Native, clean_env

AREAS = {"workshop", "homelab", "general"}
TOOLS = {"postgres", "initdb", "pg_ctl", "psql", "pg_config", "pg_controldata"}


def exact(value, keys):
    if not isinstance(value, dict) or set(value) != set(keys):
        raise Refused("unexpected/missing operator metadata fields")


def identifier(value):
    try:
        parsed = uuid.UUID(value)
    except (ValueError, TypeError, AttributeError):
        raise Refused("stable non-nil canonical UUID required") from None
    if str(parsed) != value or parsed.int == 0:
        raise Refused("stable non-nil canonical UUID required")
    return value


def pin(value):
    if not isinstance(value, str) or not re.fullmatch("[a-f0-9]{64}", value):
        raise Refused("exact lowercase SHA-256 pins required")
    return value


def port(value):
    if type(value) is not int or not 1024 <= value <= 65535:
        raise Refused("explicit unprivileged loopback ports required")
    return value


def metadata(value):
    exact(value, {"schema_version", "company", "subscription", "bootstrap", "allow_loopback_development"} | ({"replication_mode"} if "replication_mode" in value else set()) | ({"manifest_sha256"} if "manifest_sha256" in value else set()))
    if value.get("replication_mode", "shared_publications_v1") not in {"shared_publications_v1", "curated_project_v1"}:
        raise Refused("unsupported device replication mode")
    if value.get("replication_mode") == "curated_project_v1":
        pin(value.get("manifest_sha256"))
    if value["schema_version"] != 1 or type(value["allow_loopback_development"]) is not bool:
        raise Refused("unsupported subscription metadata")
    identifier(value["subscription"])
    url = urlsplit(value["company"])
    if url.username or url.password or url.path or url.query or url.fragment or not url.hostname:
        raise Refused("company must be a credential-free exact origin")
    import ipaddress
    try:
        loopback = ipaddress.ip_address(url.hostname).is_loopback
    except ValueError:
        loopback = url.hostname == "localhost"
    if url.scheme != "https" and not (url.scheme == "http" and loopback and value["allow_loopback_development"]):
        raise Refused("HTTPS company required; insecure origins allowed only for explicit loopback development")
    bootstrap = value["bootstrap"]
    exact(bootstrap, {"schema_version", "device", "scope", "catalog_revision", "authority", "receipt_keys"})
    if bootstrap["schema_version"] != 1 or type(bootstrap["catalog_revision"]) is not int or not 1 <= bootstrap["catalog_revision"] <= 2**63-1:
        raise Refused("reviewed subscription revision required")
    identifier(bootstrap["device"])
    identifier(bootstrap["authority"])
    scope = bootstrap["scope"]
    exact(scope, {"company", "area", "project", "owner", "space"})
    if scope["area"] not in AREAS:
        raise Refused("Trust is remote-only; unsupported local area")
    for key in ["company", "project", "owner", "space"]:
        identifier(scope[key])
    if not isinstance(bootstrap["receipt_keys"], list) or not 1 <= len(bootstrap["receipt_keys"]) <= 16:
        raise Refused("bounded owner receipt keys required (Codifix validates cryptographic contents)")


def make_plan(source, *, prerequisites_only=False):
    exact(source, {"schema_version", "root", "codifix", "postgres", "pg_port", "areas", "subscriptions"} | ({"curated_sync", "repositories"} if source.get("schema_version") == 2 else set()))
    if type(source["schema_version"]) is not int or source["schema_version"] not in {1, 2}:
        raise Refused("unsupported operator schema")
    if source["schema_version"] == 2:
        if source["curated_sync"] is not True or not isinstance(source["repositories"], list) or not 1 <= len(source["repositories"]) <= 8:
            raise Refused("curated native workflow requires exact selected repository routes")
        seen_repositories = set()
        for route in source["repositories"]:
            exact(route, {"area", "repository", "project", "project_id"})
            identifier(route["project_id"])
            repository = str(files.path(route["repository"]))
            if route["area"] not in AREAS or not isinstance(route["project"], str) or not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,63}", route["project"]) or repository in seen_repositories:
                raise Refused("duplicate repository or unsupported exact project route")
            seen_repositories.add(repository)
    root = files.path(source["root"])
    if not root.parent.is_dir():
        raise Refused("existing current-user private operator parent prerequisite missing")
    files.directory(root.parent)  # Validation only; never create it during preview.
    if "'" in str(root):
        raise Refused("operator data path cannot contain SQL quote characters")
    if not re.fullmatch("[A-Za-z0-9_-]{1,64}", root.name):
        raise Refused("new operator root needs a bounded ASCII name")
    memory = source["codifix"]
    exact(memory, {"path", "sha256", "version"})
    if not isinstance(memory["version"], str) or not 1 <= len(memory["version"]) <= 128 or any(ord(c) < 32 for c in memory["version"]):
        raise Refused("exact installed Codifix version required")
    pins = [{"path": str(files.path(memory["path"])), "sha256": pin(memory["sha256"]), "kind": "mach-o"}]
    pg = source["postgres"]
    exact(pg, {"bin_dir", "major", "tool_sha256", "vector_library", "vector_library_sha256", "vector_control", "vector_control_sha256", "vector_version", "vector_sql"})
    if type(pg["major"]) is not int or pg["major"] not in {16, 17, 18} or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", pg["vector_version"]):
        raise Refused("explicit PG16..18 and exact installed pgvector version required; native compatibility still pending")
    exact(pg["tool_sha256"], TOOLS)
    for tool in sorted(TOOLS):
        pins.append({"path": str(files.path(Path(pg["bin_dir"]) / tool)), "sha256": pin(pg["tool_sha256"][tool]), "kind": "mach-o"})
    for name, kind in [("vector_library", "mach-o"), ("vector_control", "data")]:
        pins.append({"path": str(files.path(pg[name])), "sha256": pin(pg[name + "_sha256"]), "kind": kind})
    if not isinstance(pg["vector_sql"], dict) or not 1 <= len(pg["vector_sql"]) <= 128:
        raise Refused("installed vector SQL pins required")
    required = f"vector--{pg['vector_version']}.sql"
    if required not in pg["vector_sql"]:
        raise Refused("exact pgvector installation SQL pin required")
    for name, sha in pg["vector_sql"].items():
        if not re.fullmatch(r"vector--[0-9.\-]+\.sql", name):
            raise Refused("unsupported pgvector SQL name")
        pins.append({"path": str(files.path(Path(pg["vector_control"]).parent / name)), "sha256": pin(sha), "kind": "data"})
    # Pin the operator and its explicit Python runtime too. LaunchAgents run
    # wrappers which reverify pins/configs, not an unguarded mutable executable.
    for file in [Path(__file__).absolute(), Path(files.__file__).absolute(), Path(__file__).with_name("onboarding_native.py").absolute(), Path(sys.executable).resolve(strict=True)]:
        pins.append({"path": str(file), "sha256": files.sha(file), "kind": "data"})
    if not isinstance(source["areas"], list) or not 1 <= len(source["areas"]) <= 3:
        raise Refused("one to three exact local areas required")
    seen, instances, ports = set(), set(), {port(source["pg_port"])}
    for area in source["areas"]:
        exact(area, {"name", "instance_id", "api_port"})
        if area["name"] not in AREAS or area["name"] in seen:
            raise Refused("duplicate/remote-only area")
        identifier(area["instance_id"])
        if area["instance_id"] in instances:
            raise Refused("each local area requires a distinct instance identity")
        instances.add(area["instance_id"])
        if port(area["api_port"]) in ports:
            raise Refused("cluster and area ports must be distinct")
        seen.add(area["name"])
        ports.add(area["api_port"])
    if not isinstance(source["subscriptions"], list) or not 1 <= len(source["subscriptions"]) <= 8:
        raise Refused("one to eight reviewed subscriptions required")
    selections, ids, scopes = [], set(), set()
    # Journey preflight checks installed pins only. Ordinary preparation and
    # every native apply still require actual reviewed subscription receipts.
    for file in ([] if prerequisites_only else source["subscriptions"]):
        file = files.private(file)
        value = files.read(file, 64 * 1024)
        metadata(value)
        scope = value["bootstrap"]["scope"]
        key = tuple(scope[k] for k in ["company", "area", "owner", "project", "space"])
        if value["subscription"] in ids or key in scopes or scope["area"] not in seen:
            raise Refused("duplicate subscription or unselected exact area/project identity")
        ids.add(value["subscription"])
        scopes.add(key)
        selections.append({"path": str(file), "metadata": value, "sha256": files.sha(file)})
    if not prerequisites_only and seen != {s["metadata"]["bootstrap"]["scope"]["area"] for s in selections}:
        raise Refused("each selected local area requires reviewed project subscription metadata")
    if source["schema_version"] == 2:
        if {r["area"] for r in source["repositories"]} != seen:
            raise Refused("repository routing must cover exactly the selected areas")
        if any(s["metadata"].get("replication_mode") != "curated_project_v1" for s in selections):
            raise Refused("writable local workflow requires curated-project subscription profiles")
        if not prerequisites_only and {(r["area"], r["project_id"]) for r in source["repositories"]} != {(s["metadata"]["bootstrap"]["scope"]["area"], s["metadata"]["bootstrap"]["scope"]["project"]) for s in selections}:
            raise Refused("watcher routes must cover exactly the approved device projects")
    if instances & {s["metadata"]["bootstrap"][key] for s in selections for key in ["device", "authority"]} or instances & {s["metadata"]["bootstrap"]["scope"]["owner"] for s in selections}:
        raise Refused("local area identities must not impersonate enrolled devices or remote owners/authorities")
    return {"input": source, "pins": pins, "selections": selections,
            "operator": str(Path(__file__).absolute()), "python": str(Path(sys.executable).resolve(strict=True))}


def inspect(journal):
    value = files.read(journal)
    exact(value, {"schema_version", "request_id", "plan", "plan_sha256", "stages"})
    if value["schema_version"] != 1 or value["plan_sha256"] != files.digest(value["plan"]):
        raise Refused("journal immutable inputs changed")
    identifier(value["request_id"])
    rebuilt = make_plan(value["plan"]["input"])
    if rebuilt != value["plan"]:
        raise Refused("subscription/operator/prerequisite input changed; review a new plan, never overwrite")
    root = Path(value["plan"]["input"]["root"])
    for file in [Path(journal), Path(str(journal) + ".codifix-lock")]:
        files.outside(file, root)
    return value


def prepare(source, journal, native):
    source = files.read(source, 64 * 1024)
    plan = make_plan(source)
    native.preflight(plan)
    journal = files.path(journal)
    for file in [journal, Path(str(journal) + ".codifix-lock")]:
        files.outside(file, Path(source["root"]))
    with files.Lock(journal):
        if journal.exists():
            existing = inspect(journal)
            if existing["plan"] != plan:
                raise Refused("prepared inputs conflict; no replacement")
            return existing
        if Path(source["root"]).exists():
            raise Refused("operator root already exists; preview never adopts user installations")
        value = {"schema_version": 1, "request_id": str(uuid.uuid4()), "plan": plan,
                 "plan_sha256": files.digest(plan), "stages": {}}
        files.missing(journal, files.encoded(value))
        return value


def stage(journal, value, name, operation, *, always=False):
    if value["stages"].get(name) == "done" and not always:
        return
    value["stages"][name] = "intent"
    files.save(journal, value)
    operation()
    value["stages"][name] = "done"
    files.save(journal, value)


def state_root(value):
    return Path(value["plan"]["input"]["root"])


def secrets_at(value):
    result = files.read(state_root(value) / "local-secrets.json")
    exact(result, {"admin", "areas"})
    if not re.fullmatch("[a-f0-9]{64}", result["admin"]) or set(result["areas"]) != {a["name"] for a in value["plan"]["input"]["areas"]}:
        raise Refused("local credential receipt invalid; no regeneration")
    for secret in result["areas"].values():
        exact(secret, {"password", "api_token"})
        if any(not re.fullmatch("[a-f0-9]{64}", s) for s in secret.values()):
            raise Refused("local credential receipt invalid; no regeneration")
    return result


def plist(value, label, arguments, interval=None):
    root = state_root(value)
    job = {"Label": label, "ProgramArguments": arguments, "RunAtLoad": True,
           "WorkingDirectory": str(root), "EnvironmentVariables": clean_env(),
           "StandardOutPath": str(root / "log" / (label + ".stdout.log")),
           "StandardErrorPath": str(root / "log" / (label + ".stderr.log")), "Umask": 0o077}
    if interval:
        job["StartInterval"] = interval
    else:
        job["KeepAlive"] = True
        job["ThrottleInterval"] = 10
    if ".sync." in label:
        # macOS attributes a launchd agent's LAN access to this visible app.
        # It requests consent without ever reading device/server credentials.
        job["AssociatedBundleIdentifiers"] = ["dev.codifix.network-access"]
        for application in (Path("/Applications/Codifix Network Access.app"), Path.home() / "Applications/Codifix Network Access.app"):
            helper = application / "Contents/MacOS/codifix-network-access"
            if helper.is_file() and value["plan"]["input"]["schema_version"] == 2:
                # Keep the granted, bundled app responsible for the child.
                # Retain the complete verified run-sync wrapper under the app.
                # Schema-v1 StartInterval jobs keep their original lifecycle.
                job["ProgramArguments"] = [str(helper), "--sync", "--python", value["plan"]["python"],
                    "--python-sha256", files.sha(value["plan"]["python"]), "--operator", value["plan"]["operator"],
                    "--operator-sha256", files.sha(value["plan"]["operator"]), *arguments[-4:]]
                break
    return plistlib.dumps(job, sort_keys=True)


def job(value, native, kind, identifier, arguments, interval=None):
    label = f"dev.codifix.onboarding.{value['request_id']}.{kind}.{identifier}"
    directory = files.path(Path.home() / "Library/LaunchAgents")
    if not directory.exists():
        # Called only from confirmed apply. A clean Mac may not have any user
        # jobs yet; create that missing directory, never repair existing modes.
        files.directory(directory)
    if not directory.is_dir():
        raise Refused("user Library/LaunchAgents path is not a directory; existing path preserved")
    file = directory / (label + ".plist")
    files.missing(file, plist(value, label, arguments, interval))
    if not native.loaded(label):
        if kind == "postgres":
            native.available(value["plan"]["input"]["pg_port"])
        elif kind == "area":
            native.available(next(a["api_port"] for a in value["plan"]["input"]["areas"] if a["name"] == identifier))
        native.bootstrap(file)


def wrapper(value, *args):
    return [value["plan"]["python"], "-I", "-S", "-B", value["plan"]["operator"], *args]


def area_config(value, area, secret):
    root = state_root(value)
    name = area["name"]
    url = database_url(value, name, secret["password"])
    cache = ""
    for selected in value["plan"]["selections"]:
        metadata = selected["metadata"]
        scope = metadata["bootstrap"]["scope"]
        if scope["area"] != name:
            continue
        cache += "\n[[laptop_cache.subscriptions]]\n"
        if metadata.get("replication_mode"):
            cache += "replication_mode = " + json.dumps(metadata["replication_mode"]) + "\n"
        for key, identifier in {
            "subscription_id": metadata["subscription"],
            "device_id": metadata["bootstrap"]["device"],
            "company_id": scope["company"],
            "owner_instance_id": scope["owner"],
            "project_id": scope["project"],
            "space_id": scope["space"],
        }.items():
            cache += key + " = " + json.dumps(identifier) + "\n"
    # Full embedded service applies its existing migration inventory. These new
    # configs never overlay/discover legacy config and enable no paid automation.
    return f'''{cache}
[area]
name = "{name}"
instance_id = "{area['instance_id']}"
[service]
bind_addr = "127.0.0.1:{area['api_port']}"
api_token = "{secret['api_token']}"
read_only = {str(value['plan']['input']['schema_version'] == 1).lower()}
[database]
url = {json.dumps(url)}
[background_work]
enabled = false
internal_automation = false
[reinforcement]
enabled = false
validation_enabled = false
[consolidation]
enabled = false
[provenance]
reverify_enabled = false
[procedural]
enabled = false
[llm]
provider = "openai_compatible"
base_url = "http://127.0.0.1:1"
api_key_env = ""
model = ""
[cluster]
enabled = false
service_id = "codifix-onboarding-{area['instance_id']}"
[mcp]
enabled = false
http_enabled = false
[automation]
enabled = {str(value['plan']['input']['schema_version'] == 2).lower()}
[embeddings]
enabled = false
create_enabled = false
[features]
llm_curation = false
[offline]
enabled = {str(value['plan']['input']['schema_version'] == 2).lower()}
path = {json.dumps(str(root / (name + '.offline.sqlite')), ensure_ascii=False)}
'''.encode()


def routing(value):
    root = state_root(value)
    areas = {a["name"]: {"instance_id": a["instance_id"], "config": str(root / (a["name"] + ".toml"))} for a in value["plan"]["input"]["areas"]}
    # Unknown repositories must not fall into another selected area's database.
    areas.setdefault("general", {"instance_id": value["request_id"], "config": None})
    return {"version": 1, "default_area": "general", "areas": areas,
            "routes": [{k: r[k] for k in ["area", "repository", "project"]} for r in value["plan"]["input"]["repositories"]]}


def client_launcher(value):
    command = [value["plan"]["input"]["codifix"]["path"], "--area-routing", str(state_root(value) / "area-routing.json")]
    return ("#!/bin/sh\nexec " + " ".join(shlex.quote(a) for a in command) + ' "$@"\n').encode()


def database_url(value, area, password):
    return f"postgresql://memory_laptop_{area}:{password}@127.0.0.1:{value['plan']['input']['pg_port']}/memory_laptop_{area}"


def local_profile(value, metadata, secret):
    profile = dict(metadata)
    area = metadata["bootstrap"]["scope"]["area"]
    profile["local_database_url"] = database_url(value, area, secret[area]["password"])
    if metadata.get("replication_mode") == "curated_project_v1":
        routes = [r for r in value["plan"]["input"]["repositories"] if r["area"] == area and r["project_id"] == metadata["bootstrap"]["scope"]["project"]]
        if len(routes) != 1:
            raise Refused("curated subscription must have one exact cloned repository binding")
        profile["local_project"] = {"slug": routes[0]["project"], "repository": routes[0]["repository"]}
    return profile


def verify_files(value):
    root = state_root(value)
    receipt = {"request_id": value["request_id"], "plan_sha256": value["plan_sha256"]}
    if files.read(root / "receipt.json") != receipt:
        raise Refused("operator root receipt changed; no adoption/reinitialization")
    secret = secrets_at(value)
    if value["plan"]["input"]["schema_version"] == 2 and files.read(root / "area-routing.json") != routing(value):
        raise Refused("selected repository routing changed; no fallback")
    if value["plan"]["input"]["schema_version"] == 2 and files.bytes_at(root / "bin/codifix") != client_launcher(value):
        raise Refused("owned routed client changed; preserve and inspect")
    for area in value["plan"]["input"]["areas"]:
        if files.bytes_at(root / (area["name"] + ".toml")) != area_config(value, area, secret["areas"][area["name"]]):
            raise Refused("owned area settings changed; preserve and inspect")
    for selection in value["plan"]["selections"]:
        profile = local_profile(value, selection["metadata"], secret["areas"])
        if files.read(root / "profiles" / (profile["subscription"] + ".json"), 64 * 1024) != profile:
            raise Refused("private replica profile changed; no fallback")
    expected = {"pg_hba.conf": b"local all all reject\nhost all all 127.0.0.1/32 scram-sha-256\nhost all all 0.0.0.0/0 reject\nhost all all ::0/0 reject\n",
                "postgresql.conf": b"listen_addresses = '127.0.0.1'\npassword_encryption = 'scram-sha-256'\nshared_preload_libraries = ''\n",
                "admin-password": (secret["admin"] + "\n").encode()}
    all_roles = {"memory_onboarding_admin": secret["admin"], **{f"memory_laptop_{name}": s["password"] for name, s in secret["areas"].items()}}
    for role, password in all_roles.items():
        expected[role + ".pgpass"] = f"127.0.0.1:{value['plan']['input']['pg_port']}:*:{role}:{password}\n".encode()
    if any(files.bytes_at(root / name) != content for name, content in expected.items()):
        raise Refused("owned PostgreSQL settings/credential file changed; preserve and inspect")


def apply(journal, digest, confirm, native):
    if not confirm:
        raise Refused("exact plan SHA-256 and --confirm required before native effects")
    value = inspect(journal)
    if digest != value["plan_sha256"]:
        raise Refused("confirmation differs from immutable preview")
    native.preflight(value["plan"])
    with files.Lock(journal):
        value = inspect(journal)
        if digest != value["plan_sha256"]:
            raise Refused("confirmed plan changed before lock acquisition; no native effects")
        native.review(value["plan"])  # Fresh Keychain/current membership, no company DB credentials.
        if inspect(journal) != value:
            raise Refused("reviewed metadata changed during authority check")
        root = state_root(value)
        plan = value["plan"]
        def layout():
            if root.exists() and not (root / "receipt.json").exists() and any(root.iterdir()):
                raise Refused("unrecognized existing operator root; preserve it")
            files.directory(root)
            files.missing(root / "receipt.json", files.encoded({"request_id": value["request_id"], "plan_sha256": value["plan_sha256"]}))
            for name in ["log", "profiles"]:
                files.directory(root / name)
            secrets_file = root / "local-secrets.json"
            if not secrets_file.exists():
                secret = {"admin": secrets.token_hex(32), "areas": {a["name"]: {"password": secrets.token_hex(32), "api_token": secrets.token_hex(32)} for a in plan["input"]["areas"]}}
                files.missing(secrets_file, files.encoded(secret))
            secret = secrets_at(value)
            files.missing(root / "admin-password", (secret["admin"] + "\n").encode())
            all_roles = {"memory_onboarding_admin": secret["admin"], **{f"memory_laptop_{name}": s["password"] for name, s in secret["areas"].items()}}
            for role, password in all_roles.items():
                files.missing(root / (role + ".pgpass"), f"127.0.0.1:{plan['input']['pg_port']}:*:{role}:{password}\n".encode())
            files.missing(root / "pg_hba.conf", b"local all all reject\nhost all all 127.0.0.1/32 scram-sha-256\nhost all all 0.0.0.0/0 reject\nhost all all ::0/0 reject\n")
            files.missing(root / "postgresql.conf", b"listen_addresses = '127.0.0.1'\npassword_encryption = 'scram-sha-256'\nshared_preload_libraries = ''\n")
            for area in plan["input"]["areas"]:
                files.missing(root / (area["name"] + ".toml"), area_config(value, area, secret["areas"][area["name"]]))
            if plan["input"]["schema_version"] == 2:
                files.missing(root / "area-routing.json", files.encoded(routing(value)))
                files.directory(root / "bin")
                files.missing(root / "bin/codifix", client_launcher(value))
                (root / "bin/codifix").chmod(0o700)
            for selection in plan["selections"]:
                profile = local_profile(value, selection["metadata"], secret["areas"])
                files.missing(root / "profiles" / (profile["subscription"] + ".json"), files.encoded(profile))
        stage(journal, value, "layout", layout)
        verify_files(value)
        def cluster():
            cluster = root / "cluster"
            marker = cluster / "PG_VERSION"
            if not marker.exists():
                if cluster.exists() and any(cluster.iterdir()):
                    raise Refused("partial initdb has no receipt/version; inspect retained data, never clear or reinitialize")
                native.command([Path(plan["input"]["postgres"]["bin_dir"]) / "initdb", "-D", cluster,
                                "--username=memory_onboarding_admin", "--auth-local=reject", "--auth-host=scram-sha-256",
                                "--encoding=UTF8", "--no-locale", "--no-clean", "--pwfile", root / "admin-password"], timeout=120)
            if files.bytes_at(marker, 16).decode().strip() != str(plan["input"]["postgres"]["major"]):
                raise Refused("cluster major differs; no upgrade/reset")
            native.command([Path(plan["input"]["postgres"]["bin_dir"]) / "pg_controldata", "-D", cluster])
        stage(journal, value, "cluster", cluster)
        # Even done cluster stages recheck their immutable version before starts.
        if files.bytes_at(root / "cluster/PG_VERSION", 16).decode().strip() != str(plan["input"]["postgres"]["major"]):
            raise Refused("cluster replaced; no native effects")
        stage(journal, value, "postgres_agent", lambda: job(value, native, "postgres", "cluster", wrapper(value, "run-postgres", "--journal", str(journal))), always=True)
        native.wait(plan, root, "postgres", f"SELECT current_setting('data_directory') = '{root / 'cluster'}' AND current_setting('listen_addresses') = '127.0.0.1';")
        secret = secrets_at(value)
        for area in plan["input"]["areas"]:
            name = area["name"]
            role = "memory_laptop_" + name
            def database():
                password = secret["areas"][name]["password"]
                sql = f"DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='{role}') THEN CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION; END IF; END $$;\nSELECT 'CREATE DATABASE {role} OWNER {role}' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname='{role}') \\gexec\n"
                native.sql(plan, root, "postgres", sql)
                native.wait(plan, root, role, "SELECT current_user = '" + role + "';", role=role)
                native.sql(plan, root, role, f"REVOKE CONNECT ON DATABASE {role} FROM PUBLIC; CREATE EXTENSION IF NOT EXISTS vector VERSION '{plan['input']['postgres']['vector_version']}';")
                native.wait(plan, root, role, f"SELECT extversion = '{plan['input']['postgres']['vector_version']}' FROM pg_extension WHERE extname='vector';")
            stage(journal, value, "database:" + name, database)
            native.wait(plan, root, role, "SELECT current_user = '" + role + "';", role=role)
            native.wait(plan, root, "postgres", f"SELECT NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole AND NOT rolreplication FROM pg_roles WHERE rolname='{role}';")
            native.wait(plan, root, "postgres", f"SELECT pg_get_userbyid(datdba) = '{role}' FROM pg_database WHERE datname='{role}';")
            stage(journal, value, "area_agent:" + name, lambda: job(value, native, "area", name, wrapper(value, "run-service", "--journal", str(journal), "--area", name)), always=True)
            native.wait(plan, root, role, "SELECT to_regclass('onboarding_project_projections') IS NOT NULL;", role=role)
            native.health(area)
        for selection in plan["selections"]:
            subscription = selection["metadata"]["subscription"]
            if plan["input"]["schema_version"] == 2:
                stage(journal, value, "initial_sync:" + subscription, lambda: native.command([plan["input"]["codifix"]["path"], "projects", "sync", "--profile", str(root / "profiles" / (subscription + ".json"))], timeout=120))
            interval = None if plan["input"]["schema_version"] == 2 else 300
            stage(journal, value, "sync_agent:" + subscription, lambda: job(value, native, "sync", subscription, wrapper(value, "run-sync", "--journal", str(journal), "--subscription", subscription), interval), always=True)
        if plan["input"]["schema_version"] == 2:
            for index, route in enumerate(plan["input"]["repositories"]):
                stage(journal, value, "watcher_agent:" + str(index), lambda index=index: job(value, native, "watcher", str(index), wrapper(value, "run-watcher", "--journal", str(journal), "--repository-index", str(index))), always=True)
        return value


def run_job(journal, mode, selected, native):
    value = inspect(journal)
    native.preflight(value["plan"])
    verify_files(value)
    root = state_root(value)
    plan = value["plan"]
    if mode == "run-postgres":
        if files.bytes_at(root / "cluster/PG_VERSION", 16).decode().strip() != str(plan["input"]["postgres"]["major"]):
            raise Refused("cluster changed; no initialization or restart")
        args = [str(Path(plan["input"]["postgres"]["bin_dir"]) / "postgres"), "-D", str(root / "cluster"), "-p", str(plan["input"]["pg_port"]), "-k", "", "-c", "listen_addresses=127.0.0.1", "-c", "config_file=" + str(root / "postgresql.conf"), "-c", "hba_file=" + str(root / "pg_hba.conf"), "-c", "shared_preload_libraries=", "-c", "session_preload_libraries=", "-c", "local_preload_libraries="]
    elif mode == "run-service":
        if selected not in {a["name"] for a in plan["input"]["areas"]}:
            raise Refused("area not in exact prepared plan")
        args = [plan["input"]["codifix"]["path"], "--config", str(root / (selected + ".toml")), "service", "run"]
    elif mode == "run-watcher":
        if plan["input"]["schema_version"] != 2:
            raise Refused("legacy read-only onboarding cannot activate watchers")
        if not isinstance(selected, str) or not selected.isdigit() or int(selected) >= len(plan["input"]["repositories"]):
            raise Refused("watcher repository is not in exact selected plan")
        route = plan["input"]["repositories"][int(selected)]
        args = [plan["input"]["codifix"]["path"], "--config", str(root / (route["area"] + ".toml")), "watcher", "run", "--project", route["project"], "--repo-root", route["repository"]]
    else:
        if selected not in {s["metadata"]["subscription"] for s in plan["selections"]}:
            raise Refused("subscription not in exact prepared plan")
        args = [plan["input"]["codifix"]["path"], "projects", "sync", "--profile", str(root / "profiles" / (selected + ".json"))]
        if plan["input"]["schema_version"] == 2:
            args.append("--watch")
    os.execve(args[0], args, clean_env())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    preview = commands.add_parser("prepare")
    preview.add_argument("--input", required=True, type=Path)
    preview.add_argument("--journal", required=True, type=Path)
    apply_parser = commands.add_parser("apply")
    apply_parser.add_argument("--journal", required=True, type=Path)
    apply_parser.add_argument("--plan-sha256", required=True)
    apply_parser.add_argument("--confirm", action="store_true")
    commands.add_parser("status").add_argument("--journal", required=True, type=Path)
    journey = commands.add_parser("journey")
    journey.add_argument("--input", required=True, type=Path)
    journey.add_argument("--journal", required=True, type=Path)
    commands.add_parser("journey-status").add_argument("--journal", required=True, type=Path)
    first = commands.add_parser("first-run", help="sign in and select projects using a reviewed bundled runtime; no installer JSON needed")
    first.add_argument("--runtime", required=True, type=Path, help="exact installed runtime-prerequisites.json")
    first.add_argument("--codifix", type=Path, default=Path("/usr/local/bin/codifix"))
    first.add_argument("--company", required=True)
    first.add_argument("--name", required=True)
    first.add_argument("--state", required=True, type=Path, help="private new/resumable state outside repositories")
    first.add_argument("--workspace-parent", required=True, type=Path, help="existing repository parent; existing repositories are preserved")
    first.add_argument("--pg-port", type=int, default=55432)
    first.add_argument("--api-base-port", type=int, default=55440)
    first.add_argument("--no-browser", action="store_true")
    first.add_argument("--allow-loopback-development", action="store_true")
    first.add_argument("--prepare-only", action="store_true", help="save selected inputs without cloning or starting local services")
    first.add_argument("--curated-sync", action="store_true", help="review full curated project replication and local capture; requires compatible device contributions support")
    agents = commands.add_parser("journey-agents")
    agents.add_argument("--journal", required=True, type=Path)
    agents.add_argument("--allow-offline", action="store_true", help="explicit bounded cached model-disclosure choice; observed denial always fences it")
    agents.add_argument("--renew-grant", help="explicitly replace this denied/expired agent UUID; retains history and requires fresh browser approval")
    for name, option in [("run-postgres", None), ("run-service", "--area"), ("run-sync", "--subscription"), ("run-watcher", "--repository-index")]:
        child = commands.add_parser(name)
        child.add_argument("--journal", required=True, type=Path)
        if option:
            child.add_argument(option, required=True)
    args = parser.parse_args()
    try:
        if args.command == "first-run":
            import onboarding_first_run
            onboarding_first_run.run(company=args.company, name=args.name, runtime=args.runtime, memory=args.codifix,
                state=args.state, workspace_parent=args.workspace_parent, native=Native(), no_browser=args.no_browser,
                allow_loopback=args.allow_loopback_development, pg_port=args.pg_port, api_base_port=args.api_base_port,
                prepare_only=args.prepare_only, curated_sync=args.curated_sync)
            return
        if args.command == "journey-agents":
            import onboarding_agents
            onboarding_agents.run(args.journal, Native(), allow_offline=args.allow_offline, renew_grant=args.renew_grant)
            return
        if args.command in {"journey", "journey-status"}:
            import onboarding_journey
            if args.command == "journey":
                onboarding_journey.run(args.input, args.journal, Native())
            else:
                print(json.dumps(onboarding_journey.status(args.journal), indent=2, sort_keys=True))
            return
        if args.command == "prepare":
            value = prepare(args.input, args.journal, Native())
        elif args.command == "apply":
            value = apply(args.journal, args.plan_sha256, args.confirm, Native())
        elif args.command == "status":
            value = inspect(args.journal)
        else:
            run_job(args.journal, args.command, getattr(args, "area", getattr(args, "subscription", getattr(args, "repository_index", None))), Native())
            return
        print(json.dumps(value, indent=2, sort_keys=True))
        print("Metadata/stages only: native Mac acceptance, signed prerequisites and self-contained distribution remain unverified. Agent identity provisioning is not performed.")
    except KeyboardInterrupt:
        print("operator interrupted; retain journals and resume exact inputs (output suppressed)", file=sys.stderr)
        raise SystemExit(130) from None
    except (Refused, OSError, ValueError, KeyError, TypeError) as error:
        # Never include raw file/provider/process bodies or local passwords.
        print(str(error) if isinstance(error, Refused) else "operator input/filesystem invalid; inspect retained journal (details suppressed)", file=sys.stderr)
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
