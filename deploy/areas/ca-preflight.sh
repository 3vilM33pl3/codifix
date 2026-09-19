#!/usr/bin/env bash
# Read-only console inventory. This script never installs, starts or changes a
# service, reads application configuration/credentials, or prompts for a password.
set -euo pipefail
# Root-console checks must not execute a program selected by an inherited PATH.
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
if ! command -v python3 >/dev/null 2>&1; then
    printf '%s\n' '{"format":"memory-ca-preflight-v2","status":"unavailable","reason":"python3_missing","database_binding":{"status":"unknown","reason":"not_inspected"}}'
    exit 1
fi
exec python3 -I -S - <<'PYTHON_PREFLIGHT'
import datetime
import ipaddress
import json
import os
import pathlib
import re
import shutil
import stat
import subprocess
import time

PATH = "/usr/sbin:/usr/bin:/sbin:/bin"
ENV = {"PATH": PATH, "LC_ALL": "C", "LANG": "C"}
DEADLINE = time.monotonic() + 60
MAX_CLUSTERS = 4
MAX_DATABASES = 16
TABLES = (
    "projects", "sessions", "tasks", "raw_captures", "memory_entries",
    "memory_sources", "memory_canonical_state", "_sqlx_migrations",
)
# Static ELF metadata observed on 2026-09-17 after independently matching the
# official v0.5.1 ARM64 archive and exact pi member against SHA256SUMS and its
# upstream build manifest. This is not operator signature/activation approval.
PI_ARM64_ABI = {
    "version": "0.5.1",
    "source_revision": "7c018475d54d4e09c907691e47a520f1f09e3288",
    "binary_sha256": "0190009efe002c3cbca80a5f005472cea0df5901706c9e49dcb6c4c6891e0884",
    "archive_sha256": "89906e3fd11842c1a571f174740a188ebdc6c8a7a65f51f1d6f0b23800ad2e44",
    "elf_class": "ELF64", "endianness": "little", "machine": "AArch64",
    "loader": "ld-linux-aarch64.so.1", "required_glibc": "2.43",
    "needed_libraries": ["libgcc_s.so.1", "libm.so.6", "libc.so.6", "ld-linux-aarch64.so.1"],
    "evidence": "upstream_checksum_and_build_manifest_matched_static_readelf_only",
    "execution_tested": False, "operator_signature_verified": False,
}


def outcome(status, reason=None, **values):
    result = {"status": status, **values}
    if reason:
        result["reason"] = reason
    return result


def run(arguments, timeout=6):
    """Never expose raw command errors, command lines, paths or environment."""
    remaining = DEADLINE - time.monotonic()
    if remaining <= 0:
        return "unavailable", "inventory_time_budget_exhausted", ""
    try:
        result = subprocess.run(arguments, env=ENV, cwd="/", stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, timeout=min(timeout, remaining), check=False)
    except FileNotFoundError:
        return "unavailable", "command_missing", ""
    except PermissionError:
        return "permission_denied", "command_not_permitted", ""
    except subprocess.TimeoutExpired:
        return "unavailable", "check_timed_out", ""
    except OSError:
        return "unavailable", "command_failed", ""
    except UnicodeError:
        return "unavailable", "unexpected_output_encoding", ""
    if result.returncode:
        denied = any(word in result.stderr.lower() for word in
                     ("permission denied", "authentication failed", "no password supplied", "access denied"))
        return ("permission_denied" if denied else "unavailable"), "check_failed", ""
    if len(result.stdout) > 1_000_000:
        return "unavailable", "unexpected_output_size", ""
    return "ok", None, result.stdout


def version():
    status, reason, output = run(["memory", "--version"])
    if status != "ok":
        return outcome(status, reason)
    match = re.fullmatch(r"memory ([0-9][A-Za-z0-9.+_~-]{0,95})\s*", output)
    return outcome("ok", version=match[1]) if match else outcome("unavailable", "unexpected_version_output")


def libc():
    try:
        value = os.confstr("CS_GNU_LIBC_VERSION")
    except (AttributeError, OSError, ValueError):
        return outcome("unknown", "gnu_libc_identity_unavailable")
    match = re.fullmatch(r"glibc ([0-9]{1,3}\.[0-9]{1,3}(?:\.[0-9]{1,3})?)", value or "")
    return outcome("ok", family="glibc", version=match[1]) if match else outcome("unknown", "gnu_libc_identity_unavailable")


def metadata_number(path):
    try:
        value = pathlib.Path(path).read_text().strip()
    except PermissionError:
        return outcome("permission_denied", "kernel_metadata_not_permitted")
    except (OSError, UnicodeError):
        return outcome("unavailable", "kernel_metadata_missing")
    return outcome("ok", value=int(value)) if re.fullmatch(r"[0-9]{1,18}", value) else outcome("unknown", "unexpected_kernel_metadata")


def namespace_metadata():
    namespaces = {}
    for name in ("user", "mnt", "pid", "net", "cgroup", "ipc", "uts"):
        try:
            # Metadata only: never read process environment/arguments or print
            # namespace IDs, cgroup paths, container IDs or another process.
            os.stat("/proc/self/ns/" + name)
            namespaces[name] = outcome("present")
        except PermissionError:
            namespaces[name] = outcome("permission_denied", "namespace_metadata_not_permitted")
        except OSError:
            namespaces[name] = outcome("unavailable", "namespace_metadata_missing")
    return {"interfaces": namespaces,
            "max_user_namespaces": metadata_number("/proc/sys/user/max_user_namespaces"),
            "unprivileged_userns_clone": metadata_number("/proc/sys/kernel/unprivileged_userns_clone"),
            "namespace_creation_tested": False, "sandbox_enforcement_proven": False}


def cgroup_metadata():
    try:
        raw = pathlib.Path("/sys/fs/cgroup/cgroup.controllers").read_text()
    except PermissionError:
        return outcome("permission_denied", "cgroup_metadata_not_permitted", enforcement_proven=False)
    except (OSError, UnicodeError):
        # Missing unified controllers can mean v1, an unmounted hierarchy or a
        # restricted container. Never claim there is no cgroup support.
        return outcome("unknown", "unified_controllers_unavailable_v1_or_restriction_unchecked", enforcement_proven=False)
    allowed = {"cpu", "cpuset", "io", "memory", "hugetlb", "pids", "rdma", "misc", "dmem"}
    controllers = raw.split()
    if len(raw) > 512 or any(value not in allowed for value in controllers) or len(set(controllers)) != len(controllers):
        return outcome("unknown", "unexpected_cgroup_metadata", enforcement_proven=False)
    return outcome("observed", version=2, controllers=sorted(controllers),
                   resource_controllers_present={name: name in controllers for name in ("cpu", "memory", "pids")},
                   delegation_tested=False, enforcement_proven=False)


def runtime_metadata():
    status, reason, output = run(["systemctl", "--version"])
    match = re.match(r"systemd ([0-9]{1,4})(?:\s|$)", output) if status == "ok" else None
    manager = outcome("ok", version=int(match[1])) if match else outcome(status if status != "ok" else "unknown", reason or "unexpected_systemd_version")
    return {"systemd": manager, "namespaces": namespace_metadata(), "cgroup": cgroup_metadata(),
            "tools": {name: bool(shutil.which(name, path=PATH)) for name in
                      ("docker", "podman", "systemd-nspawn", "bwrap", "unshare")},
            "tool_coverage": "executable_presence_only_no_daemon_connection_or_container_start",
            "container_execution_available": "unverified", "sandbox_enforcement_proven": False}


def pi_compatibility(host_metadata):
    architecture = host_metadata["architecture"]
    local_libc = host_metadata["libc"]
    if architecture not in ("aarch64", "arm64"):
        compatibility = outcome("blocked", "host_is_not_native_arm64")
    elif local_libc["status"] != "ok":
        compatibility = outcome("unknown", "host_glibc_cannot_be_compared")
    elif tuple(map(int, local_libc["version"].split("."))) < (2, 43):
        compatibility = outcome("blocked", "host_glibc_below_pinned_pi_requirement")
    else:
        compatibility = outcome("minimum_version_matches_only", "loader_libraries_kernel_isa_and_isolation_unverified")
    return {"pinned_artifact_metadata": PI_ARM64_ABI, "native_abi_precheck": compatibility,
            "local_artifact_inspected": False, "artifact_executed": False, "activation_authorized": False}


def host():
    architecture = os.uname().machine
    os_info = {}
    try:
        for line in pathlib.Path("/etc/os-release").read_text().splitlines():
            key, separator, value = line.partition("=")
            if separator and key in ("ID", "VERSION_ID"):
                value = value.strip().strip('"').strip("'")
                if re.fullmatch(r"[A-Za-z0-9._-]{1,64}", value):
                    os_info[key.lower()] = value
        os_result = outcome("ok", **os_info) if os_info else outcome("unavailable", "os_metadata_missing")
    except PermissionError:
        os_result = outcome("permission_denied", "os_metadata_not_permitted")
    except (OSError, UnicodeError):
        os_result = outcome("unavailable", "os_metadata_missing")
    disks = {}
    # Labels identify the purpose, without publishing filesystem/mount paths.
    for label, location in (("root", "/"), ("memory_data", "/var/lib/memory-layer"),
                            ("postgres_data", "/var/lib/postgresql")):
        try:
            info = os.statvfs(location)
            disks[label] = outcome("ok", total_bytes=info.f_blocks * info.f_frsize,
                                   available_bytes=info.f_bavail * info.f_frsize)
        except PermissionError:
            disks[label] = outcome("permission_denied", "filesystem_metadata_not_permitted")
        except OSError:
            disks[label] = outcome("unavailable", "location_missing_or_inaccessible")
    return {"architecture": architecture if re.fullmatch(r"[A-Za-z0-9_-]{1,32}", architecture) else "unknown",
            "os": os_result, "disk": disks, "libc": libc(), "root": os.geteuid() == 0}


def service(unit):
    status, reason, output = run(["systemctl", "show", "--no-pager",
                                  "--property=LoadState,ActiveState,SubState", unit])
    if status != "ok":
        return outcome(status, reason)
    values = {}
    for line in output.splitlines():
        key, separator, value = line.partition("=")
        if separator and key in ("LoadState", "ActiveState", "SubState") and re.fullmatch(r"[a-z-]{1,40}", value):
            values[key] = value
    if len(values) != 3:
        return outcome("unavailable", "unexpected_service_output")
    return outcome("not_installed" if values["LoadState"] == "not-found" else "ok", **values)


def listeners():
    status, reason, output = run(["ss", "--no-header", "--listening", "--tcp", "--udp", "--numeric"])
    if status != "ok":
        return outcome(status, reason)
    rows = set()
    ignored = 0
    for line in output.splitlines():
        fields = line.split()
        if len(fields) < 6 or fields[0] not in ("tcp", "udp"):
            ignored += 1
            continue
        address, separator, port = fields[4].rpartition(":")
        if not separator or not port.isdigit() or not 0 < int(port) <= 65535:
            ignored += 1
            continue
        address = address.strip("[]").split("%", 1)[0]
        try:
            ip = ipaddress.ip_address(address)
            scope = "loopback" if ip.is_loopback else "wildcard" if ip.is_unspecified else "non_loopback"
        except ValueError:
            if address != "*":
                ignored += 1
                continue
            scope = "wildcard"
        rows.add((fields[0], int(port), scope))
    ordered = sorted(rows)
    return outcome("partial" if ignored or len(ordered) > 256 else "ok",
                   listeners=[{"protocol": protocol, "port": port, "scope": scope}
                              for protocol, port, scope in ordered[:256]],
                   truncated=len(ordered) > 256, unparsed_rows=ignored,
                   process_identity="not_inspected", coverage="current_network_namespace_only")


def local_postgres_sockets():
    ports = set()
    try:
        with os.scandir("/run/postgresql") as entries:
            for entry in entries:
                match = re.fullmatch(r"\.s\.PGSQL\.([0-9]{1,5})", entry.name)
                if match and 0 < int(match[1]) <= 65535 and stat.S_ISSOCK(entry.stat(follow_symlinks=False).st_mode):
                    ports.add(int(match[1]))
    except PermissionError:
        return outcome("permission_denied", "socket_inventory_not_permitted"), []
    except OSError:
        return outcome("unavailable", "standard_socket_directory_missing_or_inaccessible"), []
    return outcome("ok", standard_socket_count=len(ports)), sorted(ports)


def sql(port, database, query):
    # env -i runs AFTER runuser, so PAM or inherited PG* variables cannot select a
    # different database/server, password/service file, or startup script.
    arguments = ["runuser", "--user", "postgres", "--", "env", "-i", f"PATH={PATH}", "LC_ALL=C",
                 "PGPASSFILE=/dev/null", "PGSERVICEFILE=/dev/null", "PGSYSCONFDIR=/nonexistent",
                 "PGCONNECT_TIMEOUT=2", "PGAPPNAME=memory-ca-readonly-preflight",
                 "PGOPTIONS=-c default_transaction_read_only=on -c statement_timeout=4000 -c lock_timeout=1000 -c search_path=pg_catalog",
                 "psql", "--no-psqlrc", "--no-password", "--tuples-only", "--no-align",
                 "--set=ON_ERROR_STOP=1", "--host=/run/postgresql", f"--port={port}",
                 "--username=postgres", f"--dbname={database}", "--command", query]
    status, reason, output = run(arguments)
    if status != "ok":
        return outcome(status, reason), None
    try:
        return outcome("ok"), json.loads(output)
    except (json.JSONDecodeError, ValueError):
        return outcome("unavailable", "unexpected_database_metadata"), None


def inspect_database(port, database):
    names = ",".join("'" + name + "'" for name in TABLES)
    check, relations = sql(port, database,
        "SELECT COALESCE(json_agg(c.relname ORDER BY c.relname), '[]'::json) "
        "FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace "
        "WHERE n.nspname='public' AND c.relkind='r' AND NOT c.relispartition "
        "AND c.relname IN (" + names + ");")
    if check["status"] != "ok":
        return {"memory_schema": check, "counts": outcome("unknown", "schema_inspection_failed")}
    if not isinstance(relations, list) or any(name not in TABLES for name in relations):
        return {"memory_schema": outcome("unavailable", "unexpected_database_metadata")}
    if not {"projects", "memory_entries"}.issubset(relations):
        return {"memory_schema": outcome("not_identified", "expected_public_memory_tables_missing"),
                "counts": outcome("unknown", "not_a_verified_memory_schema")}
    result = {"memory_schema": outcome("candidate", "table_names_only_not_application_binding")}
    # ONLY prevents an unexpected inherited/foreign child from causing remote
    # access. Views, foreign tables and partitioned replacements are not accepted.
    fields = ["'" + name + "', (SELECT COUNT(*) FROM ONLY public.\"" + name + "\")"
              for name in TABLES if name in relations and name != "_sqlx_migrations"]
    check, counts = sql(port, database, "SELECT json_build_object(" + ",".join(fields) + ");")
    expected = {name for name in relations if name != "_sqlx_migrations"}
    if check["status"] == "ok" and (not isinstance(counts, dict) or set(counts) != expected or
            any(type(value) is not int or value < 0 for value in counts.values())):
        check = outcome("unavailable", "unexpected_count_metadata")
    result["counts"] = outcome("ok", tables=counts) if check["status"] == "ok" else check
    check, activity = sql(port, database,
        "SELECT json_build_object('client_backends', COUNT(*) FILTER (WHERE backend_type='client backend'), "
        "'local_socket_client_backends', COUNT(*) FILTER (WHERE backend_type='client backend' AND client_addr IS NULL)) "
        "FROM pg_catalog.pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid();")
    if check["status"] == "ok" and (not isinstance(activity, dict) or
            set(activity) != {"client_backends", "local_socket_client_backends"} or
            any(type(value) is not int or value < 0 for value in activity.values()) or
            activity["local_socket_client_backends"] > activity["client_backends"]):
        check = outcome("unavailable", "unexpected_activity_metadata")
    result["client_activity"] = outcome("observed", counts=activity,
        memory_client_identity_proven=False) if check["status"] == "ok" else check
    if "_sqlx_migrations" in relations:
        check, migrations = sql(port, database,
            "SELECT json_build_object('head', MAX(version) FILTER (WHERE success), "
            "'applied', COUNT(*) FILTER (WHERE success), 'failed', COUNT(*) FILTER (WHERE NOT success)) "
            "FROM ONLY public._sqlx_migrations;")
        if check["status"] == "ok" and (not isinstance(migrations, dict) or
                set(migrations) != {"head", "applied", "failed"} or
                any(type(value) is not int or value < 0 for key, value in migrations.items()
                    if key != "head" or value is not None)):
            check = outcome("unavailable", "unexpected_migration_metadata")
        result["schema_head"] = outcome("ok", **migrations) if check["status"] == "ok" else check
    else:
        result["schema_head"] = outcome("unknown", "migration_table_missing")
    return result


def databases():
    sockets, ports = local_postgres_sockets()
    result = {"socket_inventory": sockets,
              "coverage": "standard_local_unix_sockets_only_no_remote_or_container_probe",
              "application_binding": outcome("unknown", "config_and_credentials_deliberately_not_inspected"),
              "local_data_absence_proven": False}
    if os.geteuid() != 0:
        result["database_inventory"] = outcome("permission_denied", "root_required_for_passwordless_peer_check")
        return result
    if not shutil.which("psql", path=PATH) or not shutil.which("runuser", path=PATH):
        result["database_inventory"] = outcome("unavailable", "psql_or_runuser_missing")
        return result
    if not ports:
        result["database_inventory"] = outcome("unknown", "no_standard_socket_observed_custom_or_remote_binding_unchecked")
        return result
    clusters = []
    for port in ports[:MAX_CLUSTERS]:
        check, inventory = sql(port, "postgres",
            "SELECT COALESCE(json_agg(d), '[]'::json) FROM "
            "(SELECT datname AS name, pg_catalog.pg_database_size(oid) AS size_bytes "
            "FROM pg_catalog.pg_database WHERE datallowconn AND NOT datistemplate "
            "ORDER BY datname LIMIT 17) d;")
        cluster = {"port": port, "connection": check}
        if check["status"] != "ok":
            cluster["data_presence"] = "unknown"
            clusters.append(cluster)
            continue
        if not isinstance(inventory, list) or any(not isinstance(row, dict) or
                set(row) != {"name", "size_bytes"} or not isinstance(row["name"], str) or
                type(row["size_bytes"]) is not int or row["size_bytes"] < 0 for row in inventory):
            cluster["connection"] = outcome("unavailable", "unexpected_database_inventory")
            cluster["data_presence"] = "unknown"
            clusters.append(cluster)
            continue
        rows = []
        for row in inventory[:MAX_DATABASES]:
            if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,62}", row["name"]):
                rows.append({"name": "[redacted-nonstandard-name]", "size_bytes": row["size_bytes"],
                             "inspection": outcome("unknown", "nonstandard_database_name_not_used_as_connection_input")})
                continue
            rows.append({**row, **inspect_database(port, row["name"])})
        cluster.update(databases=rows, truncated=len(inventory) > MAX_DATABASES)
        clusters.append(cluster)
    incomplete = any(cluster["connection"]["status"] != "ok" or cluster.get("truncated")
                     for cluster in clusters)
    for cluster in clusters:
        for database in cluster.get("databases", []):
            schema_status = database.get("memory_schema", {}).get("status")
            if "inspection" in database or schema_status in ("unavailable", "permission_denied"):
                incomplete = True
            if schema_status == "candidate" and any(database.get(field, {}).get("status") != "ok"
                                                     for field in ("counts", "schema_head")):
                incomplete = True
            if schema_status == "candidate" and database.get("client_activity", {}).get("status") != "observed":
                incomplete = True
    result["database_inventory"] = outcome("partial" if len(ports) > MAX_CLUSTERS or incomplete
        else "ok", clusters=clusters, truncated=len(ports) > MAX_CLUSTERS)
    return result


def database_binding(postgresql):
    candidates = []
    for cluster in postgresql.get("database_inventory", {}).get("clusters", []):
        for database in cluster.get("databases", []):
            if database.get("memory_schema", {}).get("status") == "candidate":
                candidates.append({"local_socket_port": cluster["port"], "database": database["name"],
                                   "schema_head": database.get("schema_head", outcome("unknown")),
                                   "client_activity": database.get("client_activity", outcome("unknown"))})
    return outcome("unknown", "cannot_bind_running_memory_to_database_without_separate_review",
                   local_memory_schema_candidates=candidates, configuration_inspected=False,
                   process_connection_identity_proven=False, shared_vs_unique_proven=False,
                   no_data_proven=False)


host_metadata = host()
report = {
    "format": "memory-ca-preflight-v2",
    "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "status": "inventory_only_activation_not_approved",
    "command_time_budget_seconds": 60,
    "host": host_metadata,
    "runtime": runtime_metadata(),
    "pi_arm64": pi_compatibility(host_metadata),
    "memory": version(),
    "services": {unit: service(unit) for unit in
                 ("memory-layer.service", "memory-layer@trust.service", "step-ca.service", "openbao.service", "sshd.service", "ssh.service", "postgresql.service", "docker.service", "containerd.service", "podman.socket")},
    "network": listeners(),
    "dataset_coverage": {
        "status": "unknown_requires_operator_reconciliation",
        "shared_snapshot_includes_ca_only_data": "unproven",
        "shared_snapshot_includes_offline_data": "unproven",
        "offline_stores_inspected": False,
        "application_evidence_files_inspected": False,
        "ca_private_files_inspected": False,
        "staged_trust_is_ca_migration": False,
        "installation_or_migration_authorized": False,
    },
    "redaction": "no_config_env_credentials_records_logs_addresses_processes_or_secret_paths",
}
postgresql = databases()
report["postgresql"] = postgresql
report["database_binding"] = database_binding(postgresql)
print(json.dumps(report, indent=2, sort_keys=True))
PYTHON_PREFLIGHT
