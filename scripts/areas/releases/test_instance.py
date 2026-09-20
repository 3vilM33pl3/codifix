#!/usr/bin/env python3
"""Synthetic keys and files only. No artifacts, services or databases execute."""
import argparse
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TOOL = load("instance")
FIXTURE = load("test_release")


def archive(path, files=None, members=None):
    with tarfile.open(path, "w:gz") as stream:
        for name, data in (files or {"bin/memory": b"synthetic never executable", "web/index.html": b"<p>fixture</p>",
                                   "web/assets/main.js": b"fixture javascript"}).items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            stream.addfile(info, io.BytesIO(data))
        for info in members or []:
            stream.addfile(info)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="memory-instance-archive-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.path = self.root / "release.tar.gz"

    def inspect(self):
        size, digest = TOOL.release.measure(self.path)
        return TOOL.unpack(self.path, {"size_bytes": size, "sha256": digest})

    def test_complete_hashes_and_normalized_modes(self):
        archive(self.path)
        files = self.inspect()
        self.assertEqual(set(files), {"bin/memory", "web/index.html", "web/assets/main.js"})
        self.assertEqual(files["bin/memory"]["mode"], 0o500)
        self.assertEqual(files["web/index.html"]["mode"], 0o400)

    def test_path_traversal_absolute_and_hook_paths_rejected(self):
        for name in ("../escape", "/etc/passwd", "web/../escape", "web//x", "web/./x", "postinst", "bin/pi"):
            with self.subTest(name=name):
                archive(self.path, {"bin/memory": b"x", "web/index.html": b"x", name: b"x"})
                with self.assertRaises(TOOL.release.ReleaseError):
                    self.inspect()

    def test_links_devices_special_modes_and_duplicates_rejected(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE,
                     tarfile.XHDTYPE, tarfile.XGLTYPE, tarfile.GNUTYPE_LONGNAME, tarfile.GNUTYPE_SPARSE):
            member = tarfile.TarInfo("web/escape")
            member.type = kind
            member.linkname = "../../outside"
            archive(self.path, members=[member])
            with self.subTest(kind=kind), self.assertRaises(TOOL.release.ReleaseError):
                self.inspect()
        for name in ("web/index.html", "WEB/index.html"):
            member = tarfile.TarInfo(name)
            archive(self.path, members=[member])
            with self.assertRaises(TOOL.release.ReleaseError):
                self.inspect()
        member = tarfile.TarInfo("web/setuid")
        member.mode = 0o4755
        archive(self.path, members=[member])
        with self.assertRaises(TOOL.release.ReleaseError):
            self.inspect()

    def test_required_files_limits_and_bad_hash(self):
        archive(self.path, {"web/index.html": b"x"})
        with self.assertRaises(TOOL.release.ReleaseError):
            self.inspect()
        archive(self.path)
        for limit in ("MAX_FILES", "MAX_BYTES"):
            with mock.patch.object(TOOL, limit, 1), self.assertRaises(TOOL.release.ReleaseError):
                self.inspect()
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.unpack(self.path, {"size_bytes": self.path.stat().st_size, "sha256": "0" * 64})

    def test_source_symlink_and_parent_symlink_rejected(self):
        archive(self.path)
        alias = self.root / "alias"
        alias.symlink_to(self.path)
        with self.assertRaises(OSError):
            TOOL.unpack(alias, {"size_bytes": self.path.stat().st_size, "sha256": "0" * 64})
        directory_alias = self.root / "alias-directory"
        directory_alias.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            TOOL.release.open_regular(directory_alias / self.path.name)

    def test_instance_name_is_exact_not_shell_or_unit_pattern(self):
        for name in ("*", "trust.service", "../trust", "trust;true", "trust homelab", "Trust", "-trust", "a" * 20):
            with self.subTest(name=name), self.assertRaises(TOOL.release.ReleaseError):
                TOOL.instance_name(name)
        self.assertEqual(TOOL.instance_name("trust"), "trust")

    def test_interrupted_switch_restores_previous_without_overwrite(self):
        for name in ("release", "incoming"):
            (self.root / name).mkdir(mode=0o700)
        descriptor = TOOL.private_directory(self.root)
        try:
            before = TOOL.directory_identity(descriptor, "release")
            incoming = TOOL.directory_identity(descriptor, "incoming")
            def interrupt():
                raise RuntimeError("simulated interruption")
            with self.assertRaises(RuntimeError):
                TOOL.exchange(descriptor, "release", "incoming", "previous", before, incoming, interrupt)
            self.assertEqual(TOOL.directory_identity(descriptor, "release"), before)
            self.assertEqual(TOOL.directory_identity(descriptor, "incoming"), incoming)
            self.assertFalse((self.root / "previous").exists())
        finally:
            os.close(descriptor)

    def test_switch_rejects_symlink_and_preserves_existing_retained(self):
        for name in ("release", "incoming", "previous"):
            (self.root / name).mkdir(mode=0o700)
        descriptor = TOOL.private_directory(self.root)
        try:
            before = TOOL.directory_identity(descriptor, "release")
            incoming = TOOL.directory_identity(descriptor, "incoming")
            with self.assertRaises(TOOL.release.ReleaseError):
                TOOL.exchange(descriptor, "release", "incoming", "previous", before, incoming)
            (self.root / "incoming").rmdir()
            (self.root / "incoming").symlink_to(self.root / "previous", target_is_directory=True)
            with self.assertRaises(TOOL.release.ReleaseError):
                TOOL.exchange(descriptor, "release", "incoming", "other", before, incoming)
            self.assertEqual(TOOL.directory_identity(descriptor, "release"), before)
        finally:
            os.close(descriptor)

    def test_interruption_path_substitution_preserves_prior_and_unexpected_target(self):
        for name in ("release", "incoming", "unrelated"):
            (self.root / name).mkdir(mode=0o700)
        descriptor = TOOL.private_directory(self.root)
        try:
            before = TOOL.directory_identity(descriptor, "release")
            incoming = TOOL.directory_identity(descriptor, "incoming")
            def replace_destination():
                (self.root / "release").symlink_to(self.root / "unrelated", target_is_directory=True)
            with self.assertRaises(TOOL.release.ReleaseError):
                TOOL.exchange(descriptor, "release", "incoming", "previous", before, incoming, replace_destination)
            self.assertEqual(TOOL.directory_identity(descriptor, "previous"), before)
            self.assertTrue((self.root / "release").is_symlink())
            self.assertTrue((self.root / "unrelated").is_dir())
        finally:
            os.close(descriptor)

    def test_atomic_no_replace_rejects_competing_empty_directory(self):
        for name in ("source", "destination"):
            (self.root / name).mkdir(mode=0o700)
        descriptor = TOOL.private_directory(self.root)
        try:
            before = TOOL.directory_identity(descriptor, "source")
            other = TOOL.directory_identity(descriptor, "destination")
            with self.assertRaises(FileExistsError):
                TOOL.rename_noreplace(descriptor, "source", "destination")
            self.assertEqual(TOOL.directory_identity(descriptor, "source"), before)
            self.assertEqual(TOOL.directory_identity(descriptor, "destination"), other)
        finally:
            os.close(descriptor)


@unittest.skipUnless(shutil.which("gpg") and shutil.which("gpgconf"), "GPG unavailable; crypto evidence not established")
class SignedPreparationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        FIXTURE.SignatureTests.setUpClass()
        cls.addClassCleanup(FIXTURE.SignatureTests.tearDownClass)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="memory-instance-signed-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in ("stages", "rehearsals"):
            (self.root / name).mkdir(mode=0o700)

    def bundle(self, version=1, schema=60, **changes):
        source = self.root / ("bundle-" + str(version))
        source.mkdir(mode=0o700)
        archive(source / "memory-arm64.tar.gz", {"bin/memory": f"never executed {version}".encode(),
                                                "web/index.html": f"<p>{version}</p>".encode()})
        (source / "memory-amd64").write_bytes(b"unselected synthetic artifact")
        (source / "signing-public-key.asc").write_bytes(FIXTURE.SignatureTests.public)
        args = FIXTURE.creator(source, version=f"2.1.0-rc.{version}", schema_floor=schema, schema_head=schema,
                               artifact=[f"amd64:binary:{source}/memory-amd64",
                                         f"arm64:archive:{source}/memory-arm64.tar.gz"], **changes)
        TOOL.release.create(args)
        FIXTURE.SignatureTests.command(["--local-user", FIXTURE.SignatureTests.signer + "!", "--digest-algo", "SHA256",
                                       "--output", str(source / "manifest.sig"), "--detach-sign", str(source / "manifest.json")])
        return argparse.Namespace(manifest=str(source / "manifest.json"), signature=str(source / "manifest.sig"),
            public_key=str(source / "signing-public-key.asc"), trusted_signing_fingerprint=FIXTURE.SignatureTests.signer,
            architecture="arm64", schema_current=schema, artifacts_dir=str(source), staging_parent=str(self.root / "stages"),
            instance="trust", release_archive="memory-arm64.tar.gz")

    def prepare(self, version=1, schema=60, **changes):
        return TOOL.prepare(self.bundle(version, schema, **changes))

    def rehearsal(self, prior, candidate, **changes):
        values = dict(previous=prior["preparation_directory"], prepared=candidate["preparation_directory"],
                      previous_trusted_signing_fingerprint=FIXTURE.SignatureTests.signer,
                      trusted_signing_fingerprint=FIXTURE.SignatureTests.signer,
                      schema_current=60, instance="trust", output_parent=str(self.root / "rehearsals"))
        values.update(changes)
        return TOOL.rehearse(argparse.Namespace(**values))

    def test_preparation_is_fresh_concrete_bound_and_inactive(self):
        args = self.bundle()
        first, second = TOOL.prepare(args), TOOL.prepare(args)
        self.assertNotEqual(first["preparation_directory"], second["preparation_directory"])
        self.assertEqual(first["files"], second["files"])
        self.assertFalse(first["installation_authorized"])
        self.assertFalse(first["artifact_execution_tested"])
        self.assertFalse(first["database_schema_verified"])
        self.assertEqual(first["plan"]["unit"], "memory-layer@trust.service")
        verified = TOOL.verify_prepared(first["preparation_directory"], args.trusted_signing_fingerprint, "trust", 60)
        self.assertEqual(verified["manifest_sha256"], first["manifest_sha256"])
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(first["preparation_directory"], args.trusted_signing_fingerprint, "general", 60)

    def test_exact_binary_and_web_roundtrip_switch_rollback_without_execution(self):
        prior, candidate = self.prepare(), self.prepare(2)
        result = self.rehearsal(prior, candidate)
        self.assertEqual(result["status"], "filesystem_switch_and_rollback_verified")
        self.assertTrue(result["prepared_sources_unchanged"])
        self.assertFalse(result["service_or_binary_executed"])
        root = Path(result["rehearsal_directory"])
        self.assertEqual((root / "trust/release/bin/memory").read_bytes(), b"never executed 1")
        self.assertEqual((root / "trust/retained-candidate/web/index.html").read_bytes(), b"<p>2</p>")
        self.assertEqual(sorted(path.name for path in root.iterdir()), ["result.json", "scope.json", "trust"])

    def test_signed_hash_mutation_and_wrong_trust_fail(self):
        args = self.bundle()
        changed = copy.copy(args)
        changed.trusted_signing_fingerprint = "A" * 40
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.prepare(changed)
        source = Path(args.artifacts_dir) / args.release_archive
        source.write_bytes(source.read_bytes() + b"tampering")
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.prepare(args)

    def test_unsigned_tampered_manifest_and_wrong_archive_selection_fail(self):
        args = self.bundle()
        changed = copy.copy(args)
        changed.release_archive = "memory-amd64"
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.prepare(changed)
        manifest = Path(args.manifest)
        value = json.loads(manifest.read_bytes())
        value["source_revision"] = "b" * 40
        manifest.write_bytes(TOOL.release.canonical(value))
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.prepare(args)

    def test_changed_metadata_files_modes_hardlinks_and_extra_files_fail(self):
        prepared = self.prepare()
        root = Path(prepared["preparation_directory"])
        signer = FIXTURE.SignatureTests.signer
        original = (root / TOOL.PREPARATION).read_bytes()
        value = json.loads(original)
        value["installation_authorized"] = True
        (root / TOOL.PREPARATION).write_bytes(TOOL.release.canonical(value))
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, signer, "trust", 60)
        (root / TOOL.PREPARATION).write_bytes(original)
        binary = root / "release/bin/memory"
        os.chmod(binary, 0o700)
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, signer, "trust", 60)
        os.chmod(binary, 0o500)
        os.link(binary, root / "extra-hardlink")
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, signer, "trust", 60)
        (root / "extra-hardlink").unlink()
        (root / "release/web/injected.js").write_bytes(b"unreviewed script")
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, signer, "trust", 60)

    def test_changed_web_body_and_symlink_fail(self):
        prepared = self.prepare()
        root = Path(prepared["preparation_directory"])
        web = root / "release/web/index.html"
        os.chmod(web, 0o600)
        web.write_bytes(b"changed")
        os.chmod(web, 0o400)
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, FIXTURE.SignatureTests.signer, "trust", 60)
        web.unlink()
        web.symlink_to(root / "release/bin/memory")
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.verify_prepared(root, FIXTURE.SignatureTests.signer, "trust", 60)

    def test_newer_schema_and_reverse_versions_block_rehearsal(self):
        prior, candidate = self.prepare(), self.prepare(2)
        with self.assertRaises(TOOL.release.ReleaseError):
            self.rehearsal(prior, candidate, schema_current=61)
        with self.assertRaises(TOOL.release.ReleaseError):
            self.rehearsal(candidate, prior)
        self.assertEqual(list((self.root / "rehearsals").iterdir()), [])

    def test_migrations_block_automatic_filesystem_rollback_claim(self):
        prior = self.prepare()
        candidate = self.prepare(2, classification="approval-required", migrations=True)
        with self.assertRaises(TOOL.release.ReleaseError):
            self.rehearsal(prior, candidate)

    def test_private_root_and_ancestor_symlink_required(self):
        args = self.bundle()
        os.chmod(self.root / "stages", 0o755)
        with self.assertRaises(TOOL.release.ReleaseError):
            TOOL.prepare(args)
        os.chmod(self.root / "stages", 0o700)
        alias = self.root / "aliased-stage"
        alias.symlink_to(self.root / "stages", target_is_directory=True)
        args.staging_parent = str(alias)
        with self.assertRaises(OSError):
            TOOL.prepare(args)

    def test_isolated_cli_preparation_and_rehearsal_json(self):
        reports = []
        executable = Path(__file__).with_name("instance.py")
        for version in (1, 2):
            args = vars(self.bundle(version))
            command = [sys.executable, "-I", "-S", str(executable), "prepare"]
            for name, value in args.items():
                command.extend(["--" + name.replace("_", "-"), str(value)])
            result = subprocess.run(command, check=True, capture_output=True, timeout=30)
            reports.append(json.loads(result.stdout))
        command = [sys.executable, "-I", "-S", str(executable), "rehearse", "--previous",
                   reports[0]["preparation_directory"], "--prepared", reports[1]["preparation_directory"],
                   "--trusted-signing-fingerprint", FIXTURE.SignatureTests.signer,
                   "--previous-trusted-signing-fingerprint", FIXTURE.SignatureTests.signer,
                   "--instance", "trust", "--schema-current", "60", "--output-parent", str(self.root / "rehearsals")]
        result = subprocess.run(command, check=True, capture_output=True, timeout=30)
        self.assertEqual(json.loads(result.stdout)["status"], "filesystem_switch_and_rollback_verified")


if __name__ == "__main__":
    unittest.main()
