"""Validate the resident launcher wiring without touching a provider or live state."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("linear_launcher", ROOT / "scripts/agency/linear-tui.py")
LAUNCHER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LAUNCHER)


class ProjectRoutes(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.state = Path(folder.name)
        self.roles = {
            role: {field: str(uuid4()) for field in ("project", "instance", "mandate", "operator")}
            for role in LAUNCHER.ROLES
        }
        self.legacy = {"endpoint": "http://127.0.0.1:9000/graphql",
                       "project_id": str(uuid4()), "team_id": str(uuid4())}
        self.scope = {field: str(uuid4()) for field in ("project_id", "mandate_id", "in_progress_state_id", "done_state_id")}
        self.scope |= {"team_id": self.legacy["team_id"], "project_name": "Codifix"}
        self.path = self.state / "linear-projects.json"
        self.response = {"data": {
            "project": {"id": self.scope["project_id"], "name": "Codifix", "archivedAt": None,
                        "teams": {"nodes": [{"id": self.scope["team_id"]}]}},
            "started": {"id": self.scope["in_progress_state_id"], "type": "started",
                        "team": {"id": self.scope["team_id"]}},
            "done": {"id": self.scope["done_state_id"], "type": "completed",
                     "team": {"id": self.scope["team_id"]}},
        }}

    def load(self):
        return LAUNCHER.project_scopes(self.state, self.legacy["endpoint"], "fixture-key", self.legacy, self.roles)

    def write_scope(self):
        LAUNCHER.write(self.path, {"version": 1, "projects": {"codifix": self.scope}})

    def test_legacy_config_needs_no_new_file(self):
        with patch.object(LAUNCHER, "request") as provider:
            self.assertEqual(self.load(), {})
        provider.assert_not_called()
        routes = LAUNCHER.write_project_routes(self.state, self.roles, {}, {})
        self.assertEqual(LAUNCHER.read(routes), {"schema_version": 1, "projects": {}})
        self.assertFalse((self.state / "project-operator.token").exists())

    def test_adds_private_route_without_changing_homelab_grant(self):
        self.write_scope()
        with patch.object(LAUNCHER, "request", return_value=self.response) as provider:
            projects = self.load()
        self.assertEqual(provider.call_args.kwargs["body"]["variables"], {
            "project": self.scope["project_id"], "started": self.scope["in_progress_state_id"],
            "done": self.scope["done_state_id"],
        })
        accounts = {
            role: {"did": f"did:web:{role}.example", "signing_key": f"{role}-key"} for role in LAUNCHER.ROLES
        }
        ports = {"homelab": 1234, "linear": 1235}
        ingress = {"homelab": "https://127.0.0.1:4321", "linear": "https://127.0.0.1:4322"}
        external = {"instance_id": str(uuid4()), "endpoint": "https://127.0.0.1:8451",
                    "did": "did:web:production.example", "signing_key": "production-key"}
        (self.state / "peer-root.pem").write_text("fixture")
        with patch.object(LAUNCHER.ssl, "PEM_cert_to_DER_cert", return_value=b"fixture"):
            LAUNCHER.configure(self.state, self.roles, accounts, "https://pds.example", ports, ingress,
                               self.legacy, (external, "fixture"), projects)
        runtime = LAUNCHER.read(self.state / "linear.json")
        legacy_grant = self.roles["linear"]["mandate"]
        new_grant = self.scope["mandate_id"]
        peers = {peer["id"]: peer for peer in runtime["peers"]}
        self.assertEqual(peers["homelab"]["mandates"], [legacy_grant, new_grant])
        self.assertEqual(peers["production-homelab"]["mandates"], [legacy_grant])
        self.assertEqual(runtime["linear"]["tracking"][legacy_grant], {
            "project_id": self.legacy["project_id"], "team_id": self.legacy["team_id"],
        })
        self.assertEqual(runtime["linear"]["tracking"][new_grant]["automatic"], {
            "requester": "homelab", "in_progress_state_id": self.scope["in_progress_state_id"],
            "done_state_id": self.scope["done_state_id"],
        })
        routes = LAUNCHER.write_project_routes(self.state, self.roles, ports, projects)
        data = json.loads(routes.read_bytes())
        self.assertEqual(data["schema_version"], 1)
        route = data["projects"]["codifix"]
        self.assertEqual(route["remote_mandate_id"], new_grant)
        self.assertEqual(route["local_mandate_id"], self.roles["homelab"]["mandate"])
        self.assertEqual(route["linear_project_id"], self.scope["project_id"])
        self.assertEqual(route["endpoint"], "http://127.0.0.1:1234")
        token = Path(route["token_file"])
        self.assertEqual(token.read_text().strip(), self.roles["homelab"]["operator"])
        self.assertFalse(routes.stat().st_mode & 0o077)
        self.assertFalse(token.stat().st_mode & 0o077)

    def test_rejects_wrong_workflow_team_or_shared_mandate(self):
        self.write_scope()
        self.response["data"]["done"]["team"]["id"] = str(uuid4())
        with patch.object(LAUNCHER, "request", return_value=self.response):
            with self.assertRaisesRegex(LAUNCHER.SetupError, "outside the configured team"):
                self.load()
        self.scope["mandate_id"] = self.roles["linear"]["mandate"]
        self.write_scope()
        with patch.object(LAUNCHER, "request") as provider:
            with self.assertRaisesRegex(LAUNCHER.SetupError, "existing Linear project or mandate"):
                self.load()
        provider.assert_not_called()

    def test_does_not_rebind_a_persisted_mandate(self):
        resource = {"kind": "linear_project", "id": self.scope["project_id"]}
        grant = LAUNCHER.grant_document("linear", self.roles["linear"], self.scope["mandate_id"], resource)
        old = json.loads(json.dumps(grant))
        old["mandate"]["resources"][0]["id"] = self.legacy["project_id"]
        with patch.object(LAUNCHER, "pg", return_value=json.dumps(old)):
            with self.assertRaisesRegex(LAUNCHER.SetupError, "conflicts with its configured project"):
                LAUNCHER.install_grant(None, None, None, None, None, None, grant, check_existing=True)


if __name__ == "__main__":
    unittest.main()
