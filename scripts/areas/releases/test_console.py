#!/usr/bin/env python3
"""Offline installer tests: disposable signatures, synthetic bytes, no services."""
import argparse
from contextlib import contextmanager
import copy
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import stat
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.dont_write_bytecode = True


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TOOL = load("console")
FIXTURE = load("test_instance")
ARCHITECTURE = {"x86_64": "amd64", "aarch64": "arm64"}.get(platform.machine())
ERRORS = (TOOL.release.ReleaseError, OSError, ValueError, TypeError, KeyError)


class Interrupted(BaseException):
    """An injected process loss; production exception recovery must not run."""


class SystemdTests(unittest.TestCase):
    def state(self, **changes):
        unit = "memory-layer@trust.service"
        value = dict(LoadState="not-found", ActiveState="inactive", SubState="dead", MainPID="0",
            ControlPID="0", Job="", Id=unit, Names=unit, FragmentPath="", DropInPaths="",
            Transient="no", UnitFileState="")
        value.update(changes)
        return "".join(f"{name}={contents}\n" for name, contents in value.items()).encode()

    def test_systemctl_uses_fixed_command_and_finite_environment(self):
        result = mock.Mock(stdout=self.state(), stderr=b"", returncode=1)
        with mock.patch.dict(os.environ, {"PATH": "/untrusted", "DBUS_SYSTEM_BUS_ADDRESS": "untrusted",
                                         "SYSTEMD_HOST": "untrusted"}), \
                mock.patch.object(TOOL.subprocess, "run", return_value=result) as run:
            TOOL.check_systemd("trust")
        command = run.call_args.args[0]
        self.assertEqual(command[:3], ["/usr/bin/systemctl", "show", "--no-pager"])
        self.assertEqual(command[-2:], ["--", "memory-layer@trust.service"])
        self.assertEqual(run.call_args.kwargs["env"],
                         {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "SYSTEMD_PAGER": ""})
        self.assertEqual(run.call_args.kwargs["timeout"], 15)

    def test_systemctl_rejects_active_pending_alias_override_and_malformed_state(self):
        invalid = [self.state(ActiveState="active"), self.state(MainPID="123"), self.state(ControlPID="123"),
                   self.state(Job="17 /org/freedesktop/systemd1/job/17"), self.state(Transient="yes"),
                   self.state(Names="memory-layer@trust.service other.service"),
                   self.state(DropInPaths="/etc/systemd/system/memory-layer@trust.service.d/override.conf"),
                   self.state(UnitFileState="enabled"), self.state().replace(b"ControlPID=0\n", b""),
                   self.state() + b"MainPID=0\n", self.state() + b"malformed\n"]
        for index, output in enumerate(invalid):
            with self.subTest(case=index), mock.patch.object(TOOL.subprocess, "run",
                    return_value=mock.Mock(stdout=output, stderr=b"", returncode=0)):
                with self.assertRaises(TOOL.release.ReleaseError):
                    TOOL.check_systemd("trust")

    def test_reviewed_unit_digest_matches_repository_and_loaded_unit(self):
        template = Path(__file__).resolve().parents[3] / "packaging/debian/memory-layer@.service"
        contents = template.read_bytes()
        self.assertEqual(TOOL.checksum(contents), TOOL.UNIT_SHA256)
        fragment = Path("/usr/lib/systemd/system/memory-layer@.service")
        result = mock.Mock(stdout=self.state(LoadState="loaded", UnitFileState="disabled",
                          FragmentPath=str(fragment)), stderr=b"", returncode=0)
        with mock.patch.object(TOOL.subprocess, "run", return_value=result), \
                mock.patch.object(TOOL.Path, "resolve", return_value=fragment), \
                mock.patch.object(TOOL, "protected_bytes", return_value=contents):
            TOOL.check_systemd("trust")
        with mock.patch.object(TOOL.subprocess, "run", return_value=result), \
                mock.patch.object(TOOL.Path, "resolve", return_value=fragment), \
                mock.patch.object(TOOL, "protected_bytes", return_value=contents + b"# changed\n"):
            with self.assertRaises(TOOL.release.ReleaseError):
                TOOL.check_systemd("trust")


@unittest.skipUnless(shutil.which("gpg") and shutil.which("gpgconf") and ARCHITECTURE,
                     "GPG or supported native architecture unavailable; crypto tests not run")
class ConsoleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        FIXTURE.FIXTURE.SignatureTests.setUpClass()
        cls.addClassCleanup(FIXTURE.FIXTURE.SignatureTests.tearDownClass)

    def setUp(self):
        previous_umask = os.umask(0o077)
        self.addCleanup(os.umask, previous_umask)
        # The production trust walk permits root-owned sticky /tmp; the host's
        # ambient TMPDIR can be a user-owned shared directory and is untrusted.
        self.temporary = tempfile.TemporaryDirectory(prefix="memory-console-test-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.layout = TOOL.Layout(self.root / "host")
        self.unit_check = mock.Mock()
        source = self.root / "bundle"
        source.mkdir(mode=0o700)
        stages = self.root / "stages"
        stages.mkdir(mode=0o700)
        # This is an ELF identity header, not a runnable executable: ET_NONE,
        # zero program headers and no instructions. It is never executed.
        binary = bytearray(64)
        binary[:6] = b"\x7fELF\x02\x01"
        binary[18:20] = b"\x3e\x00" if ARCHITECTURE == "amd64" else b"\xb7\x00"
        self.binary = bytes(binary) + b"synthetic fixture, never executed"
        self.files = {"bin/memory": self.binary, "web/index.html": b"<p>signed fixture</p>",
                      "web/assets/deep/main.js": b"/* signed synthetic asset */"}
        archive = f"memory-{ARCHITECTURE}.tar.gz"
        FIXTURE.archive(source / archive, self.files)
        other = "arm64" if ARCHITECTURE == "amd64" else "amd64"
        (source / ("memory-" + other)).write_bytes(b"unselected synthetic artifact")
        (source / "signing-public-key.asc").write_bytes(FIXTURE.FIXTURE.SignatureTests.public)
        TOOL.release.create(FIXTURE.FIXTURE.creator(source, schema_floor=60, schema_head=60,
            artifact=[f"{ARCHITECTURE}:archive:{source / archive}",
                      f"{other}:binary:{source / ('memory-' + other)}"]))
        FIXTURE.FIXTURE.SignatureTests.command(["--local-user", FIXTURE.FIXTURE.SignatureTests.signer + "!", "--digest-algo", "SHA256",
            "--output", str(source / "manifest.sig"), "--detach-sign", str(source / "manifest.json")])
        prepared = TOOL.instance.prepare(argparse.Namespace(manifest=str(source / "manifest.json"),
            signature=str(source / "manifest.sig"), public_key=str(source / "signing-public-key.asc"),
            trusted_signing_fingerprint=FIXTURE.FIXTURE.SignatureTests.signer, architecture=ARCHITECTURE, schema_current=60,
            artifacts_dir=str(source), staging_parent=str(stages), instance="trust", release_archive=archive))
        self.prepared = Path(prepared["preparation_directory"])
        template = Path(__file__).resolve().parents[3] / "deploy/areas/instance.toml.example"
        self.config = template.read_text().replace('api_token = ""', 'api_token = "' + "x" * 48 + '"')
        self.config = self.config.replace("instances/REVIEW/", "instances/trust/").replace(
            "postgresql://REVIEW:REVIEW@127.0.0.1:5432/REVIEW",
            "postgresql://fixture:synthetic-password@127.0.0.1:5432/memory_fixture")
        self.config_path = self.root / "configuration.toml"
        self.write_config(self.config)
        self.args = argparse.Namespace(instance="trust", prepared=str(self.prepared), config=str(self.config_path),
            config_sha256=TOOL.checksum(self.config.encode()), database_name="memory_fixture", schema_current=60,
            trusted_signing_fingerprint=FIXTURE.FIXTURE.SignatureTests.signer)

    def write_config(self, text):
        self.config_path.write_text(text)
        self.config_path.chmod(0o600)

    @contextmanager
    def console(self, fault=None):
        console = TOOL.Console(self.layout, unit_check=self.unit_check, **({"fault": fault} if fault else {}))
        try:
            yield console
        finally:
            console.close()

    def install(self, args=None, fault=None):
        with self.console(fault) as console:
            return console.install(args or self.args)

    def recover(self, action="finish", fault=None, signer=None):
        with self.console(fault) as console:
            console.open("trust")
            console.load(signer or FIXTURE.FIXTURE.SignatureTests.signer)
            return console.run(action)

    def interrupt(self, point):
        def fault(observed):
            if observed == point:
                raise Interrupted(point)
        with self.assertRaises(Interrupted):
            self.install(fault=fault)

    def assert_inactive(self, result):
        for field in ("activation_authorized", "service_mutated", "database_contacted",
                      "database_schema_verified", "native_abi_or_sandbox_proven"):
            self.assertIs(result[field], False)
        self.assertFalse((self.layout.config / "trust/activation-reviewed").exists())
        self.assertTrue(self.unit_check.called)
        self.assertTrue(all(call == mock.call("trust") for call in self.unit_check.call_args_list))

    def test_install_inspect_and_retain_exact_signed_files(self):
        installed = self.install()
        self.assertEqual(installed["status"], "installed_inactive")
        self.assert_inactive(installed)
        # Root's CLI umask is 0077. Public release ancestors must still be
        # searchable by the service's DynamicUser after their creation.
        for path in (self.layout.release, self.layout.release.parent, self.layout.release.parent.parent):
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755, str(path))
        for name, contents in self.files.items():
            path = self.layout.release / "trust/release" / name
            self.assertEqual(path.read_bytes(), contents)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755 if name == "bin/memory" else 0o644)
        self.assertEqual((self.layout.config / "trust/memory-layer.toml").read_bytes(), self.config.encode())
        self.assertEqual((self.layout.config / "trust/memory.sha256").read_text(),
                         TOOL.checksum(self.binary) + "\n")
        self.assertEqual(stat.S_IMODE((self.layout.config / "trust").stat().st_mode), 0o700)
        self.assertEqual(self.recover("inspect")["status"], "installed")
        retained = self.recover("retain")
        self.assertEqual(retained["status"], "retained_inactive")
        self.assert_inactive(retained)
        for role in ("release", "config"):
            parent = getattr(self.layout, role)
            self.assertFalse((parent / "trust").exists())
            self.assertTrue((parent / (".retained-trust-" + retained["transaction"])).is_dir())
        self.assertEqual(self.recover("retain")["status"], "retained_inactive")
        with self.assertRaises(ERRORS):
            self.recover("finish")

    def test_existing_release_and_config_paths_are_preserved(self):
        for role in ("release", "config"):
            with self.subTest(role=role):
                self.layout = TOOL.Layout(self.root / ("existing-" + role))
                parent = getattr(self.layout, role)
                TOOL.ensure_directory(parent, 0o755)
                existing = parent / "trust"
                existing.mkdir(mode=0o700)
                sentinel = existing / "operator-file"
                sentinel.write_bytes(b"preserve existing instance")
                with self.assertRaises(ERRORS):
                    self.install()
                self.assertEqual(sentinel.read_bytes(), b"preserve existing instance")

    def test_existing_committed_name_refuses_second_install(self):
        self.install()
        before = (self.layout.journal / "trust/plan.json").read_bytes()
        with self.assertRaises(ERRORS):
            self.install()
        self.assertEqual((self.layout.journal / "trust/plan.json").read_bytes(), before)

    def test_existing_uncommitted_journal_is_not_adopted(self):
        TOOL.ensure_directory(self.layout.journal)
        root = self.layout.journal / "trust"
        root.mkdir(mode=0o700)
        sentinel = root / "operator-notes"
        sentinel.write_bytes(b"uncommitted work requires inspection")
        with self.assertRaises(ERRORS):
            self.install()
        self.assertEqual(sentinel.read_bytes(), b"uncommitted work requires inspection")
        self.assertFalse((root / "plan.json").exists())
        self.assertFalse((root / "configuration.toml").exists())
        self.assertFalse((self.layout.release / "trust").exists())

    def test_existing_runtime_state_and_cache_names_are_preserved(self):
        for index in range(3):
            with self.subTest(location=index):
                self.layout = TOOL.Layout(self.root / ("runtime-remnant-" + str(index)))
                parent = self.layout.previous_state[index]
                TOOL.ensure_directory(parent, 0o755)
                existing = parent / "trust"
                existing.mkdir(mode=0o700)
                with self.assertRaises(ERRORS):
                    self.install()
                self.assertTrue(existing.is_dir())

    def test_config_hash_mode_symlink_and_hardlink_rejected(self):
        bad = copy.copy(self.args)
        bad.config_sha256 = "0" * 64
        with self.assertRaises(ERRORS):
            self.install(bad)
        self.config_path.chmod(0o644)
        with self.assertRaises(ERRORS):
            self.install()
        self.config_path.chmod(0o600)
        hardlink = self.root / "config-hardlink"
        os.link(self.config_path, hardlink)
        with self.assertRaises(ERRORS):
            self.install()
        hardlink.unlink()
        symlink = self.root / "config-symlink"
        symlink.symlink_to(self.config_path)
        bad.config = str(symlink)
        bad.config_sha256 = self.args.config_sha256
        with self.assertRaises(ERRORS):
            self.install(bad)
        self.assertFalse(self.layout.release.exists())

    def test_unsafe_config_is_rejected_even_with_matching_review_hash(self):
        mutations = [self.config.replace("read_only = true", "read_only = false", 1),
                     self.config.replace("127.0.0.1:4140", "0.0.0.0:4140"),
                     self.config.replace("instances/trust/", "instances/other/"),
                     self.config.replace("[automation]\nenabled = false", "[automation]\nenabled = true"),
                     self.config.replace("5432/memory_fixture", "5432/other_database"),
                     self.config + "\n[unreviewed]\nenabled = true\n"]
        for index, content in enumerate(mutations):
            with self.subTest(mutation=index):
                self.write_config(content)
                args = copy.copy(self.args)
                args.config_sha256 = TOOL.checksum(content.encode())
                with self.assertRaises(ERRORS):
                    self.install(args)
        self.assertFalse(self.layout.release.exists())

    def test_changed_prepared_bytes_and_wrong_signer_are_rejected(self):
        args = copy.copy(self.args)
        args.trusted_signing_fingerprint = "A" * 40
        with self.assertRaises(ERRORS):
            self.install(args)
        binary = self.prepared / "release/bin/memory"
        binary.chmod(0o600)
        binary.write_bytes(self.binary[:-1] + b"X")
        binary.chmod(0o500)
        with self.assertRaises(ERRORS):
            self.install()
        self.assertFalse(self.layout.release.exists())

    def test_concurrent_recovery_is_excluded(self):
        self.install()
        with self.console() as owner:
            owner.open("trust")
            with self.console() as contender:
                with self.assertRaisesRegex(TOOL.release.ReleaseError, "Another installer"):
                    contender.open("trust")
        self.assertEqual(self.recover()["status"], "installed_inactive")

    def test_restart_after_each_publication_boundary(self):
        for point in ("journal_ready", "renamed_release", "journal_release_done", "renamed_config"):
            with self.subTest(point=point):
                self.layout = TOOL.Layout(self.root / point)
                self.interrupt(point)
                result = self.recover()
                self.assertEqual(result["status"], "installed_inactive")
                self.assert_inactive(result)
                self.assertEqual((self.layout.release / "trust/release/bin/memory").read_bytes(), self.binary)
                self.assertEqual((self.layout.config / "trust/memory-layer.toml").read_bytes(), self.config.encode())
                self.assertEqual(self.recover()["transaction"], result["transaction"])

    @unittest.skipUnless(hasattr(os, "fork"), "Fork required for actual SIGKILL recovery evidence")
    def test_sigkill_process_loss_after_release_and_config_renames(self):
        # A real process death proves lock release and restart reconciliation.
        # It does not simulate cache loss or establish power-loss durability.
        for point in ("renamed_release", "renamed_config"):
            with self.subTest(point=point):
                self.layout = TOOL.Layout(self.root / ("sigkill-" + point))
                pid = os.fork()
                if pid == 0:
                    def fault(observed):
                        if observed == point:
                            os.kill(os.getpid(), signal.SIGKILL)
                    try:
                        self.install(fault=fault)
                    except BaseException:
                        os._exit(2)
                    os._exit(3)
                deadline = time.monotonic() + 30
                while True:
                    waited, status = os.waitpid(pid, os.WNOHANG)
                    if waited:
                        break
                    if time.monotonic() >= deadline:
                        os.kill(pid, signal.SIGKILL)
                        os.waitpid(pid, 0)
                        self.fail("Synthetic child did not reach the intended SIGKILL boundary within 30 seconds")
                    time.sleep(0.05)
                self.assertTrue(os.WIFSIGNALED(status), f"Child exited without intended SIGKILL: {status}")
                self.assertEqual(os.WTERMSIG(status), signal.SIGKILL)
                result = self.recover()
                self.assertEqual(result["status"], "installed_inactive")
                self.assert_inactive(result)
                self.assertEqual((self.layout.release / "trust/release/bin/memory").read_bytes(), self.binary)
                self.assertEqual((self.layout.config / "trust/memory-layer.toml").read_bytes(), self.config.encode())

    def test_interrupted_copy_preserves_old_stage_and_can_finish(self):
        self.interrupt("copy_file")
        old = tuple(self.layout.release.glob(".incoming-trust-*"))
        self.assertEqual(len(old), 1)
        self.assertEqual(self.recover()["status"], "installed_inactive")
        self.assertTrue(old[0].is_dir())

    def test_replaced_stage_with_identical_bytes_is_preserved_and_rejected(self):
        self.interrupt("journal_ready")
        stage = next(self.layout.release.glob(".incoming-trust-*"))
        preserved = stage.with_name("preserved-original-stage")
        stage.rename(preserved)
        shutil.copytree(preserved, stage)
        with self.assertRaises(ERRORS):
            self.recover()
        self.assertTrue(preserved.is_dir())
        self.assertTrue(stage.is_dir())
        self.assertFalse((self.layout.release / "trust").exists())

    def test_retention_resumes_after_ambiguous_renames(self):
        for point in ("renamed_release", "renamed_config"):
            with self.subTest(point=point):
                self.layout = TOOL.Layout(self.root / ("retain-" + point))
                self.install()
                def fault(observed):
                    if observed == point:
                        raise Interrupted(point)
                with self.assertRaises(Interrupted):
                    self.recover("retain", fault=fault)
                with self.assertRaises(ERRORS):
                    self.recover("finish")
                self.assertEqual(self.recover("retain")["status"], "retained_inactive")

    def test_changed_installed_content_mode_links_and_extra_files_block_recovery(self):
        for mutation in ("content", "mode", "hardlink", "symlink", "extra"):
            with self.subTest(mutation=mutation):
                self.layout = TOOL.Layout(self.root / ("mutation-" + mutation))
                self.install()
                path = self.layout.release / "trust/release/web/index.html"
                if mutation == "content":
                    data = path.read_bytes()
                    path.write_bytes(b"X" + data[1:])
                elif mutation == "mode":
                    path.chmod(0o600)
                elif mutation == "hardlink":
                    os.link(path, self.root / "outside-hardlink")
                elif mutation == "symlink":
                    path.unlink()
                    path.symlink_to(self.prepared / "release/web/index.html")
                else:
                    (path.parent / "unexpected.js").write_bytes(b"unreviewed")
                for action in ("inspect", "finish", "retain"):
                    with self.assertRaises(ERRORS):
                        self.recover(action)
                self.assertTrue(path.exists() or path.is_symlink())

    def test_ambiguous_rename_destination_is_never_overwritten(self):
        self.interrupt("journal_ready")
        target = self.layout.release / "trust"
        target.mkdir(mode=0o755)
        sentinel = target / "unexpected"
        sentinel.write_bytes(b"belongs to someone else")
        with self.assertRaises(ERRORS):
            self.recover()
        self.assertEqual(sentinel.read_bytes(), b"belongs to someone else")
        self.assertEqual(len(list(self.layout.release.glob(".incoming-trust-*"))), 1)

    def test_changed_config_snapshot_or_journal_is_rejected(self):
        for mutation in ("configuration", "event", "plan"):
            with self.subTest(mutation=mutation):
                self.layout = TOOL.Layout(self.root / ("journal-" + mutation))
                self.interrupt("journal_ready")
                root = self.layout.journal / "trust"
                if mutation == "configuration":
                    path = root / "configuration.toml"
                    path.write_bytes(path.read_bytes() + b"\n# changed\n")
                elif mutation == "event":
                    path = sorted(root.glob("event-*.json"))[-1]
                    path.write_bytes(path.read_bytes()[:-4])
                else:
                    path = root / "plan.json"
                    value = json.loads(path.read_bytes())
                    value["config_sha256"] = "0" * 64
                    path.write_bytes(TOOL.release.canonical(value))
                with self.assertRaises(ERRORS):
                    self.recover()
                self.assertFalse((self.layout.release / "trust").exists())

    def test_activation_marker_blocks_inspect_recovery_and_retention(self):
        self.install()
        marker = self.layout.config / "trust/activation-reviewed"
        marker.write_bytes(b"operator marker: preserve")
        for action in ("inspect", "finish", "retain"):
            with self.assertRaises(ERRORS):
                self.recover(action)
        self.assertEqual(marker.read_bytes(), b"operator marker: preserve")

    def test_unit_becoming_active_blocks_remaining_publication(self):
        def fault(point):
            if point == "journal_release_done":
                self.unit_check.side_effect = TOOL.release.ReleaseError("synthetic active unit")
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "synthetic active"):
            self.install(fault=fault)
        self.assertTrue((self.layout.release / "trust").is_dir())
        self.assertFalse((self.layout.config / "trust").exists())
        self.unit_check.side_effect = None
        self.assertEqual(self.recover()["status"], "installed_inactive")


if __name__ == "__main__":
    unittest.main()
