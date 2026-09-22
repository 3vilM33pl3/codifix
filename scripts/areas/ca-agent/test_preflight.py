"""Offline unit tests; genuine pinned signature verification is a separate rehearsal."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("preflight", Path(__file__).with_name("preflight.py"))
preflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preflight)


class PreflightTests(unittest.TestCase):
    def test_traversal_and_absolute_paths_rejected(self):
        for name in ("../x", "/x", "a/../x", "a//x", "./x", "a\\x", ""):
            with self.subTest(name=name), self.assertRaises(preflight.Blocked):
                preflight.regular_member(Path("/tmp"), name)

    def test_file_and_parent_links_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "file").write_text("test")
            (root / "link").symlink_to(root / "file")
            (root / "parent").symlink_to(root, target_is_directory=True)
            for name in ("link", "parent/file"):
                with self.subTest(name=name), self.assertRaises(preflight.Blocked):
                    preflight.regular_member(root, name)

    def test_regular_file_and_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "file").write_bytes(b"test")
            self.assertEqual(preflight.regular_member(root, "file"), root / "file")
            self.assertEqual(preflight.digest(root / "file"),
                             "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08")

    def test_altered_inventory_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "SHA256SUMS").write_text("substituted")
            with self.assertRaisesRegex(preflight.Blocked, "pinned_release_input_changed"):
                preflight.verify_inventory(root)

    def test_complete_inventory_and_extra_file_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            names = [f"public-{number}" for number in range(55)]
            for name in names:
                (root / name).write_bytes(b"public fixture")
            inventory = root / "SHA256SUMS"
            inventory.write_text("".join(f"{preflight.digest(root / name)}  {name}\n" for name in names))
            with patch.object(preflight, "PINS", {"SHA256SUMS": preflight.digest(inventory)}):
                self.assertEqual(preflight.verify_inventory(root), 55)
                (root / "unexpected").write_text("must not be packaged")
                with self.assertRaisesRegex(preflight.Blocked, "unexpected_release_members"):
                    preflight.verify_inventory(root)

    def test_duplicate_checksum_entries_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "public").write_bytes(b"public fixture")
            inventory = root / "SHA256SUMS"
            inventory.write_text(f"{preflight.digest(root / 'public')}  public\n" * 55)
            with patch.object(preflight, "PINS", {"SHA256SUMS": preflight.digest(inventory)}), \
                    self.assertRaisesRegex(preflight.Blocked, "duplicate_inventory_member"):
                preflight.verify_inventory(root)

    def test_supported_native_hosts(self):
        for machine, architecture in (("aarch64", "arm64"), ("x86_64", "amd64")):
            with self.subTest(machine=machine), \
                    patch.object(preflight.platform, "machine", return_value=machine), \
                    patch.object(preflight.os, "confstr", return_value="glibc 2.36"), \
                    patch.object(preflight.subprocess, "run", return_value=
                                 subprocess.CompletedProcess([], 0, "gpg (GnuPG) 2.2.40", "")) as run:
                result = preflight.host_compatibility()
                self.assertEqual(result["architecture"], architecture)
                self.assertFalse(result["native_memory_execution_tested"])
                self.assertEqual(run.call_args.kwargs["env"], preflight.CLEAN_ENV)

    def test_unsupported_host_fails_without_running_tools(self):
        for machine, libc in (("riscv64", "glibc 2.36"), ("aarch64", "glibc 2.31"),
                              ("aarch64", "musl 1.2")):
            with self.subTest(machine=machine, libc=libc), \
                    patch.object(preflight.platform, "machine", return_value=machine), \
                    patch.object(preflight.os, "confstr", return_value=libc), \
                    patch.object(preflight.subprocess, "run") as run, \
                    self.assertRaises(preflight.Blocked):
                preflight.host_compatibility()
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
