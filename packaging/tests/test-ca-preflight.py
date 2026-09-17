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
    def inventory(self, root=True, scenario="healthy", sockets=True, missing_tool=False):
        commands = []
        table_names = ["projects", "sessions", "tasks", "raw_captures", "memory_entries", "_sqlx_migrations"]

        def command(arguments, **kwargs):
            commands.append(arguments)
            self.assertEqual(kwargs["cwd"], "/")
            self.assertEqual(set(kwargs["env"]), {"PATH", "LANG", "LC_ALL"})
            self.assertNotIn("shell", kwargs)
            self.assertIs(kwargs["stdin"], subprocess.DEVNULL)
            self.assertLessEqual(kwargs["timeout"], 6)
            program = arguments[0]
            if scenario == "timeout":
                raise subprocess.TimeoutExpired(arguments, 6, stderr=SECRET)
            if program == "memory":
                self.assertEqual(arguments, ["memory", "--version"])
                output = SECRET if scenario == "malformed" else "memory 2.1.0-rc.1\n"
            elif program == "systemctl":
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
                    output = json.dumps([] if scenario == "not_memory" else table_names)
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
            self.assertEqual(str(path), "/etc/os-release", "only OS metadata may be read")
            return 'ID=debian\nVERSION_ID="12"\nPRETTY_NAME=' + SECRET + '\nOTHER_SECRET=' + SECRET

        stream = io.StringIO()
        ticks = iter([0] + [61] * 200)
        clock = (lambda: next(ticks)) if scenario == "budget" else time.monotonic
        with mock.patch("subprocess.run", command), \
             mock.patch("os.geteuid", return_value=0 if root else 1000), \
             mock.patch("os.scandir", scan), \
             mock.patch("os.uname", return_value=types.SimpleNamespace(machine="aarch64")), \
             mock.patch("os.statvfs", return_value=types.SimpleNamespace(f_blocks=100, f_frsize=4096, f_bavail=50)), \
             mock.patch("pathlib.Path.read_text", read), \
             mock.patch("shutil.which", return_value=None if missing_tool else "/usr/bin/tool"), \
             mock.patch("time.monotonic", clock), \
             mock.patch.dict("os.environ", {"OPENAI_API_KEY": SECRET, "PGPASSWORD": SECRET, "PGHOST": "remote.invalid"}), \
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
        self.assertEqual(report["memory"]["version"], "2.1.0-rc.1")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["name"], "memory_trust")
        self.assertEqual(database["counts"]["tables"]["memory_entries"], 12)
        self.assertEqual(database["schema_head"]["head"], 52)
        self.assertEqual(report["database_binding"]["status"], "unknown")
        self.assertFalse(report["postgresql"]["local_data_absence_proven"])
        self.assertEqual(len([args for args in commands if args[0] == "runuser"]), 4)

    def test_non_root_reports_permission_limit_without_db_attempt(self):
        report, commands = self.inventory(root=False)
        self.assertEqual(report["postgresql"]["database_inventory"]["status"], "permission_denied")
        self.assertFalse(any(args[0] == "runuser" for args in commands))

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

    def test_non_memory_database_is_not_treated_as_empty(self):
        report, _ = self.inventory(scenario="not_memory")
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
        self.assertEqual(report["memory"]["status"], "unavailable")
        database = report["postgresql"]["database_inventory"]["clusters"][0]["databases"][0]
        self.assertEqual(database["counts"]["status"], "unavailable")
        self.assertEqual(report["postgresql"]["database_inventory"]["status"], "partial")
        report, _ = self.inventory(scenario="timeout")
        self.assertEqual(report["memory"]["reason"], "check_timed_out")

    def test_total_time_budget_stops_later_commands(self):
        report, commands = self.inventory(scenario="budget")
        self.assertEqual(commands, [])
        self.assertEqual(report["memory"]["reason"], "inventory_time_budget_exhausted")
        self.assertEqual(report["database_binding"]["status"], "unknown")


if __name__ == "__main__":
    unittest.main()
