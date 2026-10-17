#!/usr/bin/env python3
"""Exercise the actual embedded inventory with command/filesystem stubs only."""
import contextlib
import io
import json
import pathlib
import stat
import subprocess
import time
import types
import unittest
from unittest import mock

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / "deploy/areas/ca-preflight.sh"
SOURCE = SCRIPT.read_text().split("<<'PYTHON_PREFLIGHT'\n", 1)[1].rsplit("\nPYTHON_PREFLIGHT", 1)[0]
SECRET = "DO_NOT_PRINT_PRIVATE_KEY_OR_PASSWORD"


class InventoryTests(unittest.TestCase):
    def inventory(self, root=True, scenario="healthy", sockets=True, missing_tool=False,
                  architecture="aarch64", libc="glibc 2.36"):
        commands = []
        table_names = ["projects", "sessions", "tasks", "raw_captures", "memory_entries", "_sqlx_migrations"]

        def command(arguments, **kwargs):
            commands.append(arguments)
            self.assertEqual(kwargs["cwd"], "/")
            self.assertEqual(set(kwargs["env"]), {"PATH", "LANG", "LC_ALL"})
            self.assertEqual(kwargs["env"]["PATH"], "/usr/sbin:/usr/bin:/sbin:/bin")
            self.assertNotIn("shell", kwargs)
            self.assertIs(kwargs["stdin"], subprocess.DEVNULL)
            self.assertLessEqual(kwargs["timeout"], 6)
            program = arguments[0]
            if scenario == "timeout":
                raise subprocess.TimeoutExpired(arguments, 6, stderr=SECRET)
            if program == "codifix":
                self.assertEqual(arguments, ["codifix", "--version"])
                output = SECRET if scenario == "malformed" else "codifix 3.0.0-rc.1\n"
            elif program == "systemctl":
                if arguments == ["systemctl", "--version"]:
                    output = "systemd 252 (252.31-1)\n+PAM +SECCOMP +BPF_FRAMEWORK\n"
                else:
                    self.assertEqual(arguments[1:4], ["show", "--no-pager", "--property=LoadState,ActiveState,SubState"])
                    output = "LoadState=loaded\nActiveState=active\nSubState=running\n"
            elif program == "ss":
                self.assertEqual(arguments[1:], ["--no-header", "--listening", "--tcp", "--udp", "--numeric"])
                output = ("tcp LISTEN 0 128 127.0.0.1:5432 0.0.0.0:*\n"
                          "tcp LISTEN 0 128 [::]:443 [::]:*\n"
                          "udp UNCONN 0 0 10.23.45.67:53 0.0.0.0:*\n")
            elif program == "runuser":
                self.assertTrue(root, "non-root must not invoke database inspection")
                self.assertEqual(arguments[:6], ["runuser", "--user", "postgres", "--", "env", "-i"])
                self.assertIn("PGPASSFILE=/dev/null", arguments)
                self.assertIn("PGSERVICEFILE=/dev/null", arguments)
                self.assertIn("PGSYSCONFDIR=/nonexistent", arguments)
                self.assertIn("--no-password", arguments)
                self.assertIn("--no-psqlrc", arguments)
                self.assertIn("--host=/run/postgresql", arguments)
                self.assertIn("--port=5432", arguments)
                self.assertTrue(any("default_transaction_read_only=on" in arg for arg in arguments))
                query = arguments[-1]
                self.assertEqual(arguments[-2], "--command")
                self.assertTrue(query.startswith("SELECT "))
                for forbidden in ("canonical_text", "payload_json", "root_path", "api_token", "COPY", "UPDATE", "INSERT", "DELETE", "CREATE"):
                    self.assertNotIn(forbidden, query)
                if scenario == "denied":
                    return subprocess.CompletedProcess(arguments, 1, "", "password authentication failed: " + SECRET)
                if scenario == "offline":
                    return subprocess.CompletedProcess(arguments, 1, "", "connection refused: " + SECRET)
                if "pg_database_size" in query:
                    name = "host=remote password=" + SECRET if scenario == "connection_string" else "memory_trust"
                    output = json.dumps([{"name": name, "size_bytes": 12345}])
                elif "pg_catalog.pg_class" in query:
                    output = json.dumps([] if scenario == "not_codifix" else table_names)
                elif "pg_catalog.pg_stat_activity" in query:
                    for forbidden in ("query", "usename", "application_name"):
                        self.assertNotIn(forbidden, query)
                    output = json.dumps({"client_backends": 2, "local_socket_client_backends": 1})
                    if scenario == "bad_activity":
                        output = SECRET
                elif "MAX(version)" in query:
                    output = json.dumps({"head": 52, "applied": 52, "failed": 0})
                else:
                    values = {name: 12 for name in table_names if name != "_sqlx_migrations"}
                    if scenario == "malformed":
                        values["memory_entries"] = SECRET
                    output = json.dumps(values)
            else:
                self.fail("Unexpected command: " + repr(arguments))
            return subprocess.CompletedProcess(arguments, 0, output, "")

        def scan(path):
            self.assertEqual(path, "/run/postgresql")
            if not sockets:
                raise FileNotFoundError(path)
            entry = types.SimpleNamespace(name=".s.PGSQL.5432", stat=lambda **kwargs: types.SimpleNamespace(st_mode=stat.S_IFSOCK))
            return contextlib.nullcontext(iter([entry]))

        def read(path, *args, **kwargs):
            path = str(path)
            if path == "/etc/os-release":
                return 'ID=debian\nVERSION_ID="12"\nPRETTY_NAME=' + SECRET + '\nOTHER_SECRET=' + SECRET
            if scenario == "kernel_denied":
                raise PermissionError(SECRET)
            if scenario == "kernel_missing":
                raise FileNotFoundError(SECRET)
            if path == "/sys/fs/cgroup/cgroup.controllers":
                return SECRET if scenario == "bad_kernel" else "cpuset cpu io memory pids\n"
            if path == "/proc/sys/user/max_user_namespaces":
                return "0\n" if scenario == "namespaces_disabled" else "3000\n"
            if path == "/proc/sys/kernel/unprivileged_userns_clone":
                return "0\n" if scenario == "namespaces_disabled" else "1\n"
            self.fail("Unexpected file read: " + path)

        def metadata(path, **kwargs):
            self.assertIn(str(path), ["/proc/self/ns/" + name for name in
                                    ("user", "mnt", "pid", "net", "cgroup", "ipc", "uts")])
            if scenario == "kernel_denied":
                raise PermissionError(SECRET)
            if scenario == "kernel_missing":
                raise FileNotFoundError(SECRET)
            return types.SimpleNamespace(st_mode=stat.S_IFREG)

        def which(program, **kwargs):
            self.assertEqual(kwargs, {"path": "/usr/sbin:/usr/bin:/sbin:/bin"})
            self.assertIn(program, ("docker", "podman", "systemd-nspawn", "bwrap", "unshare", "psql", "runuser"))
            return None if missing_tool else "/usr/bin/tool"

        stream = io.StringIO()
        ticks = iter([0] + [61] * 200)
        clock = (lambda: next(ticks)) if scenario == "budget" else time.monotonic
        with mock.patch("subprocess.run", command), \
             mock.patch("os.geteuid", return_value=0 if root else 1000), \
             mock.patch("os.scandir", scan), \
             mock.patch("os.uname", return_value=types.SimpleNamespace(machine=architecture)), \
             mock.patch("os.confstr", return_value=libc), \
             mock.patch("os.stat", metadata), \
             mock.patch("os.statvfs", return_value=types.SimpleNamespace(f_blocks=100, f_frsize=4096, f_bavail=50)), \
             mock.patch("pathlib.Path.read_text", read), \
             mock.patch("shutil.which", which), \
             mock.patch("time.monotonic", clock), \
             mock.patch.dict("os.environ", {"OPENAI_API_KEY": SECRET, "PGPASSWORD": SECRET,
                                           "PGHOST": "remote.invalid", "PATH": "/untrusted/command/path"}), \
             contextlib.redirect_stdout(stream):
            exec(compile(SOURCE, str(SCRIPT), "exec"), {"__name__": "__main__"})
        text = stream.getvalue()
        self.assertNotIn(SECRET, text)
        self.assertNotIn("10.23.45.67", text)
        self.assertNotIn("/etc/", text)
        self.assertNotIn("/var/", text)
        return json.loads(text), commands

    def test_root_reads_only_aggregate_metadata(self):
        report, commands = self.inventory()
        self.assertEqual(report["host"]["architecture"], "aarch64")
        self.assertEqual(report["host"]["os"]["id"], "debian")
        self.assertEqual(report["codifix"]["version"], "3.0.0-rc.1")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["name"], "memory_trust")
        self.assertEqual(database["counts"]["tables"]["memory_entries"], 12)
        self.assertEqual(database["schema_head"]["head"], 52)
        self.assertEqual(report["database_binding"]["status"], "unknown")
        self.assertFalse(report["postgresql"]["local_data_absence_proven"])
        self.assertEqual(len([args for args in commands if args[0] == "runuser"]), 5)
        self.assertEqual(report["format"], "codifix-ca-preflight-v2")
        self.assertEqual(report["host"]["libc"], {"status": "ok", "family": "glibc", "version": "2.36"})
        self.assertEqual(database["client_activity"]["counts"]["client_backends"], 2)
        self.assertFalse(database["client_activity"]["memory_client_identity_proven"])
        binding = report["database_binding"]
        self.assertEqual(binding["local_codifix_schema_candidates"][0]["database"], "memory_trust")
        self.assertFalse(binding["process_connection_identity_proven"])
        self.assertFalse(binding["shared_vs_unique_proven"])

    def test_non_root_reports_permission_limit_without_db_attempt(self):
        report, commands = self.inventory(root=False)
        self.assertEqual(report["postgresql"]["database_inventory"]["status"], "permission_denied")
        self.assertFalse(any(args[0] == "runuser" for args in commands))

    def test_staged_trust_never_proves_ca_or_offline_data_coverage(self):
        for inputs in ({}, {"root": False}, {"sockets": False}, {"scenario": "denied"}):
            with self.subTest(inputs=inputs):
                report, _ = self.inventory(**inputs)
                self.assertEqual(report["dataset_coverage"], {
                    "status": "unknown_requires_operator_reconciliation",
                    "shared_snapshot_includes_ca_only_data": "unproven",
                    "shared_snapshot_includes_offline_data": "unproven",
                    "offline_stores_inspected": False,
                    "application_evidence_files_inspected": False,
                    "ca_private_files_inspected": False,
                    "staged_trust_is_ca_migration": False,
                    "installation_or_migration_authorized": False,
                })

    def test_failed_connections_never_become_zero_counts_or_no_data(self):
        for scenario, expected in (("denied", "permission_denied"), ("offline", "unavailable")):
            with self.subTest(scenario=scenario):
                report, _ = self.inventory(scenario=scenario)
                cluster = report["postgresql"]["database_inventory"]["clusters"][0]
                self.assertEqual(cluster["connection"]["status"], expected)
                self.assertEqual(cluster["data_presence"], "unknown")
                self.assertNotIn("databases", cluster)
                self.assertFalse(report["postgresql"]["local_data_absence_proven"])

    def test_missing_tools_and_socket_are_explicit(self):
        for inputs, status in (({"missing_tool": True}, "unavailable"), ({"sockets": False}, "unknown")):
            with self.subTest(inputs=inputs):
                report, commands = self.inventory(**inputs)
                self.assertEqual(report["postgresql"]["database_inventory"]["status"], status)
                self.assertFalse(any(args[0] == "runuser" for args in commands))

    def test_non_codifix_database_is_not_treated_as_empty(self):
        report, _ = self.inventory(scenario="not_codifix")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["memory_schema"]["status"], "not_identified")
        self.assertEqual(database["counts"]["status"], "unknown")

    def test_database_name_cannot_be_a_connection_string(self):
        report, commands = self.inventory(scenario="connection_string")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["name"], "[redacted-nonstandard-name]")
        self.assertEqual(len([args for args in commands if args[0] == "runuser"]), 1)

    def test_unexpected_metadata_and_command_errors_are_redacted(self):
        report, _ = self.inventory(scenario="malformed")
        self.assertEqual(report["codifix"]["status"], "unavailable")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["counts"]["status"], "unavailable")
        self.assertEqual(report["postgresql"]["database_inventory"]["status"], "partial")
        report, _ = self.inventory(scenario="timeout")
        self.assertEqual(report["codifix"]["reason"], "check_timed_out")

    def test_total_time_budget_stops_later_commands(self):
        report, commands = self.inventory(scenario="budget")
        self.assertEqual(commands, [])
        self.assertEqual(report["codifix"]["reason"], "inventory_time_budget_exhausted")
        self.assertEqual(report["database_binding"]["status"], "unknown")

    def test_verified_arm64_metadata_blocks_old_libc_without_executing_artifacts(self):
        for version, expected in (("2.9", "blocked"), ("2.36", "blocked"),
                                  ("2.42", "blocked"), ("2.43", "minimum_version_matches_only"),
                                  ("2.43.1", "minimum_version_matches_only"), ("3.0", "minimum_version_matches_only")):
            with self.subTest(version=version):
                report, commands = self.inventory(libc="glibc " + version)
                pi = report["pi_arm64"]
                self.assertEqual(pi["native_abi_precheck"]["status"], expected)
                self.assertEqual(pi["pinned_artifact_metadata"]["required_glibc"], "2.43")
                self.assertEqual(pi["pinned_artifact_metadata"]["machine"], "AArch64")
                self.assertFalse(pi["artifact_executed"])
                self.assertFalse(pi["local_artifact_inspected"])
                self.assertFalse(pi["activation_authorized"])
                self.assertTrue(all(args[0] in ("systemctl", "codifix", "ss", "runuser") for args in commands))

    def test_wrong_architecture_and_unknown_libc_never_pass_native_check(self):
        report, _ = self.inventory(architecture="armv7l", libc="glibc 2.43")
        self.assertEqual(report["pi_arm64"]["native_abi_precheck"]["reason"], "host_is_not_native_arm64")
        for value in (None, "musl 1.2.5", SECRET):
            report, _ = self.inventory(libc=value)
            self.assertEqual(report["pi_arm64"]["native_abi_precheck"]["status"], "unknown")

    def test_kernel_and_container_metadata_do_not_claim_isolation_or_start_anything(self):
        report, commands = self.inventory()
        runtime = report["runtime"]
        self.assertEqual(runtime["systemd"]["version"], 252)
        self.assertEqual(runtime["cgroup"]["version"], 2)
        self.assertTrue(runtime["cgroup"]["resource_controllers_present"]["memory"])
        self.assertEqual(runtime["container_execution_available"], "unverified")
        self.assertFalse(runtime["sandbox_enforcement_proven"])
        self.assertFalse(runtime["namespaces"]["namespace_creation_tested"])
        self.assertFalse(any(args[0] in ("docker", "podman", "unshare", "bwrap") for args in commands))
        for scenario in ("bad_kernel", "kernel_denied", "kernel_missing", "namespaces_disabled"):
            report, _ = self.inventory(scenario=scenario)
            self.assertFalse(report["runtime"]["sandbox_enforcement_proven"])
            if scenario == "bad_kernel":
                self.assertEqual(report["runtime"]["cgroup"]["status"], "unknown")
            elif scenario == "kernel_denied":
                self.assertEqual(report["runtime"]["cgroup"]["status"], "permission_denied")
            elif scenario == "kernel_missing":
                self.assertEqual(report["runtime"]["cgroup"]["status"], "unknown")
                self.assertEqual(report["runtime"]["namespaces"]["interfaces"]["net"]["status"], "unavailable")
            else:
                self.assertEqual(report["runtime"]["namespaces"]["max_user_namespaces"]["value"], 0)

    def test_bad_activity_is_partial_and_never_establishes_codifix_binding(self):
        report, _ = self.inventory(scenario="bad_activity")
        self.assertEqual(report["postgresql"]["database_inventory"]["status"], "partial")
        self.assertEqual(report["database_binding"]["status"], "unknown")
        self.assertFalse(report["database_binding"]["no_data_proven"])


if __name__ == "__main__":
    unittest.main()
