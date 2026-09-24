#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Hash-bound PostgreSQL-17 receiver for one inactive Trust database.

The tool never provisions packages, clusters, databases, roles, credentials or
services. `inspect` is read-only. `restore` accepts only a fresh exact target and
performs trusted schema plus allowlisted data in one transaction.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
from typing import Any

PROFILE = {
    "format": "memory-trust-receiver-profile-v1", "postgres_major": 17,
    "postgres_version": "17.11", "client_version": "17.11",
    "extensions": {"vector": "0.8.6", "pgcrypto": "1.3", "pg_trgm": "1.6"},
    "database": "memory_trust", "database_port": 5433, "role": "memory_trust",
    "service": "memory-layer@trust.service", "port": 8091, "active": False,
}
SCHEMA_SHA256 = "2ce8b07f9c8fc252b0162d158ccb8e57082c2186306cdc489c9343ec91a115dc"
CATALOG_SHA256 = "ec075904170f4e45d739bf971215f511d701cddffc432eef46cabfe3c41199eb"
SHA = re.compile(r"^[0-9a-f]{64}$")
UUID = re.compile(r"^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$")
IDENTIFIER = re.compile(r"^[a-z][a-z0-9_]{0,62}$")
ROW_KEYS = {
    "public.projects": "r.id::text", "public.memory_entries": "r.id::text",
    "public.memory_canonical_state": "r.canonical_id::text",
    "public.memory_tags": "jsonb_build_array(r.memory_entry_id,r.tag)::text",
    "public.memory_relations": "r.id::text", "public.memory_sources": "r.id::text",
}
SEED_COUNTS = {"public.supervisor_authorities": 4, "public.supervisor_journal_heads": 4,
               "public.timeline_position": 1, "public.loop_global_state": 1}
ENV = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C", "TZ": "UTC"}
CURRENT_PHASE = "request-validation"


class Rejected(Exception):
    pass


def reject() -> None:
    raise Rejected(f"Trust receiver rejected the request during {CURRENT_PHASE}; private diagnostics suppressed")


def exact(value: Any, fields: set[str]) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != fields:
        reject()
    return value


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def private_file(path: Path, maximum: int, expected: str | None = None) -> bytes:
    if not path.is_absolute() or not SHA.fullmatch(expected or "0" * 64):
        reject()
    current = path.parent
    while True:
        info = os.lstat(current)
        if not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.geteuid()):
            reject()
        if info.st_mode & 0o022 and not info.st_mode & stat.S_ISVTX:
            reject()
        parent = current.parent
        if parent == current:
            break
        current = parent
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(descriptor)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid not in (0, os.geteuid())
                or info.st_mode & 0o077 or info.st_nlink != 1 or info.st_size > maximum):
            reject()
        data = b""
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            data += chunk
            if len(data) > maximum:
                reject()
    finally:
        os.close(descriptor)
    if expected is not None and digest(data) != expected:
        reject()
    return data


def json_file(path: Path, maximum: int, expected: str) -> tuple[dict[str, Any], bytes]:
    data = private_file(path, maximum, expected)
    try:
        value = json.loads(data)
    except (UnicodeDecodeError, json.JSONDecodeError):
        reject()
    if not isinstance(value, dict):
        reject()
    return value, data


def run(command: list[str], data: bytes | None = None, timeout: int = 30) -> bytes:
    try:
        result = subprocess.run(command, input=data, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                env=ENV, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired):
        reject()
    if result.returncode != 0:
        reject()
    return result.stdout


def psql(target: dict[str, Any], sql: str | bytes, *, transaction: bool = False,
         timeout: int = 30) -> bytes:
    connection = target["connection"]
    command = ["/usr/lib/postgresql/17/bin/psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1",
               "-h", connection["directory"], "-p", str(connection["port"]),
               "-U", target["admin_username"], "-d", target["database"]]
    if transaction:
        command.append("-1")
    return run(command, sql.encode() if isinstance(sql, str) else sql, timeout)


def target_file(path: Path, expected: str) -> dict[str, Any]:
    value, _ = json_file(path, 8192, expected)
    exact(value, {"format", "connection", "database", "admin_username", "owner_role"})
    connection = exact(value["connection"], {"kind", "directory", "port"})
    if (value["format"] != "memory-trust-receiver-target-v1" or value["database"] != PROFILE["database"]
            or value["admin_username"] != "postgres" or value["owner_role"] != PROFILE["role"]
            or connection["kind"] != "unix" or connection["port"] != PROFILE["database_port"]
            or not isinstance(connection["directory"], str) or not Path(connection["directory"]).is_absolute()
            or "\0" in connection["directory"] or len(connection["directory"].encode()) > 256):
        reject()
    return value


def artifact(bundle: Path, value: Any) -> tuple[bytes, dict[str, Any]]:
    item = exact(value, {"name", "sha256", "bytes"})
    if (not isinstance(item["name"], str) or "/" in item["name"] or item["name"] in ("", ".", "..")
            or not SHA.fullmatch(item["sha256"]) or not isinstance(item["bytes"], int)
            or item["bytes"] < 0 or item["bytes"] > 32 * 1024 * 1024):
        reject()
    data = private_file(bundle / item["name"], item["bytes"], item["sha256"])
    if len(data) != item["bytes"]:
        reject()
    return data, item


def load_bundle(directory: Path, manifest_sha256: str) -> tuple[dict[str, Any], dict[str, bytes]]:
    manifest, _ = json_file(directory / "manifest.json", 1024 * 1024, manifest_sha256)
    exact(manifest, {"format", "status", "receiver_profile", "receiver_observation", "source", "target",
                     "schema", "artifacts", "result", "source_ddl_executed", "encrypted",
                     "transfer_authorized", "activation_authorized"})
    if (manifest["format"] != "memory-trust-receiver-bundle-v1"
            or manifest["status"] != "verified_inactive_unencrypted_not_transferred"
            or manifest["receiver_profile"] != PROFILE or manifest["source_ddl_executed"] is not False
            or manifest["encrypted"] is not False or manifest["transfer_authorized"] is not False
            or manifest["activation_authorized"] is not False):
        reject()
    observation = exact(manifest["receiver_observation"], {"format", "postgres_version", "client_version", "extensions"})
    if observation != {"format": "memory-trust-receiver-observation-v1",
                       "postgres_version": PROFILE["postgres_version"], "client_version": PROFILE["client_version"],
                       "extensions": PROFILE["extensions"]}:
        reject()
    source = exact(manifest["source"], {"instance_id", "snapshot_id", "migration"})
    target = exact(manifest["target"], {"instance_id", "operation_id", "database", "owner_role"})
    if (not UUID.fullmatch(source["instance_id"]) or not UUID.fullmatch(source["snapshot_id"])
            or not isinstance(source["migration"], int) or not 52 <= source["migration"] <= 60
            or not UUID.fullmatch(target["instance_id"]) or not UUID.fullmatch(target["operation_id"])
            or target["database"] != PROFILE["database"] or target["owner_role"] != PROFILE["role"]):
        reject()
    schema = exact(manifest["schema"], {"sha256", "migration_head", "migration_checksums", "catalog",
                                        "catalog_sha256", "object_owner"})
    if (schema["sha256"] != SCHEMA_SHA256 or schema["migration_head"] != 60
            or schema["object_owner"] != PROFILE["role"] or schema["catalog_sha256"] != CATALOG_SHA256
            or digest(json.dumps(schema["catalog"], separators=(",", ":")).encode()) != CATALOG_SHA256
            or not isinstance(schema["migration_checksums"], list) or len(schema["migration_checksums"]) != 60
            or any(not re.fullmatch(r"[0-9a-f]{96}", value) for value in schema["migration_checksums"] or [])):
        reject()
    catalog = exact(schema["catalog"], {"tables", "sequences"})
    tables = catalog["tables"]
    if (not isinstance(tables, list) or len(tables) != 114 or len(set(tables)) != 114
            or tables != sorted(tables) or any(
                name != "public._sqlx_migrations"
                and not re.fullmatch(r"public\.[a-z][a-z0-9_]{0,62}", name)
                for name in tables
            )):
        reject()
    artifacts = exact(manifest["artifacts"], {"admin_pre", "owned_pre", "data", "post", "expectation"})
    loaded = {name: artifact(directory, item)[0] for name, item in artifacts.items()}
    return manifest, loaded


def expectation(value: bytes, manifest: dict[str, Any]) -> tuple[dict[str, Any], dict[str, dict[str, Any]], dict[str, dict[str, Any]]]:
    try:
        parsed = json.loads(value)
    except (UnicodeDecodeError, json.JSONDecodeError):
        reject()
    required = {"format", "area", "required_schema_sha256", "source_instance", "snapshot_id", "target_instance",
                "operation_id", "source_migration", "target_migration", "inventory_sha256", "manifest_sha256",
                "partition_sha256", "bindings", "histories", "projects", "versions", "tombstones", "rows",
                "provenance", "evidence_mode", "activation_authorized", "current_source_verified"}
    exact(parsed, required)
    if (parsed["format"] != "memory-trust-expectation-v1" or parsed["area"] != "trust"
            or parsed["required_schema_sha256"] != SCHEMA_SHA256 or parsed["target_migration"] != 60
            or parsed["source_instance"] != manifest["source"]["instance_id"]
            or parsed["snapshot_id"] != manifest["source"]["snapshot_id"]
            or parsed["target_instance"] != manifest["target"]["instance_id"]
            or parsed["operation_id"] != manifest["target"]["operation_id"]
            or parsed["activation_authorized"] is not False or parsed["current_source_verified"] is not False
            or parsed["evidence_mode"] != "archive_reference_only"):
        reject()
    rows: dict[str, dict[str, Any]] = {}
    for row in parsed["rows"]:
        exact(row, {"key", "source", "target", "transformed"})
        if (not isinstance(row["key"], str) or row["key"].count("\0") != 1 or row["key"] in rows
                or not SHA.fullmatch(row["source"]) or not SHA.fullmatch(row["target"])
                or not isinstance(row["transformed"], bool) or row["key"].split("\0", 1)[0] not in ROW_KEYS):
            reject()
        rows[row["key"]] = row
    provenance: dict[str, dict[str, Any]] = {}
    provenance_fields = {"source_id", "decision_id", "decision_sha256", "canonical_id", "memory_version_id",
                         "source_sha256", "original_version_sha256", "archived_task_id", "archived_task_sha256", "target_sha256"}
    for row in parsed["provenance"]:
        exact(row, provenance_fields)
        if row["source_id"] in provenance:
            reject()
        provenance[row["source_id"]] = row
    if len(rows) != manifest["result"]["verified_rows"] or len(provenance) != manifest["result"]["projected_sources"]:
        reject()
    return parsed, rows, provenance


def json_query(target: dict[str, Any], query: str) -> Any:
    global CURRENT_PHASE
    output = psql(target, query).decode().strip().splitlines()
    if len(output) != 1:
        CURRENT_PHASE += "-output-shape"
        reject()
    try:
        return json.loads(output[0])
    except json.JSONDecodeError:
        CURRENT_PHASE += "-invalid-json"
        reject()


def inspect_target(target: dict[str, Any], manifest: dict[str, Any]) -> dict[str, Any]:
    version = run(["/usr/lib/postgresql/17/bin/psql", "--version"]).decode().strip()
    match = re.fullmatch(r"psql \(PostgreSQL\) ([0-9]+\.[0-9]+)(?: \([^\n]+\))?", version)
    if not match or match.group(1) != PROFILE["client_version"]:
        reject()
    info = json_query(target, "SELECT json_build_object('version',current_setting('server_version'),'database',current_database(),"
                             "'current_user',current_user,'owner',pg_get_userbyid(d.datdba),'clients',(SELECT count(*) FROM pg_stat_activity "
                             "WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend')) FROM pg_database d "
                             "WHERE datname=current_database();")
    role = json_query(target, "SELECT to_jsonb(r) FROM (SELECT rolname,rolsuper,rolinherit,rolcreaterole,rolcreatedb,rolcanlogin,rolreplication,rolbypassrls "
                             f"FROM pg_roles WHERE rolname='{PROFILE['role']}') r;")
    available = json_query(target, "SELECT json_object_agg(name,default_version) FROM pg_available_extensions "
                                  "WHERE name IN ('vector','pgcrypto','pg_trgm');")
    if (not str(info.get("version", "")).startswith(PROFILE["postgres_version"] + " ")
            and info.get("version") != PROFILE["postgres_version"]):
        reject()
    if (info.get("database") != PROFILE["database"] or info.get("current_user") != target["admin_username"]
            or info.get("owner") != PROFILE["role"] or info.get("clients") != 0 or available != PROFILE["extensions"]
            or role != {"rolname": PROFILE["role"], "rolsuper": False, "rolinherit": True, "rolcreaterole": False,
                        "rolcreatedb": False, "rolcanlogin": True, "rolreplication": False, "rolbypassrls": False}):
        reject()
    occupied = json_query(target, "SELECT json_build_object('user_schemas',(SELECT count(*) FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname !~ '^pg_'),"
                                  "'public_classes',(SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),"
                                  "'public_functions',(SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public'),"
                                  "'extensions',(SELECT count(*) FROM pg_extension WHERE extname<>'plpgsql'),'large_objects',(SELECT count(*) FROM pg_largeobject_metadata));")
    return {"postgres_version": PROFILE["postgres_version"], "client_version": PROFILE["client_version"],
            "extensions": available, "fresh": all(value == 0 for value in occupied.values())}


def verify(target: dict[str, Any], manifest: dict[str, Any], expected: dict[str, Any],
           rows: dict[str, dict[str, Any]], provenance: dict[str, dict[str, Any]]) -> dict[str, Any]:
    global CURRENT_PHASE
    CURRENT_PHASE = "verification-catalog"
    tables = manifest["schema"]["catalog"]["tables"]
    actual_tables = json_query(target, "SELECT json_agg(schemaname||'.'||tablename ORDER BY schemaname,tablename) FROM pg_tables WHERE schemaname='public';")
    if actual_tables != tables:
        reject()
    CURRENT_PHASE = "verification-counts"
    counts = {}
    for table in tables:
        value = psql(target, f"SELECT count(*) FROM ONLY {table};").decode().strip()
        if not value.isdigit():
            reject()
        counts[table] = int(value)
    expected_counts = {table: SEED_COUNTS.get(table, 0) for table in tables}
    expected_counts["public._sqlx_migrations"] = 60
    expected_counts["public.area_import_receipts"] = 1
    expected_counts["public.area_import_rows"] = len(rows)
    expected_counts["public.area_import_provenance"] = len(provenance)
    for key in rows:
        table = key.split("\0", 1)[0]
        expected_counts[table] += 1
    if counts != expected_counts:
        reject()
    CURRENT_PHASE = "verification-row-digests"
    seen: set[str] = set()
    for table, key_expression in ROW_KEYS.items():
        records = json_query(target, f"SELECT COALESCE(json_agg(json_build_object('key',{key_expression},'sha256',"
                                     f"encode(public.digest(to_jsonb(r)::text,'sha256'),'hex'))),'[]'::json) FROM ONLY {table} r;")
        for record in records:
            identity = table + "\0" + record["key"]
            if identity in seen or identity not in rows or rows[identity]["target"] != record["sha256"]:
                reject()
            seen.add(identity)
    if len(seen) != len(rows):
        reject()
    CURRENT_PHASE = "verification-import-receipt"
    receipt = json_query(target, "SELECT to_jsonb(r) FROM public.area_import_receipts r;")
    required_receipt = {"id": expected["operation_id"], "target_instance": expected["target_instance"], "area": "trust",
                        "source_instance": expected["source_instance"], "snapshot_id": expected["snapshot_id"],
                        "inventory_sha256": expected["inventory_sha256"], "manifest_sha256": expected["manifest_sha256"],
                        "partition_sha256": expected["partition_sha256"], "row_count": len(rows),
                        "projection_count": len(provenance), "source_migration": expected["source_migration"],
                        "target_migration": 60, "singleton": True}
    if any(receipt.get(key) != value for key, value in required_receipt.items()):
        reject()
    CURRENT_PHASE = "verification-import-rows"
    receipt_rows = json_query(target, "SELECT COALESCE(json_agg(to_jsonb(r)),'[]'::json) FROM public.area_import_rows r;")
    if len(receipt_rows) != len(rows):
        reject()
    received = set()
    for record in receipt_rows:
        identity = record["table_name"] + "\0" + record["record_key"]
        row = rows.get(identity)
        if (not row or identity in received or record["import_id"] != expected["operation_id"]
                or record["source_sha256"] != row["source"] or record["target_sha256"] != row["target"]
                or record["transformed"] != row["transformed"]):
            reject()
        received.add(identity)
    CURRENT_PHASE = "verification-provenance"
    provenance_rows = json_query(target, "SELECT COALESCE(json_agg(to_jsonb(r)),'[]'::json) FROM public.area_import_provenance r;")
    if len(provenance_rows) != len(provenance):
        reject()
    for record in provenance_rows:
        expected_record = provenance.get(record["source_id"])
        if not expected_record or record["import_id"] != expected["operation_id"] or any(record.get(key) != value for key, value in expected_record.items()):
            reject()
    CURRENT_PHASE = "verification-migrations"
    migrations = psql(target, "SELECT encode(checksum,'hex') FROM public._sqlx_migrations ORDER BY version;").decode().strip().splitlines()
    if migrations != manifest["schema"]["migration_checksums"]:
        reject()
    CURRENT_PHASE = "verification-sequences"
    sequences = manifest["schema"]["catalog"]["sequences"]
    sequence_catalog = json_query(target, "SELECT COALESCE(json_agg(s ORDER BY s.name),'[]'::json)::jsonb::text FROM (SELECT n.nspname||'.'||c.relname AS name,"
                                  "format_type(q.seqtypid,NULL) AS data_type,q.seqstart::text AS start_value,q.seqincrement::text AS increment_by,"
                                  "q.seqmin::text AS min_value,q.seqmax::text AS max_value,q.seqcache::text AS cache_size,q.seqcycle AS cycle,"
                                  "ownn.nspname||'.'||ownc.relname AS owner_table,owna.attname AS owner_column,d.deptype::text AS dependency_type "
                                  "FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_sequence q ON q.seqrelid=c.oid "
                                  "LEFT JOIN pg_depend d ON d.objid=c.oid AND d.classid='pg_class'::regclass AND d.refclassid='pg_class'::regclass "
                                  "AND d.deptype IN ('a','i') AND d.refobjsubid>0 LEFT JOIN pg_class ownc ON ownc.oid=d.refobjid "
                                  "LEFT JOIN pg_namespace ownn ON ownn.oid=ownc.relnamespace LEFT JOIN pg_attribute owna ON owna.attrelid=d.refobjid "
                                  "AND owna.attnum=d.refobjsubid WHERE c.relkind='S' AND n.nspname='public') s;")
    if not isinstance(sequence_catalog, list) or len(sequence_catalog) != len(sequences):
        CURRENT_PHASE = "verification-sequence-catalog-shape"
        reject()
    for index, (actual_sequence, expected_sequence) in enumerate(zip(sequence_catalog, sequences)):
        if not isinstance(actual_sequence, dict) or set(actual_sequence) != set(expected_sequence):
            CURRENT_PHASE = f"verification-sequence-catalog-{index}-fields"
            reject()
        for field, value in expected_sequence.items():
            if actual_sequence[field] != value:
                CURRENT_PHASE = f"verification-sequence-catalog-{index}-{field}"
                reject()
    CURRENT_PHASE = "verification-sequence-state"
    for sequence in sequences:
        name = sequence["name"]
        state = psql(target, f"SELECT last_value::text||':'||is_called::text FROM {name};").decode().strip()
        if state != "1:false":
            reject()
    CURRENT_PHASE = "verification-safety"
    safety = json_query(target, "SELECT json_build_object('failed_migrations',(SELECT count(*) FROM _sqlx_migrations WHERE NOT success OR version NOT BETWEEN 1 AND 60),"
                                "'invalid_indexes',(SELECT count(*) FROM pg_index WHERE NOT indisvalid),'invalid_constraints',(SELECT count(*) FROM pg_constraint WHERE NOT convalidated),"
                                "'unsafe_seeds',(SELECT count(*) FROM supervisor_authorities WHERE current_revision IS NOT NULL)+(SELECT count(*) FROM timeline_position WHERE position<>0)+"
                                "(SELECT count(*) FROM loop_global_state WHERE NOT kill_switch_enabled OR updated_by IS NOT NULL OR reason IS NOT NULL),"
                                "'broken_links',(SELECT count(*) FROM memory_relations r LEFT JOIN memory_canonical_state a ON a.canonical_id=r.src_canonical_id "
                                "LEFT JOIN memory_canonical_state b ON b.canonical_id=r.dst_canonical_id WHERE a.canonical_id IS NULL OR b.canonical_id IS NULL)+"
                                "(SELECT count(*) FROM memory_entries e LEFT JOIN memory_canonical_state s ON s.canonical_id=e.canonical_id WHERE s.canonical_id IS NULL));")
    if any(value != 0 for value in safety.values()):
        reject()
    CURRENT_PHASE = "verification-ownership"
    installed = json_query(target, "SELECT json_object_agg(extname,extversion) FROM pg_extension WHERE extname IN ('vector','pgcrypto','pg_trgm');")
    non_owned = int(psql(target, f"SELECT count(*) FROM (SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace "
                                f"WHERE n.nspname='public' AND c.relkind IN ('r','S','v','m','p','f') AND pg_get_userbyid(c.relowner)<>'{PROFILE['role']}' "
                                "AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_class'::regclass AND d.objid=c.oid AND d.deptype='e') "
                                "UNION ALL SELECT p.oid FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace "
                                f"WHERE n.nspname='public' AND pg_get_userbyid(p.proowner)<>'{PROFILE['role']}' "
                                "AND NOT EXISTS(SELECT 1 FROM pg_depend d WHERE d.classid='pg_proc'::regclass AND d.objid=p.oid AND d.deptype='e')) unowned;").decode().strip())
    if installed != PROFILE["extensions"] or non_owned != 0:
        reject()
    return {"format": "memory-trust-receiver-receipt-v1", "status": "restored_verified_inactive_not_activated",
            "manifest_sha256": "", "target_instance": expected["target_instance"], "operation_id": expected["operation_id"],
            "postgres_version": PROFILE["postgres_version"], "verified_rows": len(rows),
            "projected_sources": len(provenance), "histories": len(expected["histories"]), "versions": expected["versions"],
            "source_ddl_executed": False, "activation_authorized": False}


def fresh_guard() -> str:
    return f"""DO $memory_trust_guard$ BEGIN
IF current_database()<>'{PROFILE['database']}'
OR EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend')
OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname !~ '^pg_')
OR EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_depend d JOIN pg_namespace n ON d.refobjid=n.oid WHERE d.refclassid='pg_namespace'::regclass AND n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_extension WHERE extname<>'plpgsql')
OR EXISTS(SELECT 1 FROM pg_event_trigger) OR EXISTS(SELECT 1 FROM pg_largeobject_metadata)
OR EXISTS(SELECT 1 FROM pg_foreign_server) OR EXISTS(SELECT 1 FROM pg_foreign_data_wrapper)
OR EXISTS(SELECT 1 FROM pg_default_acl) OR EXISTS(SELECT 1 FROM pg_publication)
OR EXISTS(SELECT 1 FROM pg_subscription WHERE subdbid=(SELECT oid FROM pg_database WHERE datname=current_database()))
THEN RAISE EXCEPTION 'Fresh isolated Trust restore target required'; END IF;
END $memory_trust_guard$;
"""


def write_receipt(path: Path, receipt: dict[str, Any]) -> None:
    if not path.is_absolute() or path.exists() or path.parent.stat().st_uid not in (0, os.geteuid()) or path.parent.stat().st_mode & 0o077:
        reject()
    data = (json.dumps(receipt, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        os.write(descriptor, data); os.fsync(descriptor)
    finally:
        os.close(descriptor)


def main() -> int:
    global CURRENT_PHASE
    parser = argparse.ArgumentParser(prog="trust_receiver.py")
    parser.add_argument("command", choices=("inspect", "restore"))
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--manifest-sha256", required=True)
    parser.add_argument("--target", required=True, type=Path)
    parser.add_argument("--target-sha256", required=True)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--writers-stopped", action="store_true")
    parser.add_argument("--fresh-database", action="store_true")
    args = parser.parse_args()
    if not args.bundle.is_absolute() or not SHA.fullmatch(args.manifest_sha256) or not SHA.fullmatch(args.target_sha256):
        reject()
    CURRENT_PHASE = "bundle-validation"
    manifest, files = load_bundle(args.bundle, args.manifest_sha256)
    CURRENT_PHASE = "target-validation"
    target = target_file(args.target, args.target_sha256)
    CURRENT_PHASE = "target-inspection"
    inspection = inspect_target(target, manifest)
    if args.command == "inspect":
        if args.receipt or args.execute or args.writers_stopped or args.fresh_database:
            reject()
        print(json.dumps({"status": "receiver_inspected_not_restored", **inspection,
                          "manifest_sha256": args.manifest_sha256, "activation_authorized": False}, sort_keys=True))
        return 0
    if not args.receipt or not args.execute or not args.writers_stopped or not args.fresh_database or not inspection["fresh"]:
        reject()
    CURRENT_PHASE = "expectation-validation"
    expected, rows, provenance = expectation(files["expectation"], manifest)
    restore = (b"SET statement_timeout='120s'; SET lock_timeout='5s';\n" + fresh_guard().encode()
               + files["admin_pre"] + f"SET ROLE {PROFILE['role']};\n".encode()
               + files["owned_pre"] + files["data"] + files["post"])
    CURRENT_PHASE = "transactional-restore"
    psql(target, restore, transaction=True, timeout=180)
    CURRENT_PHASE = "post-restore-verification"
    receipt = verify(target, manifest, expected, rows, provenance)
    receipt["manifest_sha256"] = args.manifest_sha256
    CURRENT_PHASE = "receipt-write"
    write_receipt(args.receipt, receipt)
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Rejected as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(1)
