import hashlib
import importlib.util
import os
from pathlib import Path
import socket
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import Mock

path = Path(__file__).with_name("tui-supervisor.py").resolve()
spec = importlib.util.spec_from_file_location("tui_supervisor", path)
supervisor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(supervisor)


class TuiSupervisorTest(unittest.TestCase):
    def test_reads_exact_bytes_and_rejects_wrong_hash_or_symlink(self):
        with TemporaryDirectory() as home:
            binary = Path(home) / "candidate"
            data = b"candidate binary"
            binary.write_bytes(data)
            digest = hashlib.sha256(data).hexdigest()
            self.assertEqual(supervisor.read_artifact(binary, digest, os.getuid()), data)
            with self.assertRaises(ValueError):
                supervisor.read_artifact(binary, "0" * 64, os.getuid())
            alias = Path(home) / "alias"
            alias.symlink_to(binary)
            with self.assertRaises(OSError):
                supervisor.read_artifact(alias, digest, os.getuid())

    def test_requires_one_admitted_release_effect(self):
        release = {"sha256":"1" * 64, "version":"3.1.0-rc.3"}
        db = Mock()
        for result in ([], [release, release]):
            db.rows.return_value = result
            with self.assertRaises(ValueError):
                supervisor.release_for_effect(db, "instance", "mandate", "specialist")
        db.rows.return_value = [release]
        self.assertEqual(supervisor.release_for_effect(db, "instance", "mandate", "specialist"), release)
        sql = db.rows.call_args.args[0]
        self.assertIn("e.status='pending'", sql)
        self.assertIn("t.execution_lease_until > clock_timestamp()", sql)
        self.assertIn("t.requester='homelab'", sql)

    @unittest.skipUnless(os.geteuid() == 0, "requires disposable Linux root to change worker UID")
    def test_broker_serves_the_worker_child_after_dropping_root(self):
        identity = {"user":65534, "group":65534, "extra_groups":[]}
        with TemporaryDirectory() as directory:
            work = Path(directory)
            os.chown(work, 0, identity["group"])
            work.chmod(0o750)
            address = work / "broker.sock"
            callback = work / "tui-supervisor.py"
            callback.write_bytes(path.read_bytes())
            with socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET) as listener:
                listener.bind(str(address))
                os.chown(address, 0, identity["group"])
                address.chmod(0o660)
                listener.listen()
                listener.settimeout(0.2)
                # This connector has the right Unix identity and the wrong parent.
                # It cannot use the broker even with access to the socket.
                stranger = subprocess.Popen([sys.executable, str(callback), str(address), "install"],
                                            stderr=subprocess.DEVNULL, **identity)
                command = (
                    "import os, subprocess, sys;"
                    "assert os.getuid() == 65534;"
                    "base = [sys.executable, sys.argv[1], sys.argv[2]];"
                    "subprocess.run(base + ['install'], check=True);"
                    "subprocess.run(base + ['verify'], check=True)"
                )
                child = subprocess.Popen([sys.executable, "-c", command, str(callback), str(address)], **identity)
                steps = []
                def install():
                    steps.append("install")
                    return "release"
                def verify(release):
                    self.assertEqual(release, "release")
                    steps.append("verify")
                try:
                    supervisor.broker(listener, child, identity, install, verify)
                    self.assertEqual(child.wait(timeout=10), 0)
                    self.assertNotEqual(stranger.wait(timeout=10), 0)
                    self.assertEqual(steps, ["install", "verify"])
                finally:
                    for process in (child, stranger):
                        if process.poll() is None:
                            process.kill()
                            process.wait()


if __name__ == "__main__":
    unittest.main()
