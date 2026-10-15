#!/usr/bin/env python3
"""Synthetic input/recovery tests, never a native Mac or model execution."""
import copy
import importlib
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import uuid

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
first = importlib.import_module("onboarding_first_run")
real_prerequisite = first.prerequisite
files = importlib.import_module("onboarding_files")
journey = importlib.import_module("onboarding_journey")
import test_onboarding_operator as support
import test_onboarding_journey as journey_support


class FirstRunContract(unittest.TestCase):
    def setUp(self):
        self.fixture = support.OperatorContract()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        helper = Path(journey.operator.__file__).parent / "onboarding_journey.py"
        helper.write_bytes((MACOS / "onboarding_journey.py").read_bytes())
        helper.chmod(0o400)
        patches = [patch.object(journey, "__file__", str(helper)), patch.object(first, "native_user")]
        self.installed = self.fixture.input["memory"]
        self.descriptor = {"schema_version": 1, "postgres": self.fixture.input["postgres"], "acceptance_pending": ["native_macos_architecture"]}
        patches.append(patch.object(first, "prerequisite", return_value=(self.descriptor, self.installed)))
        for value in patches:
            value.start()
            self.addCleanup(value.stop)
        self.repositories = self.root / "repositories"
        self.repositories.mkdir(mode=0o700)
        self.state = self.fixture.parent / "first-run"
        sample = journey_support.FakeMemory(self.fixture, {"company": self.fixture.metadata["company"]}).manifest
        self.entries = [{"manifest": sample, "allowed_roles": ["reader"], "offline_available": True}]
        self.native = self.fixture.fake
        self.logins = 0
        self.executions = []
        self.args = dict(company=self.fixture.metadata["company"], name="Synthetic Mac", runtime=self.root / "runtime-prerequisites.json",
                         memory=self.fixture.memory, state=self.state, workspace_parent=self.repositories,
                         native=self.native, memory_factory=self.memory, journey_runner=self.execute_journey)

    def memory(self, source):
        cli = journey.Memory(source)
        cli.login = lambda: setattr(self, "logins", self.logins + 1)
        return cli

    def execute_journey(self, source, journal, native):
        self.executions.append((source, journal, native))
        return {"native_acceptance": "unverified"}

    def invoke(self, answers=("1", ""), **changes):
        text = json.dumps({"schema_version": 1, "projects": self.entries}) + "\n" + first.CATALOG_SUFFIX
        replies = iter(answers)
        with patch.object(self.native, "command", return_value=(0, text)), patch("sys.stdout", new=io.StringIO()):
            return first.run(**dict(self.args, question=lambda _: next(replies), **changes))

    def test_simple_selection_builds_native_inputs_without_handwritten_json(self):
        result = self.invoke()
        source = files.read(self.state / "journey-input.json")
        self.assertEqual(source["schema_version"], 2)
        slot = source["projects"][0]
        manifest = self.entries[0]["manifest"]
        self.assertEqual(slot["selection"]["manifest_digest"], files.digest(manifest))
        self.assertEqual(slot["selection"]["project_id"], manifest["project_id"])
        self.assertEqual([a["name"] for a in source["native"]["areas"]], ["workshop"])
        self.assertEqual(self.logins, 1)
        self.assertEqual(len(self.executions), 1)
        self.assertEqual(result["native_acceptance"], "unverified")
        self.assertFalse(Path(source["native"]["root"]).exists())
        for forbidden in ["credential", "password", "local_database_url", "poll_secret"]:
            self.assertNotIn('"' + forbidden + '"', (self.state / "first-run.json").read_text())

    def test_curated_selection_requires_explicit_permission_even_with_writer_role(self):
        with self.assertRaises(files.Refused):
            self.invoke(curated_sync=True)
        self.entries[0]["allowed_roles"] = ["reader", "writer"]
        with self.assertRaises(files.Refused):
            self.invoke(curated_sync=True)
        self.entries[0]["curated_contributions_allowed"] = False
        with self.assertRaises(files.Refused):
            self.invoke(curated_sync=True)
        self.entries[0]["allowed_roles"] = ["reader"]
        self.entries[0]["curated_contributions_allowed"] = True
        self.invoke(curated_sync=True)
        source = files.read(self.state / "journey-input.json")
        self.assertEqual(source["native"]["schema_version"], 2)
        self.assertTrue(source["native"]["curated_sync"])
        self.assertEqual(source["native"]["repositories"][0]["project_id"], self.entries[0]["manifest"]["project_id"])
        self.assertEqual(source["native"]["repositories"][0]["repository"], source["projects"][0]["workspace"])
        with self.assertRaises(files.Refused):
            self.invoke(curated_sync=False)

    def test_rust_flattened_catalog_preserves_manifest_digest_and_curated_reader_permission(self):
        manifest = self.entries[0]["manifest"]
        manifest.update(manifest.pop("entry"))
        original = copy.deepcopy(manifest)
        self.entries[0]["curated_contributions_allowed"] = True
        self.invoke(curated_sync=True)
        source = files.read(self.state / "journey-input.json")
        self.assertEqual(source["projects"][0]["selection"]["manifest_digest"], files.digest(original))
        self.assertEqual(self.entries[0]["manifest"], original)
        self.assertEqual(source["native"]["repositories"][0]["project"], original["project"])

    def test_catalog_rejects_mixed_manifest_and_nonboolean_curated_permission(self):
        manifest = self.entries[0]["manifest"]
        manifest["area"] = manifest["entry"]["area"]
        with self.assertRaises(files.Refused):
            self.invoke()
        del manifest["area"]
        for invalid in [1, "true", None, [], {}]:
            with self.subTest(permission=invalid):
                self.entries[0]["curated_contributions_allowed"] = invalid
                with self.assertRaises(files.Refused):
                    self.invoke()

    def test_exact_resume_keeps_selection_local_ids_and_existing_repository(self):
        existing = self.repositories / "Unicode café project"
        existing.mkdir(mode=0o700)
        dirty = existing / "uncommitted.txt"
        dirty.write_text("preserve me")
        self.invoke(answers=("1", str(existing)), prepare_only=True)
        original = files.read(self.state / "journey-input.json")
        self.entries = []  # Resume must not repick, clone, or regenerate IDs.
        self.invoke(answers=())
        self.assertEqual(files.read(self.state / "journey-input.json"), original)
        self.assertEqual(self.logins, 1)
        self.assertEqual(dirty.read_text(), "preserve me")
        self.assertEqual(len(self.executions), 1)

    def test_selection_intent_recovers_crash_before_input_publication(self):
        original = files.missing
        def crash(file, data):
            if Path(file).name == "journey-input.json":
                raise files.Refused("synthetic crash after intent")
            return original(file, data)
        with patch.object(files, "missing", side_effect=crash), self.assertRaises(files.Refused):
            self.invoke()
        receipt = files.read(self.state / "first-run.json")
        self.assertIsNotNone(receipt["input_digest"])
        self.assertFalse((self.state / "journey-input.json").exists())
        self.invoke(answers=())
        self.assertEqual(self.logins, 1)
        self.assertEqual(files.read(self.state / "journey-input.json"), receipt["prepared_input"])

    def test_composite_same_named_projects_use_separate_paths_and_area_services(self):
        second = copy.deepcopy(self.entries[0])
        second["manifest"]["entry"]["area"] = "homelab"
        second["manifest"]["entry"]["owner_instance_id"] = str(uuid.uuid4())
        self.entries.append(second)
        self.invoke(answers=("1, 2", "", ""), prepare_only=True)
        source = files.read(self.state / "journey-input.json")
        self.assertEqual(source["projects"][0]["selection"]["project_id"], source["projects"][1]["selection"]["project_id"])
        self.assertNotEqual(source["projects"][0]["workspace"], source["projects"][1]["workspace"])
        areas = source["native"]["areas"]
        self.assertEqual({a["name"] for a in areas}, {"workshop", "homelab"})
        self.assertEqual(len({a["instance_id"] for a in areas}), 2)
        self.assertEqual(len({a["api_port"] for a in areas}), 2)

    def test_trust_is_visible_remote_only_and_cannot_be_selected(self):
        trust = copy.deepcopy(self.entries[0])
        trust["manifest"]["entry"].update(area="trust", offline_allowed=False)
        trust["offline_available"] = False
        self.entries.append(trust)
        with self.assertRaisesRegex(files.Refused, "Trust"):
            self.invoke(answers=("2",))
        self.assertEqual(self.executions, [])
        self.assertFalse((self.state / "journey-input.json").exists())

    def test_modified_inputs_runtime_or_memory_block_resume_without_effects(self):
        self.invoke(prepare_only=True)
        with self.assertRaises(files.Refused):
            self.invoke(name="Changed name")
        self.descriptor["acceptance_pending"] = ["changed descriptor"]
        with self.assertRaises(files.Refused):
            self.invoke()
        self.descriptor["acceptance_pending"] = ["native_macos_architecture"]
        source = files.read(self.state / "journey-input.json")
        source["projects"][0]["selection"]["revision"] = 2
        self.fixture.write(self.state / "journey-input.json", source)
        with self.assertRaises(files.Refused):
            self.invoke()
        self.assertEqual(self.executions, [])

    def test_bad_ports_public_state_and_unbound_inputs_fail(self):
        for changes in [dict(pg_port=55440), dict(api_base_port=65534), dict(company="http://non-loopback.invalid")]:
            with self.assertRaises(files.Refused):
                self.invoke(**changes)
        self.state.mkdir(mode=0o755)
        with self.assertRaises(files.Refused):
            self.invoke()
        self.state.chmod(0o700)  # Test-owned fixture only, not installer repair.
        files.missing(self.state / "journey-input.json", files.encoded({"schema_version": 2}))
        with self.assertRaises(files.Refused):
            self.invoke()
        self.assertEqual(self.logins, 0)

    def test_catalog_fails_closed_on_duplicate_roles_policy_and_extra_output(self):
        valid = {"schema_version": 1, "projects": self.entries}
        self.assertEqual(first.catalog(json.dumps(valid)), self.entries)
        for text in [json.dumps(valid) + "private output", '{"schema_version":1,"schema_version":1,"projects":[]}',
                     json.dumps(dict(valid, schema_version=True)), json.dumps(dict(valid, projects=self.entries * 2))]:
            with self.assertRaises(files.Refused):
                first.catalog(text)
        for key, value in [("allowed_roles", ["admin"]), ("offline_available", "yes")]:
            entry = copy.deepcopy(self.entries[0])
            entry[key] = value
            with self.assertRaises(files.Refused):
                first.catalog(json.dumps(dict(valid, projects=[entry])))

    def test_native_platform_and_eof_denied_without_batch_execution(self):
        with patch.object(first, "native_user", side_effect=files.Refused("native Mac required")), self.assertRaises(files.Refused):
            self.invoke()
        self.assertFalse(self.state.exists())
        with patch("builtins.input", side_effect=EOFError), self.assertRaisesRegex(files.Refused, "cancelled"):
            first.ask("no")

    def test_reviewed_runtime_python_and_memory_version_pins_before_login(self):
        descriptor = dict(self.descriptor, python={"path": str(Path(sys.executable).absolute()),
                           "version": ".".join(str(n) for n in sys.version_info[:3])})
        with patch.object(first.runtime_bundle, "verify_installed", return_value=descriptor), \
             patch.object(self.native, "command", return_value=(0, "memory 2.1.0-fixture")) as command:
            _, installed = real_prerequisite(self.args["runtime"], self.args["memory"], self.native)
            self.assertEqual(installed["sha256"], files.sha(self.fixture.memory))
            self.assertEqual(command.call_args_list[0].args[0][:2], ["/usr/bin/lipo", "-verify_arch"])
            self.assertEqual(command.call_args_list[1].args[0], [self.fixture.memory, "--version"])
            descriptor["python"]["version"] = "3.11.999"
            with self.assertRaisesRegex(files.Refused, "Python"):
                real_prerequisite(self.args["runtime"], self.args["memory"], self.native)
        descriptor["python"]["version"] = ".".join(str(n) for n in sys.version_info[:3])
        with patch.object(first.runtime_bundle, "verify_installed", return_value=descriptor), \
             patch.object(self.native, "command", return_value=(0, "unexpected output")), self.assertRaises(files.Refused):
            real_prerequisite(self.args["runtime"], self.args["memory"], self.native)


if __name__ == "__main__":
    unittest.main()
