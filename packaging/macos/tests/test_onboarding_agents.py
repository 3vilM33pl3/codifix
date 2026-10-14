#!/usr/bin/env python3
"""Synthetic commands only: no native Keychain, Mac, provider or model calls."""
import copy
import importlib
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import uuid

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
agents = importlib.import_module("onboarding_agents")
files = importlib.import_module("onboarding_files")
import test_onboarding_journey as support


class FakeAgents:
    def __init__(self, fixture):
        self.fixture, self.source = fixture, fixture.source
        self.prepared, self.receipts, self.configured, self.plans, self.calls = {}, {}, {}, {}, []
        self.indices = {}
        self.unusable = set()
        self.denied = self.lost_status = self.lost_apply = self.changed_settings = self.altered_scope = False

    def prepare_agent(self, slot, caller, *, new_request=False):
        self.calls.append(("prepare", caller))
        key = (slot["journal"], caller)
        if key in self.indices and not new_request:
            if self.indices[key] in self.unusable:
                raise files.Refused("synthetic expired/denied request requires explicit replacement")
            return copy.deepcopy(self.prepared[self.indices[key]])
        selected = self.fixture.memory.workspace_status(slot["journal"])["plan"]
        m = selected["manifest"]
        r = {"request_id": str(uuid.uuid4()), "agent_id": str(uuid.uuid4()), "principal_id": str(uuid.uuid4()), "token_id": str(uuid.uuid4()),
             "caller": caller, "provider_id": agents.CALLERS[caller], "credential_sha256": "a" * 64,
             "expires_at": "2026-10-06T10:00:00Z", "confirmed": False,
             "projects": [{"area": m["entry"]["area"], "owner_instance_id": m["entry"]["owner_instance_id"], "project_id": m["project_id"],
                           "manifest_revision": m["revision"], "manifest_sha256": "b" * 64, "offline_confirmed": True}]}
        value = {"device_id": selected["device_id"], "approval_endpoint": f"/v1/onboarding/devices/{selected['device_id']}/agents",
                 "request": r, "agent_id": r["agent_id"], "contains_secret": False, "automatic_approval": False}
        self.prepared[r["agent_id"]] = value
        self.indices[key] = r["agent_id"]
        receipt = {k: v for k, v in r.items() if k != "confirmed"}
        receipt.update(schema_version=1, device_id=selected["device_id"], human_id=str(uuid.uuid4()),
                       company_id=self.fixture.memory.bootstrap["scope"]["company"], issuer="https://idp.example.invalid", subject="stable-subject",
                       approved_at="2026-10-05T10:00:00Z", revoked_at=None,
                       projects=[{"selection": r["projects"][0], "destination": {"name": caller, "provider_id": agents.CALLERS[caller], "destination_id": agents.CALLERS[caller]}}])
        self.receipts[r["agent_id"]] = receipt
        return value

    def agent_status(self, agent_id):
        self.calls.append(("status", agent_id))
        if self.denied:
            raise files.Refused("synthetic current authority denied")
        if self.lost_status:
            self.lost_status = False
            raise files.Refused("synthetic status response lost")
        return copy.deepcopy(self.receipts[agent_id])

    def config_preview(self, slot, agent_id, profile, allow_offline):
        self.calls.append(("preview", agent_id))
        request = self.prepared[agent_id]
        caller = request["request"]["caller"]
        subscription = files.read(slot["subscription_output"])
        args = ["projects", "agents", "mcp", "--company", self.source["company"], "--agent-id", agent_id, "--profile", str(profile)]
        if self.source["allow_loopback_development"]:
            args.append("--allow-loopback")
        if allow_offline:
            args.append("--allow-offline")
        after = ("d" if caller == "codex" else "e") * 64
        value = {"schema_version": 1, "agent_id": agent_id, "caller": caller, "provider_id": agents.CALLERS[caller], "device_id": request["device_id"],
                 "scope": subscription["bootstrap"]["scope"], "workspace": slot["workspace"],
                 "settings_path": str(Path(slot["workspace"]) / (".codex/config.toml" if caller == "codex" else ".mcp.json")),
                 "before_sha256": after if self.configured.get(agent_id) else None, "after_sha256": after,
                 "command": self.source["native"]["memory"]["path"], "args": args, "allow_offline": allow_offline,
                 "preserves_other_settings": True, "contains_credentials": False, "already_configured": bool(self.configured.get(agent_id)), "committed": False, "replayed": False}
        if self.changed_settings:
            value["after_sha256"] = "f" * 64
        if self.altered_scope:
            value["args"][value["args"].index("--profile") + 1] = "/unreviewed/profile.json"
        value["plan_sha256"] = files.digest(value)
        if not self.configured.get(agent_id):
            self.plans[agent_id] = value["plan_sha256"]
        return value

    def config_apply(self, slot, agent_id, profile, allow_offline, digest):
        self.calls.append(("apply", agent_id, digest))
        if self.plans[agent_id] != digest:
            raise files.Refused("synthetic exact settings digest required")
        self.configured[agent_id] = True
        if self.lost_apply:
            self.lost_apply = False
            raise files.Refused("synthetic accepted settings reply lost")


class AgentJourneyContract(unittest.TestCase):
    def setUp(self):
        self.fixture = support.JourneyContract()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        helper = Path(agents.journey.operator.__file__).parent / "onboarding_agents.py"
        helper.write_bytes((MACOS / "onboarding_agents.py").read_bytes())
        helper.chmod(0o400)
        location = patch.object(agents, "__file__", str(helper))
        location.start()
        self.addCleanup(location.stop)
        self.fixture.memory.manifest["entry"]["destinations"] = [{"name": c, "provider_id": p, "destination_id": p} for c, p in agents.CALLERS.items()]
        self.fixture.run_journey()
        self.memory = FakeAgents(self.fixture)
        self.prompts, self.browser = [], []

    def run_agents(self, allow_offline=False, renew_grant=None):
        with patch("sys.stdout", new=io.StringIO()) as output:
            result = agents.run(self.fixture.journal, self.fixture.native, allow_offline=allow_offline, renew_grant=renew_grant, memory=self.memory,
                                prompt=lambda *a: self.prompts.append(copy.deepcopy(a)), browser_prompt=lambda v: self.browser.append(copy.deepcopy(v)))
        return result, output.getvalue()

    def saved(self):
        return files.read(Path(str(self.fixture.journal) + ".agents.json"))

    def test_two_separate_confirmed_agents_have_only_public_metadata(self):
        result, output = self.run_agents(True)
        self.assertEqual(result["configured_agents"], 2)
        self.assertEqual(result["native_acceptance"], "unverified")
        self.assertFalse(result["paid_calls"])
        self.assertEqual([p[0] for p in self.prompts], ["codex settings:0", "claude settings:0"])
        self.assertEqual([b["request"]["provider_id"] for b in self.browser], ["openai", "anthropic"])
        saved = self.saved()
        self.assertNotEqual(saved["agents"][0]["approval"]["agent_id"], saved["agents"][1]["approval"]["agent_id"])
        for text in [output, str(saved)]:
            for forbidden in ["mlt_", "local_database_url", "poll_secret", "password", "api_token"]:
                self.assertNotIn(forbidden, text)

    def test_curated_disclosure_requires_the_same_mode_in_the_receipt(self):
        slot = self.fixture.source["projects"][0]
        selected = self.fixture.memory.workspace_status(slot["journal"])["plan"]
        prepared = self.memory.prepare_agent(slot, "codex")
        prepared["request"]["replication_mode"] = "curated_project_v1"
        agents.approval(prepared, selected, "codex")
        receipt = self.memory.agent_status(prepared["agent_id"])
        with self.assertRaises(files.Refused):
            agents.verified(receipt, prepared, receipt["company_id"])
        receipt["replication_mode"] = "curated_project_v1"
        agents.verified(receipt, prepared, receipt["company_id"])

    def test_exact_resume_retains_credentials_ids_and_configuration_intents(self):
        self.run_agents()
        original = self.saved()
        self.prompts.clear(); self.browser.clear()
        self.run_agents()
        self.assertEqual(self.saved(), original)
        self.assertEqual(self.prompts, [])
        self.assertEqual(self.browser, [])
        self.assertEqual(len([c for c in self.memory.calls if c[0] == "prepare"]), 2)
        self.assertEqual(self.fixture.memory.clones, 1)
        recorded = agents.journey.status(self.fixture.journal)["agent_configuration"]
        self.assertEqual(recorded["state"], "recorded_complete")
        self.assertEqual(recorded["configured_agents"], 2)
        self.assertEqual(recorded["current_authority"], "not asserted by local status")

    def test_lost_status_resumes_same_public_request_not_new_credentials(self):
        self.memory.lost_status = True
        with self.assertRaises(files.Refused): self.run_agents()
        identity = self.saved()["agents"][0]["approval"]["agent_id"]
        self.run_agents()
        self.assertEqual(self.saved()["agents"][0]["approval"]["agent_id"], identity)
        self.assertEqual(len([c for c in self.memory.calls if c[0] == "prepare"]), 2)

    def test_lost_settings_acknowledgement_resumes_original_confirmed_digest(self):
        self.memory.lost_apply = True
        with self.assertRaises(files.Refused): self.run_agents()
        original = self.saved()["agents"][0]["configuration"]["plan_sha256"]
        self.run_agents()
        applies = [c for c in self.memory.calls if c[0] == "apply"]
        self.assertEqual(applies[0][2], original)
        self.assertEqual(applies[1][2], original)
        self.assertEqual(len(self.prompts), 2)

    def test_current_denial_blocks_completed_configuration_replay(self):
        self.run_agents()
        count = len([c for c in self.memory.calls if c[0] == "apply"])
        self.memory.denied = True
        with self.assertRaises(files.Refused): self.run_agents()
        self.assertEqual(len([c for c in self.memory.calls if c[0] == "apply"]), count)

    def test_explicit_replacement_retains_history_needs_fresh_approval_and_replays_once(self):
        self.run_agents()
        old = self.saved()["agents"][0]
        self.memory.unusable.add(old["approval"]["agent_id"])
        self.prompts.clear(); self.browser.clear()
        self.run_agents(renew_grant=old["approval"]["agent_id"])
        renewed = self.saved()
        self.assertEqual(renewed["superseded"][0]["entry"], old)
        self.assertNotEqual(renewed["agents"][0]["approval"]["agent_id"], old["approval"]["agent_id"])
        self.assertEqual(self.prompts[0][0], "fresh agent grant")
        self.assertEqual(len(self.browser), 1)
        self.prompts.clear(); self.browser.clear()
        self.run_agents(renew_grant=old["approval"]["agent_id"])
        self.assertEqual(self.saved(), renewed)
        self.assertEqual(self.prompts, [])
        self.assertEqual(self.browser, [])

    def test_replacement_recovers_already_prepared_fresh_identity(self):
        self.run_agents()
        old = self.saved()["agents"][0]["approval"]["agent_id"]
        slot = self.fixture.source["projects"][0]
        fresh = self.memory.prepare_agent(slot, "codex", new_request=True)
        self.run_agents(renew_grant=old)
        self.assertEqual(self.saved()["agents"][0]["approval"]["agent_id"], fresh["agent_id"])
        self.assertEqual(len(self.saved()["superseded"]), 1)

    def test_changed_disclosure_choice_and_changed_settings_need_new_review(self):
        self.run_agents()
        calls = list(self.memory.calls)
        with self.assertRaises(files.Refused): self.run_agents(True)
        self.assertEqual(self.memory.calls, calls)
        self.memory.changed_settings = True
        with self.assertRaises(files.Refused): self.run_agents()

    def test_replayed_other_profile_or_duplicate_agent_stage_fails_closed(self):
        self.memory.altered_scope = True
        with self.assertRaises(files.Refused): self.run_agents()
        self.assertEqual(len([c for c in self.memory.calls if c[0] == "apply"]), 0)
        self.memory.altered_scope = False
        self.run_agents()
        saved = self.saved(); saved["agents"][1] = copy.deepcopy(saved["agents"][0])
        files.save(Path(str(self.fixture.journal) + ".agents.json"), saved)
        with self.assertRaises(files.Refused): self.run_agents()

    def test_operator_destination_review_is_required_even_with_device_membership(self):
        selected = agents.journey.inspect(self.fixture.journal)["projects"][0]["selection"]
        public = self.memory.prepare_agent(self.fixture.source["projects"][0], "codex")
        selected["manifest"]["entry"]["destinations"] = []
        with self.assertRaises(files.Refused): agents.approval(public, selected, "codex")
        public["request"]["credential"] = "SECRET_SENTINEL"
        with self.assertRaises(files.Refused): agents.approval(public, selected, "codex")

    def test_fixed_commands_use_public_ids_only_not_parent_or_provider_tokens(self):
        memory = agents.Memory(self.fixture.source)
        calls = []
        class Commands:
            def command(self, args):
                calls.append(args); return 0, "{}"
        memory.native = Commands()
        memory.prepare_agent(self.fixture.source["projects"][0], "claude")
        memory.agent_status(str(uuid.uuid4()))
        memory.config_preview(self.fixture.source["projects"][0], str(uuid.uuid4()), "/private/profile.json", True)
        self.assertIn("--open-browser", calls[0])
        self.assertIn("--caller", calls[0])
        self.assertIn("--allow-offline", calls[2])
        self.assertFalse(any("token" in str(a) or "password" in str(a) for args in calls for a in args))


class PublicConfirmationContract(unittest.TestCase):
    def test_short_confirmation_retains_exact_digest_and_cancellation(self):
        for answer in ["yes", "a" * 64]:
            with patch("builtins.input", return_value=answer), patch("sys.stdout", new=io.StringIO()) as output:
                agents.journey.confirm("reviewed stage", "a" * 64, {"public": True})
                self.assertIn("no paid model calls", output.getvalue())
        for options in [{"return_value": "no"}, {"side_effect": EOFError}]:
            with patch("builtins.input", **options), patch("sys.stdout", new=io.StringIO()), self.assertRaises(files.Refused):
                agents.journey.confirm("reviewed stage", "a" * 64, {"public": True})

    def test_browser_confirmation_eof_retains_request_without_approval(self):
        with patch("builtins.input", side_effect=EOFError), patch("sys.stdout", new=io.StringIO()), self.assertRaises(files.Refused):
            agents.browser_confirmation({"public_request": True})


if __name__ == "__main__":
    unittest.main()
