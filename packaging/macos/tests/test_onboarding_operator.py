#!/usr/bin/env python3
"""Synthetic filesystem/commands only: this is not native Mac acceptance."""
import importlib
import json
import os
from pathlib import Path
import plistlib
import stat
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import uuid

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
operator = importlib.import_module("onboard")
files = importlib.import_module("onboarding_files")
native_module = importlib.import_module("onboarding_native")


class FakeNative:
    def __init__(self):
        self.commands, self.sql_calls, self.labels, self.bootstraps = [], [], set(), []
        self.reviews = 0
        self.lose_init = self.lose_bootstrap = self.revoked = self.mismatch = False
    def preflight(self, plan):
        if self.mismatch:
            raise files.Refused("synthetic architecture/prerequisite mismatch")
        for pin in plan["pins"]:
            if files.sha(pin["path"]) != pin["sha256"]:
                raise files.Refused("synthetic changed prerequisite")
    def review(self, plan):
        self.reviews += 1
        if self.revoked:
            raise files.Refused("synthetic current scope denied/revoked")
    def command(self, args, **kwargs):
        args = [str(a) for a in args]
        self.commands.append(args)
        if Path(args[0]).name == "initdb":
            data = Path(args[args.index("-D") + 1])
            data.mkdir(mode=0o700, exist_ok=True)
            (data / "PG_VERSION").write_text("16\n")
            (data / "PG_VERSION").chmod(0o600)
            if self.lose_init:
                self.lose_init = False
                raise files.Refused("synthetic lost initdb confirmation")
        return 0, "Database system identifier: 12345"
    def loaded(self, label):
        return label in self.labels
    def available(self, port):
        pass
    def health(self, area):
        pass
    def bootstrap(self, file):
        data = plistlib.loads(Path(file).read_bytes())
        self.labels.add(data["Label"])
        self.bootstraps.append(data)
        if self.lose_bootstrap:
            self.lose_bootstrap = False
            raise files.Refused("synthetic accepted bootstrap reply lost")
    def sql(self, plan, root, database, script, **kwargs):
        self.sql_calls.append((database, kwargs.get("role", "memory_onboarding_admin"), script))
        return "t"
    def wait(self, *args, **kwargs):
        self.sql(*args, **kwargs)


class OperatorContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="memory-native-operator-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.home = self.root / "home"
        self.home.mkdir(mode=0o700)
        (self.home / "Library").mkdir(mode=0o700)
        (self.home / "Library/LaunchAgents").mkdir(mode=0o700)
        self.home_patch = patch.object(Path, "home", return_value=self.home)
        self.home_patch.start()
        self.addCleanup(self.home_patch.stop)
        self.tools = self.root / "installed"
        self.tools.mkdir(mode=0o700)
        # Model immutable packaged helpers, not this group-writable source
        # checkout. Do not weaken production ancestor protection for tests.
        helper = self.tools / "operator"
        helper.mkdir(mode=0o700)
        for name in ["onboard.py", "onboarding_files.py", "onboarding_native.py"]:
            (helper / name).write_bytes((MACOS / name).read_bytes())
            (helper / name).chmod(0o400)
        for module, name in [(operator, "onboard.py"), (files, "onboarding_files.py")]:
            patched = patch.object(module, "__file__", str(helper / name))
            patched.start()
            self.addCleanup(patched.stop)
        self.parent = self.root / "private-data"
        self.parent.mkdir(mode=0o700)
        self.pg = self.tools / "pg"
        self.pg.mkdir(mode=0o700)
        tool_pins = {}
        for name in operator.TOOLS:
            file = self.pg / name
            file.write_bytes(b"synthetic Mach-O fixture, never executed: " + name.encode())
            file.chmod(0o500)
            tool_pins[name] = files.sha(file)
        self.memory = self.tools / "memory"
        self.memory.write_bytes(b"synthetic Memory fixture, never executed")
        self.memory.chmod(0o500)
        control, library, sql = [self.pg / name for name in ["vector.control", "vector.so", "vector--0.8.0.sql"]]
        for file in [control, library, sql]:
            file.write_bytes(b"synthetic pgvector fixture")
            file.chmod(0o400)
        self.metadata_file = self.root / "subscription.json"
        self.metadata = self.subscription("workshop")
        self.write(self.metadata_file, self.metadata)
        self.input = {"schema_version": 1, "root": str(self.parent / "selected-installation"),
                      "memory": {"path": str(self.memory), "sha256": files.sha(self.memory), "version": "memory fixture"},
                      "postgres": {"bin_dir": str(self.pg), "major": 16, "tool_sha256": tool_pins,
                                   "vector_library": str(library), "vector_library_sha256": files.sha(library),
                                   "vector_control": str(control), "vector_control_sha256": files.sha(control),
                                   "vector_version": "0.8.0", "vector_sql": {sql.name: files.sha(sql)}},
                      "pg_port": 55432, "areas": [{"name": "workshop", "instance_id": str(uuid.uuid4()), "api_port": 55440}],
                      "subscriptions": [str(self.metadata_file)]}
        self.input_file, self.journal = self.root / "input.json", self.root / "journal.json"
        self.write(self.input_file, self.input)
        self.fake = FakeNative()
    def write(self, file, value):
        file.write_bytes(files.encoded(value))
        file.chmod(0o600)
    def subscription(self, area):
        return {"schema_version": 1, "company": "https://company.example.invalid", "subscription": str(uuid.uuid4()),
                "allow_loopback_development": False, "bootstrap": {"schema_version": 1, "device": str(uuid.uuid4()),
                "authority": str(uuid.uuid4()), "catalog_revision": 1, "receipt_keys": [{"public_key": "synthetic public metadata"}],
                "scope": {"company": str(uuid.uuid4()), "area": area, "owner": str(uuid.uuid4()), "project": str(uuid.uuid4()), "space": str(uuid.uuid4())}}}
    def prepare(self):
        return operator.prepare(self.input_file, self.journal, self.fake)
    def apply(self, value=None):
        value = value or self.prepare()
        return operator.apply(self.journal, value["plan_sha256"], True, self.fake)
    def test_preview_and_exact_replay_do_not_start_or_provision(self):
        value = self.prepare()
        self.assertEqual(self.prepare(), value)
        self.assertFalse(Path(self.input["root"]).exists())
        self.assertEqual(self.fake.commands, [])
        self.assertEqual(self.fake.bootstraps, [])
        self.assertNotIn("local_database_url", json.dumps(value))
    def test_confirm_and_prerequisites_before_native_effects(self):
        value = self.prepare()
        for digest, confirm in [(value["plan_sha256"], False), ("0" * 64, True)]:
            with self.assertRaises(files.Refused):
                operator.apply(self.journal, digest, confirm, self.fake)
        self.fake.mismatch = True
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.assertFalse(Path(self.input["root"]).exists())
        self.assertEqual(self.fake.commands, [])
    def test_trust_remote_only_rejected_and_existing_install_never_adopted(self):
        self.metadata["bootstrap"]["scope"]["area"] = "trust"
        self.write(self.metadata_file, self.metadata)
        with self.assertRaises(files.Refused):
            self.prepare()
        self.assertFalse(self.journal.exists())
        self.metadata["bootstrap"]["scope"]["area"] = "workshop"
        self.write(self.metadata_file, self.metadata)
        root = Path(self.input["root"])
        root.mkdir(mode=0o700)
        (root / "keep").write_text("unrelated existing install")
        with self.assertRaises(files.Refused):
            self.prepare()
        self.assertEqual((root / "keep").read_text(), "unrelated existing install")
    def test_current_revoked_scope_before_any_provision(self):
        value = self.prepare()
        self.fake.revoked = True
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.assertFalse(Path(self.input["root"]).exists())
        self.assertEqual(self.fake.commands, [])
    def test_changed_metadata_or_installed_binary_conflicts(self):
        value = self.prepare()
        self.metadata["bootstrap"]["catalog_revision"] += 1
        self.write(self.metadata_file, self.metadata)
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.metadata["bootstrap"]["catalog_revision"] -= 1
        self.write(self.metadata_file, self.metadata)
        self.memory.chmod(0o600)
        self.memory.write_bytes(b"changed prerequisite")
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.assertFalse(Path(self.input["root"]).exists())
    def test_generated_area_config_explicitly_disables_enrichment_defaults(self):
        value = self.prepare()
        area = value["plan"]["input"]["areas"][0]
        config = tomllib.loads(operator.area_config(value, area, {
            "password": "a" * 64, "api_token": "b" * 64,
        }).decode())
        # Explicit leaves override even a future enabled configuration default;
        # empty model prevents call_llm_strict_json reaching key/network lookup.
        for section, key in [("background_work", "enabled"),
                             ("background_work", "internal_automation"),
                             ("reinforcement", "enabled"),
                             ("reinforcement", "validation_enabled"),
                             ("consolidation", "enabled"),
                             ("automation", "enabled"), ("embeddings", "enabled"),
                             ("embeddings", "create_enabled"),
                             ("features", "llm_curation")]:
            inherited = {key: True}
            inherited.update(config[section])
            self.assertIs(inherited[key], False)
        self.assertEqual(config["llm"], {
            "provider": "openai_compatible", "model": "", "api_key_env": "",
            "base_url": "http://127.0.0.1:1",
        })

    def test_three_areas_isolated_profiles_named_jobs_and_no_legacy_effects(self):
        legacy = self.home / "custom-config"
        legacy.write_text("read-only existing config")
        legacy.chmod(0o400)
        self.fake.labels.add("com.memory-layer.mem-service")
        for index, area in enumerate(["homelab", "general"], 1):
            file = self.root / (area + ".json")
            self.write(file, self.subscription(area))
            self.input["subscriptions"].append(str(file))
            self.input["areas"].append({"name": area, "instance_id": str(uuid.uuid4()), "api_port": 55440 + index})
        self.write(self.input_file, self.input)
        value = self.apply()
        root = Path(self.input["root"])
        self.assertEqual(len(self.fake.bootstraps), 7)
        jobs = [data for data in self.fake.bootstraps if ".sync." in data["Label"]]
        self.assertTrue(all(j["StartInterval"] == 300 and j["RunAtLoad"] for j in jobs))
        self.assertEqual(legacy.read_text(), "read-only existing config")
        self.assertEqual(stat.S_IMODE(legacy.stat().st_mode), 0o400)
        for file in (root / "profiles").iterdir():
            profile = files.read(file)
            area = profile["bootstrap"]["scope"]["area"]
            self.assertIn("@127.0.0.1:55432/memory_laptop_" + area, profile["local_database_url"])
            self.assertEqual(stat.S_IMODE(file.stat().st_mode), 0o600)
        raw = json.dumps(value)
        secret = files.read(root / "local-secrets.json")
        self.assertNotIn(secret["admin"], raw)
        for s in secret["areas"].values():
            self.assertNotIn(s["password"], raw)
            self.assertNotIn(s["api_token"], raw)
        self.assertFalse(any("restart" in " ".join(args) or "stop" in args for args in self.fake.commands))
    def test_lost_initdb_reply_resumes_same_cluster_without_reinitialization(self):
        value = self.prepare()
        self.fake.lose_init = True
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.apply(value)
        self.assertEqual(sum(Path(args[0]).name == "initdb" for args in self.fake.commands), 1)
        self.assertEqual(files.read(self.journal)["request_id"], value["request_id"])
    def test_lost_bootstrap_reply_no_duplicate_and_missing_own_job_reconciles(self):
        value = self.prepare()
        self.fake.lose_bootstrap = True
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.apply(value)
        self.assertEqual(len(self.fake.bootstraps), 3)
        self.apply(value)
        self.assertEqual(len(self.fake.bootstraps), 3)
        own = next(label for label in self.fake.labels if ".area." in label)
        self.fake.labels.remove(own)
        self.apply(value)
        self.assertEqual(len(self.fake.bootstraps), 4)
    def test_tampered_owned_settings_preserved_no_resurrection(self):
        value = self.apply()
        config = Path(self.input["root"]) / "workshop.toml"
        config.write_text("custom changed settings")
        before = len(self.fake.bootstraps)
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.assertEqual(config.read_text(), "custom changed settings")
        self.assertEqual(len(self.fake.bootstraps), before)
    def test_symlink_and_hardlink_metadata_rejected(self):
        alias = self.root / "alias.json"
        alias.symlink_to(self.metadata_file)
        with self.assertRaises(files.Refused):
            files.read(alias)
        alias.unlink()
        os.link(self.metadata_file, alias)
        with self.assertRaises(files.Refused):
            files.read(self.metadata_file)
    def test_native_has_no_linux_root_or_fake_environment_bypass(self):
        value = self.prepare()
        with patch.object(native_module.sys, "platform", "linux"):
            with self.assertRaisesRegex(files.Refused, "Darwin"):
                operator.apply(self.journal, value["plan_sha256"], True, native_module.Native())
        self.assertFalse(Path(self.input["root"]).exists())
    def test_job_exec_uses_only_pinned_binary_and_local_profile_not_secret_argv(self):
        self.apply()
        calls = []
        with patch.object(os, "execve", side_effect=lambda exe, args, env: calls.append((exe, args, env))):
            operator.run_job(self.journal, "run-sync", self.metadata["subscription"], self.fake)
            operator.run_job(self.journal, "run-service", "workshop", self.fake)
            operator.run_job(self.journal, "run-postgres", None, self.fake)
        self.assertEqual(calls[0][1][1:3], ["projects", "sync"])
        self.assertEqual(calls[1][1][-2:], ["service", "run"])
        self.assertIn("listen_addresses=127.0.0.1", calls[2][1])
        self.assertNotIn("OPENAI_API_KEY", calls[0][2])
        self.assertNotIn("PGPASSWORD", calls[0][2])
        for secret in files.read(Path(self.input["root"]) / "local-secrets.json")["areas"].values():
            self.assertNotIn(secret["password"], repr(calls))
    def test_journal_inside_root_or_case_alias_fails_without_creation(self):
        self.input["root"] = str(self.parent / "existing")
        self.write(self.input_file, self.input)
        root = Path(self.input["root"])
        root.mkdir(mode=0o700)
        with self.assertRaises(files.Refused):
            operator.prepare(self.input_file, root / "journal.json", self.fake)
        self.assertEqual(list(root.iterdir()), [])
        with self.assertRaises(files.Refused):
            files.outside(self.parent / "new", self.parent / "NEW")
    def test_remote_database_fields_cannot_enter_prepared_metadata(self):
        self.metadata["local_database_url"] = "postgresql://server:secret@remote/database"
        self.write(self.metadata_file, self.metadata)
        with self.assertRaises(files.Refused):
            self.prepare()
        self.assertFalse(self.journal.exists())
    def test_lost_cluster_marker_and_hba_changes_are_not_reset(self):
        value = self.apply()
        root = Path(self.input["root"])
        marker = root / "cluster/PG_VERSION"
        marker.unlink()
        before = len(self.fake.commands)
        with self.assertRaises((OSError, files.Refused)):
            self.apply(value)
        self.assertEqual(len(self.fake.commands), before)
        marker.write_text("16\n")
        marker.chmod(0o600)
        (root / "pg_hba.conf").write_text("host all all 0.0.0.0/0 trust\n")
        with self.assertRaises(files.Refused):
            self.apply(value)
        self.assertIn("trust", (root / "pg_hba.conf").read_text())
    def test_actual_native_architecture_gate_before_installed_binary_execution(self):
        value = self.prepare()
        calls = []
        class Mismatched(native_module.Native):
            def command(self, args, **kwargs):
                calls.append(args)
                raise files.Refused("synthetic lipo architecture mismatch")
        with patch.object(native_module.sys, "platform", "darwin"), patch.object(os, "getuid", return_value=1000), patch.object(os, "geteuid", return_value=1000), patch.object(native_module.platform, "machine", return_value="arm64"):
            with self.assertRaisesRegex(files.Refused, "architecture mismatch"):
                Mismatched().preflight(value["plan"])
        self.assertEqual(calls[0][:3], ["/usr/bin/lipo", "-verify_arch", "arm64"])
        self.assertEqual(len(calls), 1)
    def test_real_native_review_uses_cli_preview_contract_and_confirmation_digest(self):
        value = self.prepare()
        calls = []
        selected = value["plan"]["selections"][0]
        class ScriptedCli(native_module.Native):
            def command(self, args, **kwargs):
                import argparse
                # Mirrors the producer's actual published flag/JSON contract;
                # no Rust binary, Keychain or company network call is executed.
                parser = argparse.ArgumentParser()
                for flag in ["company", "project", "area", "owner-instance", "revision", "output"]:
                    parser.add_argument("--" + flag, required=True)
                parser.add_argument("--bootstrap-sha256")
                parser.add_argument("--confirm", action="store_true")
                parser.add_argument("--allow-loopback", action="store_true")
                parsed = parser.parse_args(args[3:])
                calls.append(parsed)
                if parsed.confirm:
                    if parsed.bootstrap_sha256 != "e" * 64:
                        raise files.Refused("CLI requires its exact preview digest")
                    return 0, "accepted exact replay; unchanged"
                return 0, json.dumps({"company": selected["metadata"]["company"], "bootstrap": selected["metadata"]["bootstrap"],
                                     "bootstrap_sha256": "e" * 64, "output": selected["path"],
                                     "contains_database_credentials": False, "creates_services": False, "downloads_knowledge": False})
        ScriptedCli().review(value["plan"])
        self.assertEqual(len(calls), 2)
        self.assertFalse(calls[0].confirm)
        self.assertIsNone(calls[0].bootstrap_sha256)
        self.assertTrue(calls[1].confirm)
        self.assertEqual(calls[1].bootstrap_sha256, "e" * 64)
        self.assertEqual(files.read(self.metadata_file)["subscription"], self.metadata["subscription"])
    def test_real_native_review_rejects_changed_preview_without_confirming(self):
        value = self.prepare()
        calls = []
        class Changed(native_module.Native):
            def command(self, args, **kwargs):
                calls.append(args)
                return 0, json.dumps({"company": "https://changed.example.invalid"})
        with self.assertRaises(files.Refused):
            Changed().review(value["plan"])
        self.assertEqual(len(calls), 1)


if __name__ == "__main__":
    unittest.main()
