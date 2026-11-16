import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1] / "scripts/normalize-deb-version.py"
SPEC = importlib.util.spec_from_file_location("normalize_deb_version", SCRIPT)
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


class DebianVersionNormalizationTests(unittest.TestCase):
    def package(self, directory, version="0.2.0-rc.1"):
        root = Path(directory)
        tree = root / "tree"
        (tree / "DEBIAN").mkdir(parents=True)
        (tree / "usr/share/codifix-desktop").mkdir(parents=True)
        (tree / "usr/bin").mkdir()
        (tree / "DEBIAN/control").write_text(
            f"Package: codifix-desktop\nVersion: {version}\nArchitecture: amd64\n"
            "Maintainer: Test <test@example.invalid>\nDescription: synthetic desktop package\n")
        payload = tree / "usr/share/codifix-desktop/payload"
        payload.write_text("preserve payload\n")
        executable = tree / "usr/bin/codifix-desktop"
        executable.write_text("#!/bin/sh\nexit 0\n")
        payload.chmod(0o640)
        executable.chmod(0o755)
        package = root / f"Codifix Desktop_{version}_amd64.deb"
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(package)],
                       check=True, capture_output=True)
        return package

    def test_repackages_control_with_debian_prerelease_order_and_preserves_payload(self):
        with tempfile.TemporaryDirectory() as directory:
            source = self.package(directory)
            result = TOOL.normalize(source)
            self.assertEqual(result.name, "codifix-desktop_0.2.0~rc.1_amd64.deb")
            self.assertFalse(source.exists())
            self.assertEqual(TOOL.fields(result)["Version"], "0.2.0~rc.1")
            extracted = Path(directory) / "extracted"
            subprocess.run(["dpkg-deb", "--extract", str(result), str(extracted)], check=True)
            payload = extracted / "usr/share/codifix-desktop/payload"
            executable = extracted / "usr/bin/codifix-desktop"
            self.assertEqual(payload.read_text(), "preserve payload\n")
            self.assertEqual(payload.stat().st_mode & 0o777, 0o640)
            self.assertEqual(executable.stat().st_mode & 0o777, 0o755)
            subprocess.run(["dpkg", "--compare-versions", "0.2.0-rc.1", "gt", "0.2.0"], check=True)
            subprocess.run(["dpkg", "--compare-versions", "0.2.0~rc.1", "gt", "0.1.4"], check=True)
            subprocess.run(["dpkg", "--compare-versions", "0.2.0~rc.1", "lt", "0.2.0"], check=True)

    def test_rejects_unexpected_input_and_existing_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            wrong = self.package(directory, "0.2.0")
            with self.assertRaises(ValueError):
                TOOL.normalize(wrong)
        with tempfile.TemporaryDirectory() as directory:
            source = self.package(directory)
            destination = Path(directory) / "codifix-desktop_0.2.0~rc.1_amd64.deb"
            destination.write_text("preserve")
            with self.assertRaises(FileExistsError):
                TOOL.normalize(source)
            self.assertTrue(source.exists())
            self.assertEqual(destination.read_text(), "preserve")

    def test_discovers_artifact_under_cargo_custom_target_directory_with_spaces(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "custom target directory"
            package_dir = target / "release/bundle/deb"
            package_dir.mkdir(parents=True)
            raw = self.package(package_dir)
            environment = dict(os.environ, CARGO_TARGET_DIR=str(target))
            result = subprocess.run([sys.executable, str(SCRIPT)], cwd=SCRIPT.parents[1], env=environment,
                                    check=True, capture_output=True, text=True)
            normalized = Path(result.stdout.strip())
            self.assertEqual(normalized, package_dir / "codifix-desktop_0.2.0~rc.1_amd64.deb")
            self.assertFalse(raw.exists())

    def test_requires_exactly_one_raw_package_in_cargo_target_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "target"
            package_dir = target / "release/bundle/deb"
            package_dir.mkdir(parents=True)
            self.package(package_dir)
            second = package_dir / "Codifix Desktop_0.2.0-rc.1_arm64.deb"
            second.write_bytes((package_dir / "Codifix Desktop_0.2.0-rc.1_amd64.deb").read_bytes())
            environment = dict(os.environ, CARGO_TARGET_DIR=str(target))
            result = subprocess.run([sys.executable, str(SCRIPT)], cwd=SCRIPT.parents[1], env=environment,
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("expected exactly one raw Tauri Debian package", result.stderr)


if __name__ == "__main__":
    unittest.main()
