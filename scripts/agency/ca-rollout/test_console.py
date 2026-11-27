import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch


def load(name):
    path = Path(__file__).with_name(name + ".py")
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


console = load("console")
journal = load("installer-journal")


class PortableSettingsTest(unittest.TestCase):
    def setUp(self):
        self.settings = {
            "unit": "codifix-companion.service", "worker_binary": "/opt/example/worker",
            "worker_config": "/etc/example/worker.json",
            "instance_id": "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa",
            "publisher_peer": "publisher", "publisher_instance_id": "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb",
            "publisher_did": "did:web:publisher.example.test",
            "mandate_id": "cccccccc-cccc-4ccc-cccc-cccccccccccc", "specialist_id": "installer",
            "companion_health": "http://127.0.0.1:8787/health",
            "installed_tui": "/opt/example/tui", "launcher": "/opt/example/launch-tui",
            "operator": "terminal-user", "rclone_config": "/etc/example/reader.conf",
            "object_origin": "https://objects.example.test:8443/",
        }

    def test_requires_exact_tui_settings_and_approved_scope(self):
        self.assertEqual(console.validate_settings(self.settings, "tui"), self.settings)
        for change in (
            {"publisher_peer": "peer'injected"}, {"unit": "--user.service"},
            {"mandate_id": "00000000-0000-0000-0000-000000000000"},
            {"object_origin": "http://objects.example.test/"},
            {"object_origin": "https://objects.example.test/private"},
            {"companion_health": "http://127.0.0.1.example.test:8787/health"},
            {"install_command": "/unreviewed"},
        ):
            with self.assertRaises((console.Stop, ValueError)):
                console.validate_settings(self.settings | change, "tui")
        without_origin = dict(self.settings)
        del without_origin["object_origin"]
        with self.assertRaises(console.Stop):
            console.validate_settings(without_origin, "tui")

    def test_companion_bundle_checks_all_digests(self):
        directory = Path("/opt/example/candidate")
        binary, smoke, upgrade = b"worker", b"smoke evidence", b"upgrade evidence"
        derive = lambda value: hashlib.sha256(value).hexdigest()
        receipt = {"version": "3.1.0", "url": "https://example.test/worker", "sha256": derive(binary),
                   "artifact_test_sha256": derive(smoke), "upgrade_test_sha256": derive(upgrade)}
        files = {directory / "tested.json": json.dumps(receipt).encode(), directory / "codifix-agency": binary,
                 directory / (derive(smoke) + ".log"): smoke, directory / (derive(upgrade) + ".log"): upgrade}
        with patch.object(console, "secure_path", return_value=directory), patch.object(
            console, "read_protected", side_effect=lambda path, *args: files[path],
        ):
            self.assertEqual(console.load_release(directory), (receipt, binary))
            files[directory / (derive(smoke) + ".log")] = b"wrong smoke evidence"
            with self.assertRaises(console.Stop):
                console.load_release(directory)

    def test_journal_never_exposes_message_body(self):
        entries = [
            {"MESSAGE": "Failed with result: synthetic private secret", "__REALTIME_TIMESTAMP": "123"},
            {"MESSAGE": "raw synthetic private value", "__REALTIME_TIMESTAMP": "malformed"},
        ]
        labels = list(journal.label_journal("\n".join(map(json.dumps, entries))))
        self.assertEqual(labels, [{"time": "123", "message": "Failed with result"},
                                  {"time": None, "message": "other service message"}])
