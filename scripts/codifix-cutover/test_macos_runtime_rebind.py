"""Populated private fixtures only; never native Mac/DB/service/authority effects."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import uuid

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("macos_runtime_rebind", HERE / "macos-runtime-rebind.py")
rebind = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rebind)
files = rebind.files


class Executed(Exception):
    pass


class FakeBoundary(rebind.Boundary):
    def __init__(self, fixture):
        self.fixture = fixture
        self.processes, self.queries, self.preflights = [], [], 0
        self.loaded_labels = set()
        self.system_identifier = "7654321098765432100"
        self.wrong_oid = False
        self.binding_enabled = False
    def preflight(self, plan):
        self.preflights += 1
        for pin in plan["pins"]:
            rebind.require(files.sha(pin["path"]) == pin["sha256"], "changed prerequisite")
    def command(self, args, **kwargs):
        self.processes.append([str(a) for a in args])
        if Path(args[0]).name == "pg_controldata" and args[1] == "-D":
            return 0, "Database system identifier: " + self.system_identifier
        raise AssertionError("unexpected native command")
    def loaded(self, label):
        return label in self.loaded_labels
    def review(self, *_):
        raise AssertionError("authority must never be requested")
    def bootstrap(self, *_):
        raise AssertionError("plists must remain inert")
    def query(self, payload, database, query, role="codifix_onboarding_admin"):
        self.queries.append((database, query, role))
        assert query.startswith("SELECT ")
        if "pg_control_system" in query:
            return {"system_identifier": self.system_identifier, "data_directory": str(self.fixture.new / "cluster"),
                    "port": 55432, "admin_oid": 10, "admin_scram": True}
        if "legacy" in query or "SELECT to_json(EXISTS" in query:
            return False
        name = database.removeprefix("codifix_laptop_")
        item = next(a for a in self.fixture.recon["areas"] if a["name"] == name)
        if "database_oid" in query:
            return {"database_oid": item["database_oid"] + int(self.wrong_oid), "owner_oid": item["role_oid"],
                    "role_oid": item["role_oid"], "role_restricted": True, "project_ids": item["project_ids"],
                    "migrations": copy.deepcopy(self.fixture.migrations), "bindings_disabled": not self.binding_enabled}
        metadata = next(m for m in self.fixture.metadata if m["bootstrap"]["scope"]["area"] == name)
        return {"subscription": metadata["subscription"], "device": metadata["bootstrap"]["device"], "scope": metadata["bootstrap"]["scope"]}


class ColdRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="codifix-cold-runtime-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.root.chmod(0o700)
        self.old = self.root / "old-runtime"
        self.new = self.root / "new-runtime"
        self.new.mkdir(mode=0o700)
        self.archive = self.root / "archive"
        self.archive.mkdir(mode=0o700)
        # Real protected retained layout, independent of the developer checkout.
        bundle = self.root / "retained-3.0.0-rc.1"
        script = bundle / "scripts/codifix-cutover/macos-runtime-rebind.py"
        script.parent.mkdir(parents=True, mode=0o700)
        script.write_bytes((HERE / "macos-runtime-rebind.py").read_bytes())
        script.chmod(0o400)
        macos = bundle / "packaging/macos"
        macos.mkdir(parents=True, mode=0o700)
        source_macos = HERE.parents[1] / "packaging/macos"
        for name in ("onboard.py", "onboarding_files.py", "onboarding_native.py"):
            (macos / name).write_bytes((source_macos / name).read_bytes())
            (macos / name).chmod(0o400)
        for directory in [bundle, *bundle.rglob("*")]:
            if directory.is_dir():
                directory.chmod(0o700)
        for module, file in [(rebind, script), (rebind.onboarding, macos / "onboard.py"),
                             (files, macos / "onboarding_files.py"),
                             (sys.modules[rebind.onboarding.Native.__module__], macos / "onboarding_native.py")]:
            value = patch.object(module, "__file__", str(file))
            value.start()
            self.addCleanup(value.stop)
        tools = self.root / "installed"
        tools.mkdir(mode=0o700)
        candidate = tools / "codifix"
        candidate.write_bytes(b"synthetic candidate; never executed")
        candidate.chmod(0o500)
        pg = tools / "postgres"
        pg.mkdir(mode=0o700)
        pins = {}
        for tool in rebind.onboarding.TOOLS:
            path = pg / tool
            path.write_bytes(b"synthetic native prerequisite " + tool.encode())
            path.chmod(0o500)
            pins[tool] = files.sha(path)
        for name in ("vector.so", "vector.control", "vector--0.8.7.sql"):
            path = pg / name
            path.write_bytes(b"synthetic vector prerequisite")
            path.chmod(0o400)
        postgres = {"bin_dir": str(pg), "major": 17, "tool_sha256": pins, "vector_library": str(pg / "vector.so"),
                    "vector_library_sha256": files.sha(pg / "vector.so"), "vector_control": str(pg / "vector.control"),
                    "vector_control_sha256": files.sha(pg / "vector.control"), "vector_version": "0.8.7",
                    "vector_sql": {"vector--0.8.7.sql": files.sha(pg / "vector--0.8.7.sql")}}
        self.metadata = []
        areas, repositories, selections = [], [], []
        self.secret = {"admin": "a" * 64, "areas": {}}
        self.migrations = [{"version": n, "checksum": "ab" * 48, "success": True} for n in range(1, 86)]
        self.recon = {"format": rebind.RECONCILIATION, "cluster_system_identifier": "7654321098765432100",
                      "admin_role_oid": 10, "preservation_receipt_sha256": "f" * 64, "areas": []}
        for i, name in enumerate(("workshop", "homelab")):
            metadata = {"schema_version": 1, "company": "https://company.example.invalid", "subscription": str(uuid.uuid4()),
                        "allow_loopback_development": False, "replication_mode": "curated_project_v1", "manifest_sha256": "d" * 64,
                        "bootstrap": {"schema_version": 1, "device": str(uuid.uuid4()), "authority": str(uuid.uuid4()),
                                      "catalog_revision": 1, "receipt_keys": [{"public_key": "synthetic public fixture"}],
                                      "scope": {"area": name, **{key: str(uuid.uuid4()) for key in ("company", "owner", "project", "space")}}}}
            self.metadata.append(metadata)
            areas.append({"name": name, "instance_id": str(uuid.uuid4()), "api_port": 55440 + i})
            repositories.append({"area": name, "repository": str(self.root / ("repository-" + name)), "project": name,
                                 "project_id": metadata["bootstrap"]["scope"]["project"]})
            selections.append({"path": str(self.old / "profiles" / (metadata["subscription"] + ".json")), "metadata": metadata, "sha256": "e" * 64})
            self.secret["areas"][name] = {"password": ("b" if i == 0 else "c") * 64, "api_token": ("d" if i == 0 else "e") * 64}
            self.recon["areas"].append({"name": name, "database_oid": 16384 + i, "role_oid": 16400 + i,
                                      "project_ids": [metadata["bootstrap"]["scope"]["project"]], "migrations_sha256": files.digest(self.migrations)})
        old_input = {"schema_version": 2, "root": str(self.old), "memory": {"path": "/archived/memory", "sha256": "a" * 64, "version": "memory 2.1.0"},
                     "postgres": postgres, "pg_port": 55432, "areas": areas, "subscriptions": [s["path"] for s in selections],
                     "curated_sync": True, "repositories": repositories}
        plan = {"input": old_input, "pins": [{"path": "/archived/memory", "sha256": "a" * 64, "kind": "mach-o"}],
                "selections": selections, "operator": "/archived/onboard.py", "python": "/archived/python"}
        stages = {"layout", "cluster", "postgres_agent"}
        stages.update("database:" + a["name"] for a in areas)
        stages.update("area_agent:" + a["name"] for a in areas)
        stages.update("sync_agent:" + m["subscription"] for m in self.metadata)
        stages.update("initial_sync:" + m["subscription"] for m in self.metadata)
        stages.update("watcher_agent:" + str(i) for i in range(2))
        journal = {"schema_version": 1, "request_id": str(uuid.uuid4()), "plan": plan,
                   "plan_sha256": files.digest(plan), "stages": {name: "done" for name in stages}}
        root_receipt = {"request_id": journal["request_id"], "plan_sha256": journal["plan_sha256"]}
        self.write(self.archive / "journal.json", journal)
        self.write(self.archive / "receipt.json", root_receipt)
        self.write(self.archive / "reconciliation.json", self.recon)
        self.write(self.new / "receipt.json", root_receipt)
        self.write(self.new / "local-secrets.json", self.secret)
        self.write(self.new / "admin-password", (self.secret["admin"] + "\n").encode())
        self.write(self.new / "codifix_onboarding_admin.pgpass", f"127.0.0.1:55432:*:codifix_onboarding_admin:{self.secret['admin']}\n".encode())
        (self.new / "profiles").mkdir(mode=0o700)
        (self.new / "cluster").mkdir(mode=0o700)
        self.write(self.new / "cluster/PG_VERSION", b"17\n")
        self.write(self.new / "cluster/postgresql.auto.conf", b"# no ALTER SYSTEM settings\n")
        self.write(self.new / "cluster/populated-data", b"populated mutable fixture; not an initdb marker")
        operational_plan = {"input": {**old_input, "root": str(self.new)}, "selections": selections}
        for area in areas:
            name = area["name"]
            self.write(self.new / (name + ".toml"), rebind.onboarding.area_config({"plan": operational_plan}, area, self.secret["areas"][name]))
            self.write(self.new / ("codifix_laptop_" + name + ".pgpass"), f"127.0.0.1:55432:*:codifix_laptop_{name}:{self.secret['areas'][name]['password']}\n".encode())
        for metadata in self.metadata:
            name = metadata["bootstrap"]["scope"]["area"]
            self.write(self.new / "profiles" / (metadata["subscription"] + ".json"),
                       {**metadata, "local_database_url": rebind.onboarding.database_url({"plan": operational_plan}, name, self.secret["areas"][name]["password"])})
        jobs = []
        suffixes = ["postgres.cluster", "area.workshop", "area.homelab", *["sync." + m["subscription"] for m in self.metadata], "watcher.0", "watcher.1"]
        for i, suffix in enumerate(suffixes):
            label = "com.memory-layer.onboarding." + journal["request_id"] + "." + suffix
            plist = self.archive / (str(i) + ".plist")
            self.write(plist, plistlib.dumps({"Label": label, "ProgramArguments": ["/archived/memory"], "KeepAlive": True}))
            jobs.append({"label": label, "prior_state": "active" if i != 1 else "inactive", "plist": self.pin(plist)})
        self.source = {"format": rebind.INPUT, "old_root": str(self.old), "new_root": str(self.new),
                       "archive": {"journal": self.pin(self.archive / "journal.json"), "root_receipt": self.pin(self.archive / "receipt.json")},
                       "candidate": {"path": str(candidate), "sha256": files.sha(candidate), "version": "codifix 3.0.0-rc.1"},
                       "postgres": postgres, "reconciliation": self.pin(self.archive / "reconciliation.json"), "old_jobs": jobs}
        self.input = self.root / "input.json"
        self.manifest = self.root / "manifest.json"
        self.write(self.input, self.source)
        self.native = FakeBoundary(self)
        self.before = self.original_bytes()
    def write(self, path, value):
        path.write_bytes(value if isinstance(value, bytes) else files.encoded(value))
        path.chmod(0o600)
    def pin(self, path):
        return {"path": str(path), "sha256": files.sha(path)}
    def original_bytes(self):
        return {str(path): path.read_bytes() for base in (self.archive, self.new) for path in base.rglob("*") if path.is_file()}
    def inspect(self):
        sealed = rebind.inspect(self.input, self.native)
        self.write(self.manifest, sealed)
        return sealed
    def bind(self, sealed=None):
        sealed = sealed or self.inspect()
        return rebind.bind(self.manifest, sealed["sha256"], True, self.native)
    def assert_preserved(self):
        for path, contents in self.before.items():
            self.assertEqual(Path(path).read_bytes(), contents, path)
    def test_populated_schema2_cold_bind_preserves_every_original_and_stays_inert(self):
        with patch.object(rebind.onboarding, "apply", side_effect=AssertionError("never apply")), \
             patch.object(rebind.onboarding.secrets, "token_hex", side_effect=AssertionError("never generate credentials")):
            sealed = self.inspect()
            self.assertFalse(rebind.control(sealed["payload"]).exists())
            self.bind(sealed)
        self.assert_preserved()
        root = rebind.control(sealed["payload"])
        for area in ("workshop", "homelab"):
            config = tomllib.loads((root / "configs" / (area + ".toml")).read_text())
            self.assertTrue(config["service"]["read_only"])
            self.assertEqual(config["database"]["migrations"], "verify_only")
            for section in ("automation", "offline", "background_work", "mcp", "cluster", "telemetry", "embeddings", "consolidation", "procedural", "reinforcement"):
                self.assertFalse(config[section]["enabled"], section)
            self.assertFalse(config["background_work"]["internal_automation"])
            self.assertFalse(config["mcp"]["http_enabled"])
            self.assertEqual(config["llm"]["model"], "")
            self.assertNotIn("supervisor", config)
            expected = next(m for m in self.metadata if m["bootstrap"]["scope"]["area"] == area)
            self.assertEqual(config["laptop_cache"]["subscriptions"][0]["subscription_id"], expected["subscription"])
        plists = list((root / "plists").glob("*.plist"))
        self.assertEqual(len(plists), 3)
        for path in plists:
            job = plistlib.loads(path.read_bytes())
            self.assertFalse(job["RunAtLoad"])
            self.assertFalse(job["KeepAlive"])
            self.assertTrue(job["Disabled"])
            self.assertNotIn("StartInterval", job)
            self.assertNotIn("sync", job["ProgramArguments"])
            self.assertNotIn("watcher", job["ProgramArguments"])
        self.assertTrue(all(Path(command[0]).name == "pg_controldata" for command in self.native.processes))
        self.assertEqual(self.bind(sealed), rebind.receipt(sealed))
        self.assert_preserved()
    def test_missing_confirmation_and_foreign_managed_root_refused(self):
        sealed = self.inspect()
        for digest, stopped in [("0" * 64, True), (sealed["sha256"], False)]:
            with self.assertRaises(files.Refused):
                rebind.bind(self.manifest, digest, stopped, self.native)
        managed = rebind.control(sealed["payload"])
        managed.mkdir(mode=0o700)
        self.write(managed / "foreign", b"retain foreign data")
        with self.assertRaises(files.Refused):
            self.bind(sealed)
        self.assertEqual((managed / "foreign").read_bytes(), b"retain foreign data")
    def test_interruption_never_runs_and_exact_retry_completes_receipt_last(self):
        sealed = self.inspect()
        publish = files.missing
        def interrupted(path, value):
            if str(path).endswith("configs/homelab.toml"):
                raise OSError("synthetic interrupted publication")
            return publish(path, value)
        with patch.object(files, "missing", side_effect=interrupted), self.assertRaises(OSError):
            self.bind(sealed)
        root = rebind.control(sealed["payload"])
        self.assertFalse((root / "receipt.json").exists())
        with patch.object(os, "execve", side_effect=AssertionError("partial must not exec")), self.assertRaises(files.Refused):
            rebind.run(self.manifest, "area", "workshop", self.native)
        self.bind(sealed)
        self.assertTrue((root / "receipt.json").exists())
        self.assert_preserved()
    def test_tampered_partial_file_is_preserved_and_never_overwritten(self):
        sealed = self.inspect()
        root = rebind.control(sealed["payload"])
        self.bind(sealed)
        target = root / "configs/workshop.toml"
        self.write(target, b"preserve altered bytes")
        with self.assertRaises(files.Refused):
            self.bind(sealed)
        self.assertEqual(target.read_bytes(), b"preserve altered bytes")
    def test_wrong_cluster_oid_or_active_binding_refuses_before_bind(self):
        for attribute, value in [("system_identifier", "9999999999999999999"), ("wrong_oid", True), ("binding_enabled", True)]:
            previous = getattr(self.native, attribute)
            setattr(self.native, attribute, value)
            with self.subTest(attribute=attribute), self.assertRaises(files.Refused):
                self.inspect()
            setattr(self.native, attribute, previous)
        self.assertFalse((self.new / rebind.CONTROL).exists())
    def test_old_job_loaded_or_old_root_present_refuses(self):
        self.native.loaded_labels.add(self.source["old_jobs"][0]["label"])
        with self.assertRaises(files.Refused):
            self.inspect()
        self.native.loaded_labels.clear()
        self.old.mkdir(mode=0o700)
        with self.assertRaises(files.Refused):
            self.inspect()
    def test_changed_pin_receipt_credential_or_archive_blocks_run(self):
        sealed = self.inspect()
        self.bind(sealed)
        targets = [Path(self.source["candidate"]["path"]), self.new / "receipt.json", self.new / "codifix_laptop_workshop.pgpass", self.archive / "journal.json"]
        for target in targets:
            old = target.read_bytes()
            mode = target.stat().st_mode & 0o777
            target.chmod(0o600)
            target.write_bytes(b"changed synthetic prerequisite")
            with self.subTest(target=target.name), patch.object(os, "execve", side_effect=AssertionError("never exec changed state")), self.assertRaises(files.Refused):
                rebind.run(self.manifest, "area", "workshop", self.native)
            target.write_bytes(old)
            target.chmod(mode)
    def test_symlinked_bound_config_and_credential_are_rejected(self):
        sealed = self.inspect()
        self.bind(sealed)
        for target in [rebind.control(sealed["payload"]) / "configs/workshop.toml", self.new / "codifix_laptop_workshop.pgpass"]:
            original = target.with_suffix(target.suffix + ".original")
            target.rename(original)
            target.symlink_to(original)
            with self.subTest(target=target.name), self.assertRaises(files.Refused):
                rebind.run(self.manifest, "area", "workshop", self.native)
            target.unlink()
            original.rename(target)
    def test_command_selection_and_clean_environment_and_mutable_cluster_data(self):
        sealed = self.inspect()
        self.bind(sealed)
        for kind, area in [("sync", None), ("watcher", "workshop"), ("initdb", None)]:
            before = self.native.preflights
            with self.assertRaises(files.Refused):
                rebind.run(self.root / "not-read.json", kind, area, self.native)
            self.assertEqual(self.native.preflights, before)
        with self.assertRaises(files.Refused):
            rebind.run(self.manifest, "area", "trust", self.native)
        self.write(self.new / "cluster/populated-data", b"legitimate mutable cluster bytes differ")
        for kind, area in [("postgres", None), ("area", "workshop")]:
            with patch.dict(os.environ, {"CODIFIX_FEDERATION_CONFIG": "forbidden", "OPENAI_API_KEY": "synthetic-forbidden"}), \
                 patch.object(os, "execve", side_effect=Executed) as execute, self.assertRaises(Executed):
                rebind.run(self.manifest, kind, area, self.native)
            executable, arguments, env = execute.call_args.args
            self.assertNotIn("CODIFIX_FEDERATION_CONFIG", env)
            self.assertNotIn("OPENAI_API_KEY", env)
            if kind == "area":
                self.assertEqual(arguments[-2:], ["service", "run"])
                self.assertIn(str(rebind.control(sealed["payload"]) / "configs/workshop.toml"), arguments)
            else:
                self.assertEqual(Path(executable).name, "postgres")
                self.assertNotIn("initdb", arguments)
    def test_incomplete_archived_stages_and_runtime_upgrade_are_refused(self):
        artifact = self.archive / "journal.json"
        journal = files.read(artifact)
        journal["stages"]["cluster"] = "intent"
        self.write(artifact, journal)
        self.source["archive"]["journal"] = self.pin(artifact)
        self.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.inspect()
        journal["stages"]["cluster"] = "done"
        self.write(artifact, journal)
        self.source["archive"]["journal"] = self.pin(artifact)
        self.source["postgres"]["vector_version"] = "9.9.9"
        self.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.inspect()

    def test_helper_bundle_pin_drift_and_unrecognized_files_are_refused(self):
        sealed = self.inspect()
        self.bind(sealed)
        helper = Path(rebind.onboarding.__file__)
        contents = helper.read_bytes()
        helper.chmod(0o600)
        helper.write_bytes(contents + b"\n# changed fixture\n")
        with self.assertRaises(files.Refused):
            rebind.run(self.manifest, "area", "workshop", self.native)
        helper.write_bytes(contents)
        helper.chmod(0o400)
        unexpected = rebind.control(sealed["payload"]) / "foreign-file"
        self.write(unexpected, b"preserve foreign content")
        with self.assertRaises(files.Refused):
            self.bind(sealed)
        self.assertEqual(unexpected.read_bytes(), b"preserve foreign content")

    def test_final_receipt_or_manifest_or_old_job_drift_blocks_run(self):
        sealed = self.inspect()
        self.bind(sealed)
        root = rebind.control(sealed["payload"])
        for target in (root / "receipt.json", root / "manifest.json"):
            content = target.read_bytes()
            self.write(target, b"{}")
            with self.subTest(target=target.name), self.assertRaises(files.Refused):
                rebind.run(self.manifest, "area", "workshop", self.native)
            self.write(target, content)
        self.native.loaded_labels.add(self.source["old_jobs"][0]["label"])
        with self.assertRaises(files.Refused):
            rebind.run(self.manifest, "postgres", None, self.native)


if __name__ == "__main__":
    unittest.main()
