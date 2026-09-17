#!/usr/bin/env python3
"""Offline release tests. Any signing keys are disposable test fixtures only."""
import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("release_tool", Path(__file__).with_name("release.py"))
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


def creator(root, **overrides):
    values = dict(version="2.1.0-rc.1", source_revision="a" * 40, schema_floor=52, schema_head=52,
                  classification="maintenance", migrations=False, trust_changes=False, permission_changes=False,
                  pi_source_url=f"{TOOL.PI_SOURCE_REPOSITORY}/commit/{TOOL.PI_SOURCE_REVISION}",
                  pi_source_revision=TOOL.PI_SOURCE_REVISION,
                  pi_source_sha256=None, pi_artifact_sha256=[],
                  artifact=[f"amd64:binary:{root}/memory-amd64", f"arm64:binary:{root}/memory-arm64"],
                  output=str(root / "manifest.json"))
    values.update(overrides)
    return argparse.Namespace(**values)


class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="memory-release-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "memory-amd64").write_bytes(b"amd64 artifact fixture")
        (self.root / "memory-arm64").write_bytes(b"arm64 artifact fixture")
        TOOL.create(creator(self.root))
        self.manifest, self.data = TOOL.load_manifest(self.root / "manifest.json")

    def test_manifest_is_deterministic_independent_of_input_order(self):
        arguments = creator(self.root, output=str(self.root / "again.json"))
        arguments.artifact.reverse()
        result = TOOL.create(arguments)
        self.assertEqual((self.root / "again.json").read_bytes(), self.data)
        self.assertFalse(result["installation_authorized"])
        self.assertEqual(result["status"], "unsigned_manifest_created")

    def test_refuses_existing_output_and_symlink_inputs(self):
        with self.assertRaises(FileExistsError):
            TOOL.create(creator(self.root))
        (self.root / "alias").symlink_to(self.root / "memory-amd64")
        with self.assertRaises(OSError):
            TOOL.measure(self.root / "alias")
        (self.root / "alias-dir").symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            TOOL.measure(self.root / "alias-dir" / "memory-amd64")
        self.assertEqual((self.root / "manifest.json").read_bytes(), self.data)

    def test_unknown_fields_duplicate_keys_and_noncanonical_json_rejected(self):
        changed = copy.deepcopy(self.manifest)
        changed["install_command"] = "anything"
        with self.assertRaises(TOOL.ReleaseError):
            TOOL.validate(changed)
        path = self.root / "duplicate.json"
        path.write_bytes(self.data.replace(b'{"artifacts":', b'{"format":"other","artifacts":', 1))
        with self.assertRaises(TOOL.ReleaseError):
            TOOL.load_manifest(path)
        path.write_text(json.dumps(self.manifest, indent=2))
        with self.assertRaises(TOOL.ReleaseError):
            TOOL.load_manifest(path)

    def test_unsafe_names_wrong_arch_and_duplicate_artifacts_rejected(self):
        for field, value in (("name", "../escape"), ("name", "sub/file"), ("name", "manifest.json"),
                             ("name", "MANIFEST.JSON"), ("architecture", "riscv64"), ("architecture", []),
                             ("kind", {}), ("size_bytes", True), ("sha256", "bad")):
            with self.subTest(field=field, value=value):
                changed = copy.deepcopy(self.manifest)
                changed["artifacts"][0][field] = value
                with self.assertRaises(TOOL.ReleaseError):
                    TOOL.validate(changed)
        changed = copy.deepcopy(self.manifest)
        changed["artifacts"].append(copy.deepcopy(changed["artifacts"][0]))
        with self.assertRaises(TOOL.ReleaseError):
            TOOL.validate(changed)

    def test_maintenance_rejects_migrations_trust_and_permission_changes(self):
        for flag in ("migrations", "trust_changes", "permission_changes"):
            changed = copy.deepcopy(self.manifest)
            changed["upgrade"][flag] = True
            with self.assertRaises(TOOL.ReleaseError):
                TOOL.validate(changed)
        changed = copy.deepcopy(self.manifest)
        changed["schema"]["floor"] = 51
        with self.assertRaises(TOOL.ReleaseError):
            TOOL.validate(changed)
        changed["upgrade"].update(classification="approval-required", migrations=True)
        TOOL.validate(changed)

    def test_pi_pin_and_source_are_constrained(self):
        for field, value in (("version", "latest"), ("source_revision", "main"), ("source_revision", "b" * 40),
                             ("source_url", "https://user:secret@example.invalid/repo"),
                             ("source_url", "http://example.invalid/repo"),
                             ("source_url", f"https://github.com/other/pi_agent_rust/commit/{TOOL.PI_SOURCE_REVISION}"),
                             ("source_url", TOOL.PI_SOURCE_REPOSITORY + "/commit/" + "b" * 40),
                             ("artifact_sha256", {"arm64": "bad"})):
            changed = copy.deepcopy(self.manifest)
            changed["pi"][field] = value
            with self.assertRaises(TOOL.ReleaseError):
                TOOL.validate(changed)

        accepted = copy.deepcopy(self.manifest)
        accepted["pi"]["source_url"] = f"{TOOL.PI_SOURCE_REPOSITORY}/archive/{TOOL.PI_SOURCE_REVISION}.tar.gz"
        TOOL.validate(accepted)


@unittest.skipUnless(shutil.which("gpg") and shutil.which("gpgconf"), "GPG unavailable; crypto tests not run")
class SignatureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.keys = tempfile.TemporaryDirectory(prefix="memory-release-fixture-keyring-")
        cls.keyring = Path(cls.keys.name)
        os.chmod(cls.keyring, 0o700)
        cls.command(["--quick-generate-key", "Disposable Release Fixture <release-fixture@example.invalid>", "ed25519", "cert", "0"])
        listing = cls.command(["--with-colons", "--list-keys"]).decode()
        cls.primary = next(line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:"))
        cls.command(["--quick-add-key", cls.primary, "ed25519", "sign", "0"])
        listing = cls.command(["--with-colons", "--list-keys"]).decode()
        cls.signer = [line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:")][-1]
        cls.public = cls.command(["--armor", "--export", cls.primary])

    @classmethod
    def command(cls, arguments):
        result = subprocess.run(["gpg", "--no-options", "--homedir", str(cls.keyring), "--batch", "--no-tty",
                                 "--pinentry-mode", "loopback", "--passphrase", "", *arguments],
                                env={"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LC_ALL": "C", "GNUPGHOME": str(cls.keyring)},
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=30, check=True)
        return result.stdout

    @classmethod
    def tearDownClass(cls):
        subprocess.run(["gpgconf", "--homedir", str(cls.keyring), "--kill", "gpg-agent"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False, timeout=10)
        cls.keys.cleanup()

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="memory-release-signed-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "memory-amd64").write_bytes(b"amd64 fixture release")
        (self.root / "memory-arm64").write_bytes(b"arm64 fixture release")
        (self.root / "key.asc").write_bytes(self.public)
        (self.root / "staging").mkdir(mode=0o700)
        TOOL.create(creator(self.root))
        self.sign()

    def sign(self):
        signature = self.root / "manifest.sig"
        signature.unlink(missing_ok=True)
        self.command(["--local-user", self.signer + "!", "--digest-algo", "SHA256", "--output", str(signature),
                      "--detach-sign", str(self.root / "manifest.json")])

    def arguments(self, **changes):
        values = dict(manifest=str(self.root / "manifest.json"), signature=str(self.root / "manifest.sig"),
                      public_key=str(self.root / "key.asc"), trusted_signing_fingerprint=self.signer,
                      architecture="arm64", schema_current=52, artifacts_dir=str(self.root),
                      staging_parent=str(self.root / "staging"))
        values.update(changes)
        return argparse.Namespace(**values)

    def assert_rejected_without_stage(self, **changes):
        with self.assertRaises((TOOL.ReleaseError, OSError)):
            TOOL.verify_stage(self.arguments(**changes))
        self.assertEqual(list((self.root / "staging").iterdir()), [])

    def test_exact_signing_subkey_and_selected_architecture_are_staged(self):
        report = TOOL.verify_stage(self.arguments())
        stage = Path(report["staging_directory"])
        self.assertEqual(report["selected_artifacts"], ["memory-arm64"])
        self.assertEqual((stage / "memory-arm64").read_bytes(), b"arm64 fixture release")
        self.assertFalse((stage / "memory-amd64").exists())
        self.assertFalse(report["installation_authorized"])
        self.assertFalse(report["archives_unpacked"])
        self.assertFalse(report["unselected_architecture_bytes_verified"])
        self.assertEqual(stat.S_IMODE(stage.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((stage / "memory-arm64").stat().st_mode), 0o600)
        again = TOOL.verify_stage(self.arguments())
        self.assertNotEqual(again["staging_directory"], str(stage))
        self.assertEqual((stage / "memory-arm64").read_bytes(), b"arm64 fixture release")

    def test_primary_fingerprint_is_not_a_substitute_for_signing_subkey(self):
        self.assert_rejected_without_stage(trusted_signing_fingerprint=self.primary)
        self.assert_rejected_without_stage(trusted_signing_fingerprint="0" * 40)
        self.assert_rejected_without_stage(trusted_signing_fingerprint=self.signer[-16:])

    def test_wrong_architecture_and_schema_are_rejected(self):
        self.assert_rejected_without_stage(architecture="riscv64")
        self.assert_rejected_without_stage(schema_current=51)
        self.assert_rejected_without_stage(schema_current=53)
        # AMD64 files are never a fallback when the requested ARM64 file is absent.
        (self.root / "memory-arm64").unlink()
        self.assert_rejected_without_stage()

    def test_tampered_manifest_signature_and_artifact_are_rejected(self):
        path = self.root / "manifest.json"
        original = path.read_bytes()
        value = json.loads(original)
        value["release_version"] = "2.1.0-rc.2"
        path.write_bytes(TOOL.canonical(value))
        self.assert_rejected_without_stage()
        path.write_bytes(original)
        signature = self.root / "manifest.sig"
        original_signature = signature.read_bytes()
        signature.write_bytes(original_signature[:-1] + bytes([original_signature[-1] ^ 1]))
        self.assert_rejected_without_stage()
        signature.write_bytes(original_signature)
        (self.root / "memory-arm64").write_bytes(b"tampr fixture release")
        self.assert_rejected_without_stage()

    def test_unsigned_and_private_key_inputs_are_rejected(self):
        (self.root / "manifest.sig").write_bytes(b"")
        self.assert_rejected_without_stage()
        self.sign()
        (self.root / "key.asc").write_text("-----BEGIN PGP PRIVATE KEY BLOCK-----\nnot a real key\n")
        self.assert_rejected_without_stage()

    def test_selected_artifact_and_staging_parent_symlinks_are_rejected(self):
        artifact = self.root / "memory-arm64"
        artifact.rename(self.root / "real-arm64")
        artifact.symlink_to(self.root / "real-arm64")
        self.assert_rejected_without_stage()
        artifact.unlink()
        (self.root / "real-arm64").rename(artifact)
        (self.root / "staging-alias").symlink_to(self.root / "staging", target_is_directory=True)
        self.assert_rejected_without_stage(staging_parent=str(self.root / "staging-alias"))

    def test_signed_unknown_fields_and_traversal_are_rejected(self):
        original = json.loads((self.root / "manifest.json").read_bytes())
        for change in ("unknown", "traversal", "duplicate"):
            value = copy.deepcopy(original)
            if change == "unknown":
                value["execute_after_verify"] = "not allowed"
            elif change == "traversal":
                value["artifacts"][0]["name"] = "../escape"
            else:
                value["artifacts"].append(copy.deepcopy(value["artifacts"][0]))
            (self.root / "manifest.json").write_bytes(TOOL.canonical(value))
            self.sign()
            self.assert_rejected_without_stage()

    def test_staging_parent_must_be_private_and_owned_by_current_user(self):
        parent = self.root / "staging"
        for mode in (0o755, 0o777, 0o1777):
            with self.subTest(mode=oct(mode)):
                parent.chmod(mode)
                self.assert_rejected_without_stage()
        parent.chmod(0o700)
        other_uid = os.geteuid() + 1
        with mock.patch.object(TOOL.os, "geteuid", return_value=other_uid):
            self.assert_rejected_without_stage()

    def test_substituted_directory_before_open_preserves_unrelated_sentinel(self):
        stage_name = "memory-release-" + "c" * 32
        parent = self.root / "staging"
        unrelated = self.root / "unrelated-directory"
        unrelated.mkdir(mode=0o700)
        (unrelated / "sentinel").write_text("unrelated evidence must survive")
        original_open = os.open
        substituted = False

        def substitute(path, flags, *args, **kwargs):
            nonlocal substituted
            if path == stage_name and flags & os.O_DIRECTORY and not substituted:
                substituted = True
                (parent / stage_name).rename(parent / "original-created-directory")
                unrelated.rename(parent / stage_name)
            return original_open(path, flags, *args, **kwargs)

        with mock.patch.object(TOOL.secrets, "token_hex", return_value="c" * 32), \
             mock.patch.object(TOOL.os, "open", side_effect=substitute):
            with self.assertRaisesRegex(TOOL.ReleaseError, "identity changed"):
                TOOL.verify_stage(self.arguments())
        self.assertTrue(substituted)
        self.assertEqual((parent / stage_name / "sentinel").read_text(), "unrelated evidence must survive")
        self.assertEqual(list((parent / "original-created-directory").iterdir()), [])

    def test_substituted_directory_during_copy_is_not_cleaned_by_name(self):
        stage_name = "memory-release-" + "d" * 32
        parent = self.root / "staging"
        stage = parent / stage_name
        unrelated = self.root / "late-unrelated-directory"
        unrelated.mkdir(mode=0o700)
        (unrelated / "sentinel").write_text("preserved late substitution")
        (self.root / "memory-arm64").write_bytes(b"tampr fixture release")
        original_fsync = os.fsync
        substituted = False

        def substitute(descriptor):
            nonlocal substituted
            artifact = stage / "memory-arm64"
            if not substituted and artifact.exists() and os.fstat(descriptor).st_ino == artifact.stat().st_ino:
                substituted = True
                stage.rename(parent / "original-copied-directory")
                unrelated.rename(stage)
            return original_fsync(descriptor)

        with mock.patch.object(TOOL.secrets, "token_hex", return_value="d" * 32), \
             mock.patch.object(TOOL.os, "fsync", side_effect=substitute):
            with self.assertRaisesRegex(TOOL.ReleaseError, "signed digest"):
                TOOL.verify_stage(self.arguments())
        self.assertTrue(substituted)
        self.assertEqual((stage / "sentinel").read_text(), "preserved late substitution")
        self.assertEqual((parent / "original-copied-directory" / "memory-arm64").read_bytes(), b"tampr fixture release")

    def test_cleanup_preserves_replaced_and_untracked_files(self):
        stage_name = "memory-release-" + "e" * 32
        stage = self.root / "staging" / stage_name
        (self.root / "memory-arm64").write_bytes(b"tampr fixture release")
        original_fsync = os.fsync
        replaced = False

        def replace(descriptor):
            nonlocal replaced
            artifact = stage / "memory-arm64"
            if not replaced and artifact.exists() and os.fstat(descriptor).st_ino == artifact.stat().st_ino:
                replaced = True
                artifact.rename(self.root / "original-copied-artifact")
                artifact.write_text("replacement evidence")
                (stage / "untracked-sentinel").write_text("untracked evidence")
            return original_fsync(descriptor)

        with mock.patch.object(TOOL.secrets, "token_hex", return_value="e" * 32), \
             mock.patch.object(TOOL.os, "fsync", side_effect=replace):
            with self.assertRaisesRegex(TOOL.ReleaseError, "signed digest"):
                TOOL.verify_stage(self.arguments())
        self.assertTrue(replaced)
        self.assertEqual((stage / "memory-arm64").read_text(), "replacement evidence")
        self.assertEqual((stage / "untracked-sentinel").read_text(), "untracked evidence")


if __name__ == "__main__":
    unittest.main()
