#!/usr/bin/env python3
"""Upgrade crash/rollback tests with synthetic signed ELF bytes and a fake host."""
import argparse
from contextlib import contextmanager
import importlib.util
import os
from pathlib import Path
import shutil
import unittest
from unittest import mock


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


TOOL, FIXTURE = load("upgrade"), load("test_console")


class FakeHost:
    def __init__(self):
        self.active, self.schema, self.target_schema = True, 60, 60
        self.bindings = {"/etc/systemd/system/memory-layer@trust.service.d/ca.conf": "a" * 64}
        self.calls, self.peers, self.healthy = [], 0, True

    def inspect(self, name):
        return {"ActiveState": "active" if self.active else "inactive"}, self.bindings.copy()

    def stopped(self, name):
        TOOL.require(not self.active, "Service still active")

    def stop(self, name):
        self.calls.append(("stop", name))
        self.active = False

    def start(self, name):
        self.calls.append(("start", name))
        self.active, self.schema = True, self.target_schema

    def database(self, config, name, fenced):
        TOOL.require(not fenced or (not self.active and self.peers == 0), "Other database sessions remain")
        return self.schema

    def health(self, config, name, version):
        self.calls.append(("health", name, version))
        TOOL.require(self.active and self.healthy, "Candidate health not verified")


class HostTests(unittest.TestCase):
    def test_command_finite_environment_and_no_shell(self):
        with mock.patch.object(TOOL.subprocess, "run", return_value=mock.Mock(returncode=0, stdout=b"ok")) as run:
            TOOL.command(["/usr/bin/systemctl", "stop", "--", "memory-layer@trust.service"])
        self.assertNotIn("shell", run.call_args.kwargs)
        self.assertEqual(run.call_args.kwargs["env"], TOOL.ENV)
        self.assertEqual(run.call_args.kwargs["timeout"], 30)

    def test_psql_uses_no_password_in_arguments_and_checks_writers(self):
        config = (b'[database]\nurl="postgresql://fixture:secret@127.0.0.1:5433/memory_trust"\n'
                  b'[service]\nbind_addr="127.0.0.1:8091"\n'
                  b'web_root="/opt/memory-layer/instances/trust/release/web"\n')
        with mock.patch.object(TOOL, "command", return_value=b"60|0|0\n") as run:
            self.assertEqual(TOOL.Host().database(config, "trust", True), 60)
        self.assertNotIn("secret", str(run.call_args.args[0]))
        self.assertEqual(run.call_args.args[1]["PGPASSWORD"], "secret")
        for result in (b"60|0|1", b"60|1|0", b"60\n"):
            with self.subTest(result=result), mock.patch.object(TOOL, "command", return_value=result):
                with self.assertRaises(TOOL.release.ReleaseError):
                    TOOL.Host().database(config, "trust", True)

    def test_loopback_and_exact_web_root_required(self):
        for url, root in (("postgresql://u:p@example.com/db", "trust"),
                          ("postgresql://u:p@127.0.0.1/db?sslmode=disable", "trust"),
                          ("postgresql://u:p@127.0.0.1/db", "other")):
            config = f'[database]\nurl="{url}"\n[service]\nbind_addr="127.0.0.1:8091"\nweb_root="/opt/memory-layer/instances/{root}/release/web"\n'
            with self.assertRaises(TOOL.release.ReleaseError):
                TOOL.config_binding(config.encode(), "trust")


@unittest.skipUnless(shutil.which("gpg") and FIXTURE.ARCHITECTURE, "Disposable signature fixture unavailable")
class UpgradeTests(unittest.TestCase):
    setUpClass = classmethod(FIXTURE.ConsoleTests.setUpClass.__func__)
    write_config = FIXTURE.ConsoleTests.write_config

    def setUp(self):
        FIXTURE.ConsoleTests.setUp(self)
        installer = FIXTURE.TOOL.Console(self.layout, unit_check=lambda _: None)
        try:
            installer.install(self.args)
        finally:
            installer.close()
        self.host = FakeHost()
        self.signer = FIXTURE.FIXTURE.FIXTURE.SignatureTests.signer
        source = self.root / "next-bundle"
        source.mkdir(mode=0o700)
        archive = "next-memory.tar.gz"
        FIXTURE.FIXTURE.archive(source / archive, {**self.files, "web/index.html": b"<p>new fixture</p>"})
        other = "arm64" if FIXTURE.ARCHITECTURE == "amd64" else "amd64"
        (source / "other-binary").write_bytes(b"unused fixture")
        (source / "signing-public-key.asc").write_bytes(FIXTURE.FIXTURE.FIXTURE.SignatureTests.public)
        TOOL.release.create(FIXTURE.FIXTURE.FIXTURE.creator(source, version="2.1.0-rc.2", schema_floor=60, schema_head=61,
            classification="approval-required", migrations=True,
            artifact=[f"{FIXTURE.ARCHITECTURE}:archive:{source / archive}", f"{other}:binary:{source / 'other-binary'}"]))
        FIXTURE.FIXTURE.FIXTURE.SignatureTests.command(["--local-user", self.signer + "!", "--digest-algo", "SHA256",
            "--output", str(source / "manifest.sig"), "--detach-sign", str(source / "manifest.json")])
        result = TOOL.instance.prepare(argparse.Namespace(manifest=str(source / "manifest.json"),
            signature=str(source / "manifest.sig"), public_key=str(source / "signing-public-key.asc"),
            trusted_signing_fingerprint=self.signer, architecture=FIXTURE.ARCHITECTURE, schema_current=60,
            artifacts_dir=str(source), staging_parent=str(self.root / "stages"), instance="trust", release_archive=archive))
        self.next_prepared = result["preparation_directory"]
        self.host.target_schema = 61
        self.args = argparse.Namespace(instance="trust", operation="a" * 32, previous=str(self.prepared),
            prepared=self.next_prepared, config_sha256=TOOL.checksum(self.config.encode()), schema_current=60,
            trusted_signing_fingerprint=self.signer)

    @contextmanager
    def operator(self, fault=lambda _: None):
        tool = TOOL.Upgrade(self.layout, self.host, fault)
        try:
            yield tool
        finally:
            tool.close()

    def begin(self, fault=lambda _: None):
        with self.operator(fault) as tool:
            return tool.begin(self.args)

    def recover(self, action="finish", fault=lambda _: None):
        with self.operator(fault) as tool:
            tool.open("trust", self.args.operation)
            tool.load(self.signer)
            return tool.run(action)

    def interrupt(self, point):
        def fault(observed):
            if point == observed:
                raise FIXTURE.Interrupted(point)
        with self.assertRaises(FIXTURE.Interrupted):
            self.begin(fault)

    def test_upgrade_preserves_config_dropins_and_retains_previous_release(self):
        config = self.layout.config / "trust/memory-layer.toml"
        before = config.read_bytes()
        result = self.begin()
        self.assertEqual(result["status"], "complete")
        self.assertFalse(result["rollback_permitted"])
        self.assertEqual(config.read_bytes(), before)
        self.assertEqual(self.host.bindings, {"/etc/systemd/system/memory-layer@trust.service.d/ca.conf": "a" * 64})
        old = self.layout.release / "trust" / ("release-upgrade-" + self.args.operation)
        self.assertEqual((old / "web/index.html").read_bytes(), self.files["web/index.html"])
        self.assertTrue(all(call[1] == "trust" for call in self.host.calls))
        self.assertEqual(self.host.schema, 61)
        calls = list(self.host.calls)
        self.assertEqual(self.recover()["status"], "complete")
        self.assertEqual(len([call for call in self.host.calls if call[0] == "start"]),
                         len([call for call in calls if call[0] == "start"]))

    def test_crash_recovery_before_after_atomic_exchange_and_start(self):
        # Each iteration has an independent synthetic installed instance.
        for point in ("journal_planned", "copy_file", "staged", "journal_stopped", "journal_switch_intent",
                      "exchanged", "digest_replaced", "journal_switched", "journal_start_intent", "started"):
            with self.subTest(point=point):
                original_layout = self.layout
                self.layout = TOOL.console.Layout(self.root / ("recovery-" + point))
                shutil.copytree(original_layout.root, self.layout.root)
                self.host = FakeHost()
                self.host.target_schema = 61
                self.interrupt(point)
                self.assertEqual(self.recover()["status"], "complete")
                self.assertEqual(self.host.schema, 61)
                self.layout = original_layout

    def test_pre_start_rollback_restores_binary_web_and_digest_stopped(self):
        before = (self.layout.config / "trust/memory.sha256").read_bytes()
        self.interrupt("digest_replaced")
        self.assertEqual(self.recover("rollback")["status"], "rolled_back_stopped")
        self.assertEqual((self.layout.config / "trust/memory.sha256").read_bytes(), before)
        self.assertEqual((self.layout.release / "trust/release/web/index.html").read_bytes(), self.files["web/index.html"])
        self.assertFalse(self.host.active)
        self.assertEqual(self.recover("rollback")["status"], "rolled_back_stopped")
        with self.assertRaises(TOOL.release.ReleaseError):
            self.recover()

    def test_rollback_recovers_process_loss_after_reverse_exchange(self):
        self.interrupt("journal_switched")
        def fault(point):
            if point == "exchanged":
                raise FIXTURE.Interrupted(point)
        with self.assertRaises(FIXTURE.Interrupted):
            self.recover("rollback", fault)
        self.assertEqual(self.recover("rollback")["status"], "rolled_back_stopped")
        self.assertEqual((self.layout.release / "trust/release/web/index.html").read_bytes(), self.files["web/index.html"])

    def test_changed_schema_blocks_pre_start_rollback(self):
        self.interrupt("journal_switched")
        self.host.schema = 61
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "Schema changed"):
            self.recover("rollback")

    def test_tampered_journal_and_installed_tree_fail_closed(self):
        self.interrupt("journal_switched")
        web = self.layout.release / "trust/release/web/index.html"
        web.write_bytes(b"unexpected bytes")
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "Candidate bytes changed"):
            self.recover()
        web.write_bytes(b"<p>new fixture</p>")
        path = self.layout.root / "var/lib/memory-layer-upgrades/trust" / self.args.operation / "event-0000.json"
        event = TOOL.console.canonical_read(path)
        event["previous"] = "b" * 64
        path.write_bytes(TOOL.release.canonical(event))
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "Changed upgrade journal"):
            self.recover()

    def test_after_start_intent_never_rollback_even_if_schema_unchanged(self):
        self.interrupt("journal_start_intent")
        self.assertEqual(self.host.schema, 60)
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "recover forward"):
            self.recover("rollback")
        self.assertEqual(self.recover()["status"], "complete")

    def test_health_failure_keeps_candidate_and_can_recover_forward(self):
        self.host.healthy = False
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "health"):
            self.begin()
        with self.assertRaises(TOOL.release.ReleaseError):
            self.recover("rollback")
        self.host.healthy = True
        self.assertEqual(self.recover()["status"], "complete")

    def test_changed_config_or_sandbox_blocks_recovery(self):
        self.interrupt("journal_switched")
        self.host.bindings["/etc/systemd/system/memory-layer@trust.service.d/ca.conf"] = "b" * 64
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "drop-in"):
            self.recover()
        self.host.bindings["/etc/systemd/system/memory-layer@trust.service.d/ca.conf"] = "a" * 64
        path = self.layout.config / "trust/memory-layer.toml"
        path.write_bytes(path.read_bytes() + b"\n# changed")
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "configuration changed"):
            self.recover()

    def test_other_database_sessions_block_switch(self):
        self.host.peers = 1
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "sessions"):
            self.begin()
        self.assertEqual((self.layout.release / "trust/release/web/index.html").read_bytes(), self.files["web/index.html"])
        self.host.peers = 0
        self.assertEqual(self.recover()["status"], "complete")

    def test_signature_tree_and_version_rejection_precede_service_changes(self):
        self.args.prepared = str(self.prepared)
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "strictly newer"):
            self.begin()
        self.assertEqual(self.host.calls, [])

    def test_competing_upgrade_operator_cannot_lock_same_instance(self):
        self.interrupt("journal_planned")
        with self.operator() as first:
            first.open("trust", self.args.operation)
            with self.operator() as second, self.assertRaises(BlockingIOError):
                second.open("trust", self.args.operation)

    def test_pending_operation_blocks_new_operation(self):
        self.interrupt("journal_planned")
        self.args.operation = "b" * 32
        with self.assertRaisesRegex(TOOL.release.ReleaseError, "unfinished operation"):
            self.begin()

    def test_inspect_reports_journal_without_mutating_service(self):
        self.interrupt("journal_planned")
        calls = list(self.host.calls)
        result = self.recover("inspect")
        self.assertEqual(result["status"], "recorded_planned")
        self.assertEqual(self.host.calls, calls)


if __name__ == "__main__":
    unittest.main()
