import hashlib
import importlib.util
import json
from pathlib import Path
import ssl
import tempfile
import unittest
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("launcher", ROOT / "scripts/agency/linear-tui.py")
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class Enrollment(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.state = Path(folder.name)
        self.roles = {r: {"instance": str(uuid4()), "mandate": str(uuid4())} for r in launcher.ROLES}
        self.accounts = {r: {"did": "did:web:" + r + ".example.com", "signing_key": "test-" + r} for r in launcher.ROLES}
        self.input = self.state / "enrolled-peers.json"
        self.row = {
            "instance_id": str(uuid4()),
            "did": "did:web:enrolled.example.com",
            "signing_key": "did:key:zQ3" + "A" * 46,
            "endpoint": "https://127.0.0.1:10451",
            "tls_ca_sha256": "0" * 64,
        }

    def load(self):
        return launcher.production_enrollment(self.state, self.roles, self.accounts)

    def save(self):
        launcher.write(self.input, {"version": 1, "production-homelab": self.row})

    def test_optional_owner_only_no_symlinks(self):
        self.assertIsNone(self.load())
        self.save()
        self.input.chmod(0o640)
        with self.assertRaises(launcher.SetupError):
            self.load()
        self.input.rename(self.state / "target")
        self.input.symlink_to(self.state / "target")
        with self.assertRaises(OSError):
            self.load()

    def test_rejects_broader_endpoint_or_colliding_identity(self):
        original = self.row.copy()
        for field, value in (("endpoint", "https://localhost:8451"), ("did", self.accounts["homelab"]["did"]),
                             ("instance_id", self.roles["linear"]["instance"]), ("signing_key", "unreviewed")):
            with self.subTest(field=field):
                self.row = dict(original, **{field: value})
                self.save()
                with self.assertRaises(launcher.SetupError):
                    self.load()

    def test_grant_is_only_linear_project_and_reciprocal_is_public(self):
        root, _, _ = launcher.tls_pair(self.state)
        self.assertTrue(root.is_file())
        ports = {"homelab": 10000, "linear": 10001}
        urls = {k: "https://127.0.0.1:" + str(v + 100) for k, v in ports.items()}
        scope = {"endpoint": launcher.LINEAR, "project_id": str(uuid4()), "team_id": str(uuid4())}
        launcher.configure(self.state, self.roles, self.accounts, "http://127.0.0.1:10002", ports, urls, scope,
                           (self.row, root.read_text()))
        linear = launcher.read(self.state / "linear.json")
        self.assertEqual([p["id"] for p in linear["peers"]], ["operator", "homelab", "production-homelab"])
        remote = linear["peers"][-1]
        self.assertEqual(remote, {"id": "production-homelab", "instance_id": self.row["instance_id"],
                                  "endpoint": self.row["endpoint"], "mandates": [self.roles["linear"]["mandate"]]})
        self.assertNotIn("production-homelab", launcher.read(self.state / "homelab.json")["social"]["peers"])
        reciprocal = launcher.read(self.state / "linear-reciprocal.json")
        self.assertEqual(reciprocal["incident_linear"]["project_id"], scope["project_id"])
        self.assertEqual(reciprocal["peer"]["mandates"], [])
        self.assertEqual(set(reciprocal), {"peer", "social_peer", "tls_ca_sha256", "incident_linear"})
        self.assertEqual(set(reciprocal["social_peer"]), {"did", "signing_key"})

    def test_requires_pinned_public_certificate(self):
        root, _, _ = launcher.tls_pair(self.state)
        pem = root.read_text()
        self.row["tls_ca_sha256"] = hashlib.sha256(ssl.PEM_cert_to_DER_cert(pem)).hexdigest()
        self.save()
        launcher.save(self.state / "production-homelab-ca.pem", pem)
        self.assertEqual(self.load()[0], self.row)
        self.row["tls_ca_sha256"] = "0" * 64
        self.save()
        with self.assertRaises(launcher.SetupError):
            self.load()


if __name__ == "__main__":
    unittest.main()
