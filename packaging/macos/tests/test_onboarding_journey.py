#!/usr/bin/env python3
"""Synthetic Memory/native commands only; no Mac/provider/network acceptance."""
import copy
import importlib
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch
import uuid

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
journey = importlib.import_module("onboarding_journey")
files = importlib.import_module("onboarding_files")
import test_onboarding_operator as support


class FakeMemory:
    def __init__(self, fixture, source):
        self.fixture, self.source = fixture, source
        self.calls = []
        self.bootstrap = copy.deepcopy(fixture.metadata["bootstrap"])
        self.manifest = {"project_id": self.bootstrap["scope"]["project"], "project": "same-name",
                         "entry": {"area": "workshop", "owner_instance_id": self.bootstrap["scope"]["owner"],
                                   "repository_url": "https://git.example.invalid/repo.git", "default_branch": "main",
                                   "offline_allowed": True, "tools": [], "destinations": []},
                         "revision": 1, "reviewed_by": str(uuid.uuid4()), "reviewed_at": "2026-10-05T12:00:00Z"}
        self.cancel_select = self.lose_select = self.lose_apply = self.lose_subscription = False
        self.revoked = self.unsupported_tool = False
        self.clones = 0

    def login(self):
        self.calls.append("login")

    def select(self, slot):
        self.calls.append("select")
        if self.cancel_select:
            return
        plan = {"company": self.source["company"], "device_id": self.bootstrap["device"],
                "workspace": slot["workspace"], "manifest": copy.deepcopy(self.manifest), "manifest_sha256": "b" * 64,
                "repository_existing": Path(slot["workspace"]).exists(), "templates": []}
        files.missing(slot["journal"], files.encoded({"schema_version": 1, "request_id": str(uuid.uuid4()), "plan": plan,
                      "plan_sha256": "a" * 64, "repository": "pending", "templates_done": False, "tools_done": False, "preserved_paths": []}))
        if self.lose_select:
            self.lose_select = False
            raise files.Refused("synthetic metadata selection reply lost")

    def workspace_status(self, file):
        self.calls.append("workspace_status")
        return files.read(file)

    def apply(self, journal, digest):
        self.calls.append(("apply", str(journal), digest))
        if self.revoked or self.unsupported_tool:
            raise files.Refused("synthetic denied/revoked or unsupported tool")
        value = files.read(journal)
        if value["plan_sha256"] != digest:
            raise files.Refused("synthetic digest mismatch")
        root = Path(value["plan"]["workspace"])
        if not root.exists():
            root.mkdir(mode=0o700)
            (root / ".git").mkdir(mode=0o700)
            self.clones += 1
        value["repository"] = "existing_preserved" if value["plan"]["repository_existing"] else "ready"
        value["templates_done"] = value["tools_done"] = True
        files.save(journal, value)
        if self.lose_apply:
            self.lose_apply = False
            raise files.Refused("synthetic accepted apply reply lost")

    def subscription_preview(self, receipt, output):
        self.calls.append("subscription_preview")
        return {"company": self.source["company"], "bootstrap": self.bootstrap_for(receipt), "bootstrap_sha256": "c" * 64,
                "output": output, "contains_database_credentials": False, "creates_services": False, "downloads_knowledge": False}

    def bootstrap_for(self, receipt):
        result = copy.deepcopy(self.bootstrap)
        manifest = receipt["manifest"]
        result["catalog_revision"] = manifest["revision"]
        result["scope"].update(area=manifest["entry"]["area"], owner=manifest["entry"]["owner_instance_id"], project=manifest["project_id"])
        return result

    def subscription_confirm(self, receipt, output, digest):
        self.calls.append(("subscription_confirm", str(output), digest))
        if digest != "c" * 64:
            raise files.Refused("synthetic bootstrap digest mismatch")
        if not Path(output).exists():
            files.missing(output, files.encoded({"schema_version": 1, "company": self.source["company"],
                          "subscription": str(uuid.uuid4()), "bootstrap": self.bootstrap_for(receipt),
                          "allow_loopback_development": self.source["allow_loopback_development"]}))
        if self.lose_subscription:
            self.lose_subscription = False
            raise files.Refused("synthetic accepted subscription reply lost")


class JourneyContract(unittest.TestCase):
    def setUp(self):
        # Reuse the existing protected prerequisite filesystem/fake native
        # fixture, not its tests. All subprocess effects remain synthetic.
        self.fixture = support.OperatorContract()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        helper = Path(journey.operator.__file__).parent / "onboarding_journey.py"
        helper.write_bytes((MACOS / "onboarding_journey.py").read_bytes())
        helper.chmod(0o400)
        self.patch = patch.object(journey, "__file__", str(helper))
        self.patch.start()
        self.addCleanup(self.patch.stop)
        native = copy.deepcopy(self.fixture.input)
        native.pop("subscriptions")
        self.source = {"schema_version": 1, "company": self.fixture.metadata["company"], "name": "Fake Mac",
                       "no_browser": False, "allow_loopback_development": False, "native": native,
                       "projects": [{"workspace": str(self.root / "repo"), "journal": str(self.root / "workspace.json"),
                                     "subscription_output": str(self.root / "new-subscription.json")}]}
        self.input, self.journal = self.root / "journey-input.json", self.root / "journey.json"
        self.fixture.write(self.input, self.source)
        self.memory = FakeMemory(self.fixture, self.source)
        self.native = self.fixture.fake
        self.prompts = []

    def prompt(self, kind, digest, preview):
        self.prompts.append((kind, digest, copy.deepcopy(preview)))

    def run_journey(self, prompt=None):
        with patch("sys.stdout", new=io.StringIO()) as output:
            result = journey.run(self.input, self.journal, self.native, memory=self.memory, prompt=prompt or self.prompt)
        return result, output.getvalue()

    def test_unified_journey_three_confirmations_status_no_device_secrets(self):
        result, output = self.run_journey()
        self.assertEqual([p[0] for p in self.prompts], ["repository/tools:0", "offline disclosure:0", "native services"])
        self.assertEqual(self.prompts[0][1], "a" * 64)
        self.assertEqual(self.prompts[1][1], "c" * 64)
        self.assertEqual(self.memory.clones, 1)
        self.assertEqual(result["agent_configuration"], "not started; run journey-agents")
        self.assertEqual(result["native_acceptance"], "unverified")
        for text in [output, self.journal.read_text()]:
            for forbidden in ["local_database_url", "poll_secret", "mlt_", '"password"', '"api_token"']:
                self.assertNotIn(forbidden, text)
        self.assertEqual(len(self.native.bootstraps), 3)
        self.assertFalse(any("restart" in a or "stop" in a for args in self.native.commands for a in args))

    def test_exact_resume_preserves_ids_subscriptions_and_no_duplicate_clone_or_jobs(self):
        self.run_journey()
        original = files.read(self.journal)
        subscription = files.read(self.source["projects"][0]["subscription_output"])
        self.prompts.clear()
        self.run_journey()
        self.assertEqual(files.read(self.journal)["request_id"], original["request_id"])
        self.assertEqual(files.read(self.journal)["native"], original["native"])
        self.assertEqual(files.read(self.source["projects"][0]["subscription_output"]), subscription)
        self.assertEqual(self.prompts, [])
        self.assertEqual(self.memory.calls.count("login"), 1)
        self.assertEqual(self.memory.calls.count("select"), 1)
        self.assertEqual(self.memory.clones, 1)
        self.assertEqual(len(self.native.bootstraps), 3)
        self.assertEqual(len([c for c in self.memory.calls if isinstance(c, tuple) and c[0] == "apply"]), 2)

    def test_cancel_selection_or_repository_confirmation_creates_no_repo_native_effects(self):
        self.memory.cancel_select = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertFalse(Path(self.source["projects"][0]["workspace"]).exists())
        self.assertEqual(self.native.commands, [])
        self.memory.cancel_select = False
        def cancel(*_):
            raise files.Refused("synthetic confirmation cancelled")
        with self.assertRaises(files.Refused):
            self.run_journey(cancel)
        self.assertEqual(self.native.commands, [])
        self.assertFalse(Path(self.source["projects"][0]["workspace"]).exists())

    def test_lost_selection_reply_resumes_same_journal_without_reselection(self):
        self.memory.lose_select = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        original = files.read(self.source["projects"][0]["journal"])["request_id"]
        self.run_journey()
        self.assertEqual(self.memory.calls.count("select"), 1)
        self.assertEqual(files.read(self.journal)["projects"][0]["selection"]["request_id"], original)

    def test_lost_apply_and_subscription_replies_retry_exact_inputs_without_duplicate_effects(self):
        self.memory.lose_apply = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.memory.lose_subscription = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        subscription = files.read(self.source["projects"][0]["subscription_output"])
        self.run_journey()
        self.assertEqual(self.memory.clones, 1)
        self.assertEqual(files.read(self.source["projects"][0]["subscription_output"]), subscription)
        applies = [c[2] for c in self.memory.calls if isinstance(c, tuple) and c[0] == "apply"]
        self.assertEqual(set(applies), {"a" * 64})
        subscriptions = [c[2] for c in self.memory.calls if isinstance(c, tuple) and c[0] == "subscription_confirm"]
        self.assertEqual(subscriptions, ["c" * 64, "c" * 64])

    def test_changed_pending_bootstrap_never_refreshes_or_resubmits_selection(self):
        self.memory.lose_subscription = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        before = files.read(self.source["projects"][0]["subscription_output"])
        self.memory.bootstrap["authority"] = str(uuid.uuid4())
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(files.read(self.source["projects"][0]["subscription_output"]), before)
        self.assertEqual(len([c for c in self.memory.calls if isinstance(c, tuple) and c[0] == "subscription_confirm"]), 1)
        self.assertEqual(self.native.commands, [])

    def test_changed_manifest_device_inputs_or_prerequisites_block_resume(self):
        def cancel(*_):
            raise files.Refused("synthetic cancellation")
        with self.assertRaises(files.Refused):
            self.run_journey(cancel)
        workspace = files.read(self.source["projects"][0]["journal"])
        for field in ["owner", "device"]:
            changed = copy.deepcopy(workspace)
            if field == "owner":
                changed["plan"]["manifest"]["entry"]["owner_instance_id"] = str(uuid.uuid4())
            else:
                changed["plan"]["device_id"] = str(uuid.uuid4())
            self.fixture.write(Path(self.source["projects"][0]["journal"]), changed)
            with self.assertRaises(files.Refused):
                self.run_journey()
        self.assertEqual(self.memory.clones, 0)
        self.source["name"] = "different input"
        self.fixture.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.native.commands, [])

    def test_same_project_uuid_and_name_across_owners_remain_distinct_subscriptions(self):
        self.source["projects"].append({"workspace": str(self.root / "repo-two"), "journal": str(self.root / "workspace-two.json"), "subscription_output": str(self.root / "subscription-two.json")})
        self.fixture.write(self.input, self.source)
        select = self.memory.select
        def owner_selection(slot):
            if slot == self.source["projects"][1]:
                self.memory.manifest["entry"]["owner_instance_id"] = str(uuid.uuid4())
            select(slot)
        with patch.object(self.memory, "select", side_effect=owner_selection):
            self.run_journey()
        subscriptions = [files.read(s["subscription_output"]) for s in self.source["projects"]]
        self.assertEqual(subscriptions[0]["bootstrap"]["scope"]["project"], subscriptions[1]["bootstrap"]["scope"]["project"])
        self.assertNotEqual(subscriptions[0]["bootstrap"]["scope"]["owner"], subscriptions[1]["bootstrap"]["scope"]["owner"])
        self.assertNotEqual(subscriptions[0]["subscription"], subscriptions[1]["subscription"])

    def test_changed_installed_binary_pin_prevents_all_resume_commands(self):
        def cancel(*_):
            raise files.Refused("synthetic cancellation")
        with self.assertRaises(files.Refused):
            self.run_journey(cancel)
        count = len(self.memory.calls)
        self.fixture.memory.chmod(0o700)  # Only this test-owned immutable fixture.
        self.fixture.memory.write_bytes(b"synthetic replaced fixture, never executed")
        self.fixture.memory.chmod(0o500)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(len(self.memory.calls), count)
        self.assertEqual(self.native.commands, [])

    def test_native_confirmation_cancel_and_lost_init_resume_reuse_full_plan(self):
        def cancel_native(kind, digest, preview):
            if kind == "native services":
                raise files.Refused("synthetic native cancellation")
            self.prompt(kind, digest, preview)
        with self.assertRaises(files.Refused):
            self.run_journey(cancel_native)
        native_receipt = files.read(self.journal)["native"]
        self.assertFalse(Path(self.source["native"]["root"]).exists())
        self.native.lose_init = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.run_journey()
        self.assertEqual(files.read(self.journal)["native"], native_receipt)
        self.assertEqual(len([c for c in self.native.commands if Path(c[0]).name == "initdb"]), 1)

    def test_current_revocation_or_unsupported_tool_blocks_before_subscriptions_native(self):
        self.memory.revoked = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.native.commands, [])
        self.assertFalse(Path(self.source["projects"][0]["subscription_output"]).exists())
        self.assertEqual(self.memory.clones, 0)

    def test_trust_and_uninstalled_prerequisites_fail_before_login(self):
        self.source["native"]["areas"][0]["name"] = "trust"
        self.fixture.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.memory.calls, [])
        self.source["native"]["areas"][0]["name"] = "workshop"
        self.fixture.write(self.input, self.source)
        self.native.mismatch = True
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.memory.calls, [])
        self.assertFalse(self.journal.exists())

    def test_existing_settings_preserved_and_metadata_never_placed_inside_repo(self):
        root = Path(self.source["projects"][0]["workspace"])
        root.mkdir(mode=0o700)
        (root / ".git").mkdir(mode=0o700)
        (root / ".codex").mkdir(mode=0o700)
        custom = root / ".codex/config.toml"
        custom.write_text("custom = true\n")
        custom.chmod(0o400)
        self.run_journey()
        self.assertEqual(custom.read_text(), "custom = true\n")
        self.assertEqual(custom.stat().st_mode & 0o777, 0o400)
        self.assertEqual(self.memory.clones, 0)

    def test_memory_boundary_fixed_arguments_suppressed_batch_output_and_cli_digest(self):
        memory = journey.Memory(self.source)
        captured = []
        def execute(args, **kwargs):
            captured.append((args, kwargs))
            return subprocess.CompletedProcess(args, 0, stdout=b"SENTINEL_SECRET", stderr=b"SENTINEL_SECRET")
        with patch.object(subprocess, "run", side_effect=execute), patch("sys.stdout", new=io.StringIO()) as output:
            memory.apply("/private/exact.json", "a" * 64)
            memory.subscription_confirm({"manifest": self.memory.manifest}, "/private/sub.json", "c" * 64)
        self.assertEqual(output.getvalue(), "")
        self.assertEqual(captured[0][1]["stdout"], subprocess.DEVNULL)
        self.assertEqual(captured[0][1]["stderr"], subprocess.DEVNULL)
        self.assertNotIn("shell", captured[0][1])
        self.assertEqual(captured[1][0][-3:], ["--bootstrap-sha256", "c" * 64, "--confirm"])
        self.assertNotIn("Authorization", json.dumps(captured, default=str))

    def select_v2(self):
        manifest = self.memory.manifest
        self.source["schema_version"] = 2
        self.source["projects"][0]["selection"] = {
            "area": manifest["entry"]["area"], "owner_instance_id": manifest["entry"]["owner_instance_id"],
            "project_id": manifest["project_id"], "revision": manifest["revision"], "manifest_digest": files.digest(manifest)}
        self.fixture.write(self.input, self.source)

    def test_v2_selected_composite_project_replays_and_v1_stays_readable(self):
        self.select_v2()
        self.run_journey()
        original = files.read(self.journal)
        self.run_journey()
        self.assertEqual(files.read(self.journal), original)
        self.assertEqual(self.memory.clones, 1)
        self.assertEqual(journey.status(self.journal)["native_acceptance"], "unverified")

    def test_v2_selection_mismatch_blocks_all_workspace_and_native_effects(self):
        self.select_v2()
        for field, changed in [("project", "changed same revision"), ("owner_instance_id", str(uuid.uuid4())), ("project_id", str(uuid.uuid4())),
                               ("revision", 2), ("area", "homelab")]:
            target = self.memory.manifest if field in {"revision", "project_id", "project"} else self.memory.manifest["entry"]
            original = target[field]
            target[field] = changed
            with self.assertRaises(files.Refused):
                self.run_journey()
            target[field] = original
            workspace = Path(self.source["projects"][0]["journal"])
            if workspace.exists():
                workspace.unlink()  # Test-owned synthetic receipt only.
        self.assertEqual(self.memory.clones, 0)
        self.assertEqual(self.native.commands, [])

    def test_v2_fixed_prepare_arguments_and_strict_schema(self):
        self.select_v2()
        memory = journey.Memory(self.source)
        with patch.object(memory, "execute") as execute:
            memory.select(self.source["projects"][0])
        args = execute.call_args.args[0]
        self.assertEqual(args[:2], ["projects", "prepare"])
        self.assertIn(self.memory.manifest["project_id"], args)
        self.assertIn(self.memory.manifest["entry"]["owner_instance_id"], args)
        for version in [True, 3, 1]:
            self.source["schema_version"] = version
            with self.assertRaises(files.Refused):
                journey.plan(self.source, self.journal)
        self.source["schema_version"] = 2
        self.source["projects"][0]["selection"]["area"] = "trust"
        with self.assertRaises(files.Refused):
            journey.plan(self.source, self.journal)

    def test_missing_private_parent_and_alias_outputs_fail_without_creating_parents(self):
        absent = self.root / "not-created"
        self.source["projects"][0]["journal"] = str(absent / "plan.json")
        self.fixture.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertFalse(absent.exists())
        self.assertEqual(self.memory.calls, [])
        self.source["projects"][0]["journal"] = str(self.root / "SCOPE.json")
        self.source["projects"][0]["subscription_output"] = str(self.root / "scope.json")
        self.fixture.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.memory.calls, [])

    def test_duplicate_composite_selection_blocks_all_workspace_effects(self):
        self.source["projects"].append({"workspace": str(self.root / "repo-two"), "journal": str(self.root / "workspace-two.json"), "subscription_output": str(self.root / "subscription-two.json")})
        self.fixture.write(self.input, self.source)
        with self.assertRaises(files.Refused):
            self.run_journey()
        self.assertEqual(self.memory.calls.count("select"), 2)
        self.assertEqual(self.memory.clones, 0)
        self.assertEqual(self.native.commands, [])

    def test_fake_success_without_completed_workspace_does_not_mark_stage_done(self):
        with patch.object(self.memory, "apply", return_value=None):
            with self.assertRaises(files.Refused):
                self.run_journey()
        self.assertEqual(files.read(self.journal)["stages"]["repository/tools:0"], "intent")
        self.assertEqual(self.native.commands, [])

    def test_ctrl_c_main_is_sanitized_and_retains_journal(self):
        with patch.object(sys, "argv", ["onboard.py", "journey", "--input", str(self.input), "--journal", str(self.journal)]), patch.object(journey, "run", side_effect=KeyboardInterrupt), patch("sys.stderr", new=io.StringIO()) as output:
            with self.assertRaises(SystemExit) as exit_result:
                journey.operator.main()
        self.assertEqual(exit_result.exception.code, 130)
        self.assertIn("resume exact inputs", output.getvalue())
        self.assertNotIn("Traceback", output.getvalue())


if __name__ == "__main__":
    unittest.main()
