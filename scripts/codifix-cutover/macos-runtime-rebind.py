#!/usr/bin/env python3
"""Codifix 3.0.0-rc.1 cold Mac runtime handover; never provision, sync or watch.

Run from the reviewed protected candidate tree with Python -I -S -B. The old
onboarding operator remains strict. Its archived journal is evidence, not input
to apply/replay. Only bind writes new managed files; all plists remain inert.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import sys
import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "packaging/macos"))
import onboard as onboarding
from onboarding_files import Refused

files = onboarding.files
FORMAT = "codifix-3.0-macos-runtime-rebind-v1"
INPUT = FORMAT + "-input"
RECONCILIATION = FORMAT + "-reconciliation"
CONTROL = ".codifix-runtime-3.0.0-rc.1"
MAXIMUM = 2 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise Refused(message)


def exact(value, keys):
    require(isinstance(value, dict) and set(value) == set(keys), "unexpected/missing runtime handover fields")


def private_pin(value):
    exact(value, {"path", "sha256"})
    path = files.private(value["path"])
    require(files.sha(path) == onboarding.pin(value["sha256"]), "archived/reconciliation artifact changed")
    return path


def read_pin(value):
    return files.read(private_pin(value), MAXIMUM)


def positive_oid(value):
    require(type(value) is int and 1 <= value <= 2**32 - 1, "reviewed PostgreSQL OID required")
    return value


def archive(source):
    exact(source["archive"], {"journal", "root_receipt"})
    journal = read_pin(source["archive"]["journal"])
    exact(journal, {"schema_version", "request_id", "plan", "plan_sha256", "stages"})
    require(journal["schema_version"] == 1 and journal["plan_sha256"] == files.digest(journal["plan"]),
            "archived journal plan/hash differs")
    onboarding.identifier(journal["request_id"])
    plan = journal["plan"]
    exact(plan, {"input", "pins", "selections", "operator", "python"})
    old = plan["input"]
    require(old.get("schema_version") in {1, 2} and old.get("root") == source["old_root"],
            "archived schema/root mapping differs")
    require(isinstance(plan["pins"], list) and 1 <= len(plan["pins"]) <= 256,
            "archived prerequisite pins missing")
    require(isinstance(plan["selections"], list) and 1 <= len(plan["selections"]) <= 8,
            "archived selected project bindings required")
    seen = set()
    for selected in plan["selections"]:
        exact(selected, {"path", "metadata", "sha256"})
        onboarding.pin(selected["sha256"])
        onboarding.metadata(selected["metadata"])
        identity = selected["metadata"]["subscription"]
        require(identity not in seen, "duplicate archived subscription")
        seen.add(identity)
    areas = old.get("areas")
    require(isinstance(areas, list) and 1 <= len(areas) <= 3, "archived areas missing")
    expected = {"layout", "cluster", "postgres_agent"}
    for area in areas:
        expected.update({"database:" + area["name"], "area_agent:" + area["name"]})
    for subscription in seen:
        expected.add("sync_agent:" + subscription)
        if old["schema_version"] == 2:
            expected.add("initial_sync:" + subscription)
    if old["schema_version"] == 2:
        require(isinstance(old.get("repositories"), list), "archived repository routes missing")
        expected.update("watcher_agent:" + str(i) for i in range(len(old["repositories"])))
    require(journal["stages"] == {key: "done" for key in expected},
            "archived onboarding was incomplete or has unknown stages; never forge/replay stages")
    receipt = read_pin(source["archive"]["root_receipt"])
    require(receipt == {"request_id": journal["request_id"], "plan_sha256": journal["plan_sha256"]},
            "archived root receipt differs from original journal")
    return journal


def old_jobs(source, journal):
    request = journal["request_id"]
    old = journal["plan"]["input"]
    suffixes = {"postgres.cluster"} | {"area." + a["name"] for a in old["areas"]}
    suffixes.update("sync." + s["metadata"]["subscription"] for s in journal["plan"]["selections"])
    if old["schema_version"] == 2:
        suffixes.update("watcher." + str(i) for i in range(len(old["repositories"])))
    labels = {"com.memory-layer.onboarding." + request + "." + suffix for suffix in suffixes}
    require(isinstance(source["old_jobs"], list) and len(source["old_jobs"]) == len(labels),
            "exact original job inventory required")
    observed = set()
    for job in source["old_jobs"]:
        exact(job, {"label", "prior_state", "plist"})
        require(job["label"] in labels and job["label"] not in observed
                and job["prior_state"] in {"active", "inactive", "failed"}, "original job binding/state differs")
        plist = private_pin(job["plist"])
        require(plistlib.loads(files.bytes_at(plist))["Label"] == job["label"], "archived job label differs")
        observed.add(job["label"])


def reconciliation(source, journal):
    result = read_pin(source["reconciliation"])
    exact(result, {"format", "cluster_system_identifier", "admin_role_oid", "preservation_receipt_sha256", "areas"})
    require(result["format"] == RECONCILIATION and isinstance(result["cluster_system_identifier"], str)
            and re.fullmatch(r"[0-9]{10,20}", result["cluster_system_identifier"]), "reviewed cluster identity required")
    require(positive_oid(result["admin_role_oid"]) == 10, "original bootstrap administrator OID required")
    onboarding.pin(result["preservation_receipt_sha256"])
    areas = journal["plan"]["input"]["areas"]
    require(isinstance(result["areas"], list) and len(result["areas"]) == len(areas), "exact reconciled areas required")
    observed = set()
    for area in result["areas"]:
        exact(area, {"name", "database_oid", "role_oid", "project_ids", "migrations_sha256"})
        require(area["name"] in {a["name"] for a in areas} and area["name"] not in observed,
                "reconciled area differs")
        positive_oid(area["database_oid"])
        positive_oid(area["role_oid"])
        require(isinstance(area["project_ids"], list) and len(area["project_ids"]) <= 64
                and area["project_ids"] == sorted(set(area["project_ids"])), "exact sorted local project IDs required")
        for identifier in area["project_ids"]:
            onboarding.identifier(identifier)
        onboarding.pin(area["migrations_sha256"])
        if journal["plan"]["input"]["schema_version"] == 2:
            expected = {r["project_id"] for r in journal["plan"]["input"]["repositories"] if r["area"] == area["name"]}
            require(expected <= set(area["project_ids"]), "original local project identity missing from reconciliation")
        observed.add(area["name"])
    return result


def credentials(source, journal, runtime):
    root = Path(source["new_root"])
    # The cold SQL/filesystem procedure must already have copied/renamed these.
    require(files.bytes_at(root / "receipt.json") == files.bytes_at(private_pin(source["archive"]["root_receipt"])),
            "populated root receipt changed; no adoption/reinitialization")
    secret = files.read(root / "local-secrets.json")
    exact(secret, {"admin", "areas"})
    names = {a["name"] for a in runtime["input"]["areas"]}
    require(set(secret["areas"]) == names and re.fullmatch(r"[a-f0-9]{64}", secret["admin"]),
            "existing local credentials missing; never regenerate")
    preserved = [root / "receipt.json", root / "local-secrets.json", root / "admin-password"]
    require(files.bytes_at(root / "admin-password") == (secret["admin"] + "\n").encode(), "existing administrator secret differs")
    roles = {"codifix_onboarding_admin": secret["admin"]}
    for name, value in secret["areas"].items():
        exact(value, {"password", "api_token"})
        require(all(isinstance(v, str) and re.fullmatch(r"[a-f0-9]{64}", v) for v in value.values()),
                "existing area credentials invalid")
        roles["codifix_laptop_" + name] = value["password"]
    for role, password in roles.items():
        pgpass = root / (role + ".pgpass")
        expected = f"127.0.0.1:{runtime['input']['pg_port']}:*:{role}:{password}\n".encode()
        require(files.bytes_at(pgpass) == expected, "completed exact pgpass rename/credential reconciliation required")
        preserved.append(pgpass)
    for area in runtime["input"]["areas"]:
        config_file = root / (area["name"] + ".toml")
        config = tomllib.loads(files.bytes_at(config_file).decode())
        expected_url = onboarding.database_url({"plan": runtime}, area["name"], secret["areas"][area["name"]]["password"])
        require(config.get("area") == {"name": area["name"], "instance_id": area["instance_id"]}
                and config.get("database", {}).get("url") == expected_url
                and config.get("service", {}).get("api_token") == secret["areas"][area["name"]]["api_token"]
                and config["service"].get("bind_addr") == f"127.0.0.1:{area['api_port']}",
                "completed area configuration/identity rename required")
        preserved.append(config_file)
    for selection in journal["plan"]["selections"]:
        metadata = selection["metadata"]
        profile_file = root / "profiles" / (metadata["subscription"] + ".json")
        profile = files.read(profile_file)
        name = metadata["bootstrap"]["scope"]["area"]
        require(all(profile.get(key) == value for key, value in metadata.items())
                and profile.get("local_database_url") == onboarding.database_url({"plan": runtime}, name, secret["areas"][name]["password"]),
                "completed replica profile rename/identity reconciliation required")
        preserved.append(profile_file)
    cluster = files.path(root / "cluster")
    require(cluster.is_dir(), "populated cluster directory missing")
    files.directory(cluster)  # Existing directory validation only.
    require(files.bytes_at(cluster / "PG_VERSION", 16).decode().strip() == str(runtime["input"]["postgres"]["major"]),
            "populated cluster major differs; never initialize or upgrade")
    auto = cluster / "postgresql.auto.conf"
    require(not any(line.strip() and not line.lstrip().startswith("#") for line in files.bytes_at(auto).decode().splitlines()),
            "unexpected ALTER SYSTEM configuration; review separately")
    preserved.extend([cluster / "PG_VERSION", auto])
    return secret, [{"path": str(path), "sha256": files.sha(files.private(path))} for path in preserved]


def build_payload(source):
    bundle = Path(__file__).absolute().parents[2] / "packaging/macos"
    require(Path(onboarding.__file__).absolute() == bundle / "onboard.py"
            and Path(files.__file__).absolute() == bundle / "onboarding_files.py"
            and Path(sys.modules[onboarding.Native.__module__].__file__).absolute() == bundle / "onboarding_native.py",
            "run only from the retained protected candidate operator bundle")
    exact(source, {"format", "old_root", "new_root", "archive", "candidate", "postgres", "reconciliation", "old_jobs"})
    require(source["format"] == INPUT, "release-specific input format required")
    old_root, root = files.path(source["old_root"]), files.path(source["new_root"])
    require(old_root != root and not old_root.exists(), "cold root move incomplete; old root must be absent")
    require(root.is_dir(), "already populated destination root required")
    files.directory(root)
    files.outside(old_root, root)
    for artifact in [*source["archive"].values(), source["reconciliation"]]:
        files.outside(private_pin(artifact), root)
    journal = archive(source)
    original_pg = journal["plan"]["input"]["postgres"]
    require(source["postgres"].get("major") == original_pg.get("major")
            and source["postgres"].get("vector_version") == original_pg.get("vector_version"),
            "runtime rebind cannot upgrade PostgreSQL or pgvector")
    old_jobs(source, journal)
    reconciled = reconciliation(source, journal)
    current = copy.deepcopy(journal["plan"]["input"])
    current.pop("memory", None)
    current.update(root=str(root), codifix=source["candidate"], postgres=source["postgres"])
    require(current["codifix"].get("version") in {"codifix 3.0.0-rc.1", "codifix 3.0.0"}, "reviewed 3.0 candidate version required")
    # Only construct/check current prerequisite pins. Never inspect/apply the old
    # journal, request authority, create subscriptions or execute its old stages.
    runtime = onboarding.make_plan(current, prerequisites_only=True)
    runtime["pins"].append({"path": str(Path(__file__).absolute()), "sha256": files.sha(Path(__file__).absolute()), "kind": "data"})
    secret, preserved = credentials(source, journal, runtime)
    payload = {"format": FORMAT, "input": source, "request_id": journal["request_id"],
               "runtime": runtime, "cache": [s["metadata"] for s in journal["plan"]["selections"]],
               "reconciliation": reconciled, "preserved_files": preserved}
    payload["outputs"] = {name: hashlib.sha256(content).hexdigest() for name, content in outputs(payload, secret).items()}
    return payload


def control(payload):
    return Path(payload["input"]["new_root"]) / CONTROL


def outputs(payload, secret):
    runtime = payload["runtime"]
    root, managed = Path(runtime["input"]["root"]), control(payload)
    # Start from the existing isolated config generator, then override schema-2
    # execution and migration policy explicitly. Never overlay user/global TOML.
    config_value = {"plan": {**runtime, "selections": [{"metadata": m} for m in payload["cache"]]}}
    result = {}
    for area in runtime["input"]["areas"]:
        data = onboarding.area_config(config_value, area, secret["areas"][area["name"]]).decode()
        data = data.replace("read_only = false", "read_only = true")
        data = data.replace("[database]\n", '[database]\nmigrations = "verify_only"\n')
        data = data.replace("[automation]\nenabled = true", "[automation]\nenabled = false")
        data = data.replace("[offline]\nenabled = true", "[offline]\nenabled = false")
        data = data.replace("[mcp]\n", "[mcp]\nread_only = true\nrequire_token = true\n")
        data += '\n[auth]\nmode = "single_user"\n[telemetry]\nenabled = false\n'
        result["configs/" + area["name"] + ".toml"] = data.encode()
    result["postgresql.conf"] = b"listen_addresses = '127.0.0.1'\npassword_encryption = 'scram-sha-256'\nshared_preload_libraries = ''\n"
    result["pg_hba.conf"] = b"local all all reject\nhost all all 127.0.0.1/32 scram-sha-256\nhost all all 0.0.0.0/0 reject\nhost all all ::0/0 reject\n"
    kinds = [("postgres", None)] + [("area", area["name"]) for area in runtime["input"]["areas"]]
    for kind, area in kinds:
        suffix = kind + "." + (area or "cluster")
        label = "dev.codifix.cutover." + payload["request_id"] + "." + suffix
        arguments = [runtime["python"], "-I", "-S", "-B", str(Path(__file__).absolute()), "run", "--manifest", str(managed / "manifest.json"), "--kind", kind]
        if area:
            arguments += ["--area", area]
        job = {"Label": label, "ProgramArguments": arguments, "WorkingDirectory": str(root),
               "EnvironmentVariables": onboarding.clean_env(), "RunAtLoad": False, "KeepAlive": False,
               "Disabled": True, "Umask": 0o077,
               "StandardOutPath": str(managed / "logs" / (suffix + ".stdout.log")),
               "StandardErrorPath": str(managed / "logs" / (suffix + ".stderr.log"))}
        result["plists/" + suffix + ".plist"] = plistlib.dumps(job, sort_keys=True)
    return result


class Boundary(onboarding.Native):
    def cluster_identity(self, payload):
        pg = payload["runtime"]["input"]["postgres"]
        _, value = self.command([Path(pg["bin_dir"]) / "pg_controldata", "-D", Path(payload["input"]["new_root"]) / "cluster"])
        found = re.findall(r"^Database system identifier:\s*([0-9]+)\s*$", value, re.M)
        require(found == [payload["reconciliation"]["cluster_system_identifier"]], "populated cluster system identifier differs")

    def query(self, payload, database, query, role="codifix_onboarding_admin"):
        script = "BEGIN READ ONLY; SET LOCAL statement_timeout='4000ms'; SET LOCAL lock_timeout='1000ms'; SET LOCAL search_path=pg_catalog;\n" + query + ";\nROLLBACK;"
        value = self.sql(payload["runtime"], Path(payload["input"]["new_root"]), database, script, role=role)
        lines = [line for line in value.splitlines() if line not in {"BEGIN", "SET", "ROLLBACK"}]
        require(len(lines) == 1, "bounded local reconciliation query failed")
        return json.loads(lines[0])

    def database_identity(self, payload):
        runtime, reconciled = payload["runtime"], payload["reconciliation"]
        root = Path(payload["input"]["new_root"])
        identity = self.query(payload, "postgres", "SELECT json_build_object('system_identifier',(SELECT system_identifier::text FROM pg_control_system()),'data_directory',current_setting('data_directory'),'port',current_setting('port')::integer,'admin_oid',(SELECT oid::bigint FROM pg_roles WHERE rolname=current_user),'admin_scram',(SELECT rolsuper AND rolcanlogin AND rolpassword LIKE 'SCRAM-SHA-256$%' FROM pg_authid WHERE rolname=current_user))")
        require(identity == {"system_identifier": reconciled["cluster_system_identifier"], "data_directory": str(root / "cluster"),
                             "port": runtime["input"]["pg_port"], "admin_oid": reconciled["admin_role_oid"], "admin_scram": True},
                "running cluster/administrator identity differs")
        for area in reconciled["areas"]:
            role = "codifix_laptop_" + area["name"]
            actual = self.query(payload, role, "SELECT json_build_object('database_oid',(SELECT oid::bigint FROM pg_database WHERE datname=current_database()),'owner_oid',(SELECT datdba::bigint FROM pg_database WHERE datname=current_database()),'role_oid',(SELECT oid::bigint FROM pg_roles WHERE rolname=current_user),'role_restricted',(SELECT NOT rolsuper AND NOT rolcreatedb AND NOT rolcreaterole AND NOT rolreplication FROM pg_roles WHERE rolname=current_user),'project_ids',COALESCE((SELECT json_agg(id ORDER BY id) FROM public.projects),'[]'::json),'migrations',(SELECT json_agg(json_build_object('version',version,'checksum',encode(checksum,'hex'),'success',success) ORDER BY version) FROM public._sqlx_migrations),'bindings_disabled',NOT EXISTS(SELECT FROM public.laptop_curated_bindings WHERE enabled))", role=role)
            migrations = actual.pop("migrations", None)
            require(isinstance(migrations, list) and [m.get("version") for m in migrations] == list(range(1, 86))
                    and all(m.get("success") is True for m in migrations) and files.digest(migrations) == area["migrations_sha256"],
                    "candidate migrations must already be verified; never migrate")
            require(actual == {"database_oid": area["database_oid"], "owner_oid": area["role_oid"], "role_oid": area["role_oid"],
                               "role_restricted": True, "project_ids": area["project_ids"], "bindings_disabled": True},
                    "populated database/role/project reconciliation differs")
            selected = [m for m in payload["cache"] if m["bootstrap"]["scope"]["area"] == area["name"]]
            for metadata in selected:
                table = "device_replica_projections" if metadata.get("replication_mode") == "curated_project_v1" else "onboarding_project_projections"
                expected = {"subscription": metadata["subscription"], "device": metadata["bootstrap"]["device"], "scope": metadata["bootstrap"]["scope"]}
                projection = self.query(payload, role, "SELECT COALESCE((SELECT json_build_object('subscription',subscription,'device',device,'scope',scope) FROM public." + table + " WHERE subscription='" + metadata["subscription"] + "'::uuid),'null'::json)", role=role)
                require(projection == expected, "historical cache identity not present; no new subscription is created")
        legacy_roles = ["memory_onboarding_admin"] + ["memory_laptop_" + a["name"] for a in reconciled["areas"]]
        legacy = self.query(payload, "postgres", "SELECT EXISTS(SELECT FROM pg_roles WHERE rolname IN (" + ",".join("'" + r + "'" for r in legacy_roles) + "))")
        require(legacy is False, "old database role namespace remains")


def verify_native(payload, native, *, database):
    native.preflight(payload["runtime"])
    native.cluster_identity(payload)
    for job in payload["input"]["old_jobs"]:
        require(not native.loaded(job["label"]), "retired old job is still loaded; no bypass or automatic stop")
    if database:
        native.database_identity(payload)


def inspect(input_file, native):
    payload = build_payload(files.read(input_file, MAXIMUM))
    verify_native(payload, native, database=True)
    return {"payload": payload, "sha256": files.digest(payload)}


def manifest(file):
    sealed = files.read(file, MAXIMUM)
    exact(sealed, {"payload", "sha256"})
    require(sealed["sha256"] == files.digest(sealed["payload"]), "immutable runtime manifest seal differs")
    rebuilt = build_payload(sealed["payload"]["input"])
    require(sealed["payload"] == rebuilt, "reviewed runtime inputs/pins/credentials changed")
    return sealed


def receipt(sealed):
    return {"format": FORMAT + "-bound", "manifest_sha256": sealed["sha256"], "outputs": sealed["payload"]["outputs"]}


def claim(sealed):
    return {"format": FORMAT + "-intent", "manifest_sha256": sealed["sha256"]}


def tree(payload, *, complete):
    root = control(payload)
    allowed_dirs = {"configs", "plists", "logs"}
    allowed = set(payload["outputs"]) | {"manifest.json", "receipt.json"}
    logs = {kind + "." + (name or "cluster") + "." + stream + ".log"
            for kind, name in [("postgres", None)] + [("area", a["name"]) for a in payload["runtime"]["input"]["areas"]]
            for stream in ["stdout", "stderr"]}
    for path in root.rglob("*"):
        name = path.relative_to(root).as_posix()
        files.path(path)
        if path.is_dir():
            require(name in allowed_dirs, "unrecognized managed directory collision")
            files.directory(path)
        else:
            require(name in allowed or (path.parent == root / "logs" and path.name in logs), "unrecognized managed file collision")
            files.private(path)
    if complete:
        require(all((root / name).is_file() for name in allowed), "partial runtime binding cannot run")


def verify_bound(sealed):
    payload, root = sealed["payload"], control(sealed["payload"])
    require(files.read(str(root) + ".intent.json") == claim(sealed), "runtime intent missing/different")
    require(root.is_dir(), "runtime binding incomplete")
    tree(payload, complete=True)
    require(files.bytes_at(root / "manifest.json") == files.encoded(sealed)
            and files.read(root / "receipt.json") == receipt(sealed), "final runtime receipt/manifest missing or changed")
    for name, sha in payload["outputs"].items():
        require(files.sha(root / name) == sha, "bound config/plist changed")


def bind(manifest_file, reviewed_sha256, writers_stopped, native):
    sealed = manifest(manifest_file)
    require(writers_stopped and reviewed_sha256 == sealed["sha256"], "exact reviewed digest and stopped/drained writers required")
    payload, root = sealed["payload"], control(sealed["payload"])
    # One lock per destination, independent of the caller's manifest filename.
    with files.Lock(root):
        sealed = manifest(manifest_file)
        require(reviewed_sha256 == sealed["sha256"], "reviewed manifest changed before lock acquisition")
        verify_native(payload, native, database=True)
        intent = Path(str(root) + ".intent.json")
        if not intent.exists():
            require(not root.exists(), "unrecognized existing managed root; no adoption")
        files.missing(intent, files.encoded(claim(sealed)))
        files.directory(root)
        tree(payload, complete=False)
        # Already published files must match before writing anything else.
        generated = outputs(payload, files.read(Path(payload["input"]["new_root"]) / "local-secrets.json"))
        generated["manifest.json"] = files.encoded(sealed)
        for name, content in generated.items():
            path = root / name
            if path.exists():
                require(files.bytes_at(path, MAXIMUM) == content, "interrupted binding has conflicting bytes; preserve and inspect")
        if (root / "receipt.json").exists():
            verify_bound(sealed)
            return receipt(sealed)
        for directory in ["configs", "plists", "logs"]:
            files.directory(root / directory)
        for name, content in generated.items():
            files.missing(root / name, content)
        # Recheck evidence after file publication; a partial bind never starts.
        require(manifest(manifest_file) == sealed, "reviewed inputs changed during bind")
        verify_native(payload, native, database=True)
        files.missing(root / "receipt.json", files.encoded(receipt(sealed)))
        verify_bound(sealed)
        return receipt(sealed)


def command(payload, kind, area):
    runtime, root = payload["runtime"], control(payload)
    require(kind in {"postgres", "area"}, "cold handover refuses sync/watch/provisioning directly")
    if kind == "area":
        require(area in {a["name"] for a in runtime["input"]["areas"]}, "command area not in reviewed runtime")
        return [runtime["input"]["codifix"]["path"], "--config", str(root / "configs" / (area + ".toml")), "service", "run"]
    require(area is None, "PostgreSQL command cannot select an area")
    return [str(Path(runtime["input"]["postgres"]["bin_dir"]) / "postgres"), "-D", str(Path(payload["input"]["new_root"]) / "cluster"),
            "-p", str(runtime["input"]["pg_port"]), "-k", "", "-c", "listen_addresses=127.0.0.1", "-c", "config_file=" + str(root / "postgresql.conf"),
            "-c", "hba_file=" + str(root / "pg_hba.conf"), "-c", "shared_preload_libraries=", "-c", "session_preload_libraries=", "-c", "local_preload_libraries="]


def run(manifest_file, kind, area, native):
    # Reject forbidden selections before prerequisites or any process call.
    require(kind in {"postgres", "area"}, "cold handover refuses sync/watch/provisioning directly")
    sealed = manifest(manifest_file)
    with files.Lock(control(sealed["payload"])):
        require(manifest(manifest_file) == sealed, "runtime manifest changed before command selection")
        verify_bound(sealed)
        args = command(sealed["payload"], kind, area)
        verify_native(sealed["payload"], native, database=kind == "area")
        require(manifest(manifest_file) == sealed, "runtime inputs changed before exec")
        verify_bound(sealed)
        os.execve(args[0], args, onboarding.clean_env())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("inspect").add_argument("--input", type=Path, required=True)
    binder = sub.add_parser("bind")
    binder.add_argument("--manifest", type=Path, required=True)
    binder.add_argument("--review-sha256", required=True)
    binder.add_argument("--writers-stopped", action="store_true")
    runner = sub.add_parser("run")
    runner.add_argument("--manifest", type=Path, required=True)
    runner.add_argument("--kind", choices=["postgres", "area"], required=True)
    runner.add_argument("--area")
    args = parser.parse_args()
    native = Boundary()
    if args.command == "inspect":
        print(json.dumps(inspect(args.input, native), indent=2))
    elif args.command == "bind":
        print(json.dumps(bind(args.manifest, args.review_sha256, args.writers_stopped, native), indent=2))
    else:
        run(args.manifest, args.kind, args.area, native)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("Codifix cold runtime rebind refused: " + (str(error) if isinstance(error, Refused) else type(error).__name__), file=sys.stderr)
        sys.exit(1)
