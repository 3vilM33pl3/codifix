import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

DESKTOP = Path(__file__).parents[1]
SCRIPT = DESKTOP / "scripts/build-desktop.py"


class DesktopBuildWrapperTests(unittest.TestCase):
    def test_relative_custom_target_with_spaces_is_shared_and_build_args_are_forwarded(self):
        with tempfile.TemporaryDirectory(prefix="codifix-desktop-wrapper-") as directory:
            root = Path(directory)
            target = root / "custom target"
            binary_dir = root / "bin with spaces"
            binary_dir.mkdir()
            args_file = root / "tauri-args.json"
            fake = binary_dir / "tauri"
            fake.write_text(f'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
target=pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / "x86_64-unknown-linux-gnu"
assert target.is_absolute()
path=target / "release/bundle/deb"
path.mkdir(parents=True)
tree=path / "package"
(tree / "DEBIAN").mkdir(parents=True)
(tree / "usr/bin").mkdir(parents=True)
(tree / "DEBIAN/control").write_text("Package: codifix-desktop\\nVersion: 0.2.0-rc.1\\nArchitecture: amd64\\nMaintainer: Fixture <fixture@example.invalid>\\nDescription: fixture\\n")
(tree / "usr/bin/codifix-desktop").write_text("#!/bin/sh\\nexit 0\\n")
(tree / "usr/bin/codifix-desktop").chmod(0o755)
subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(path / "Codifix Desktop_0.2.0-rc.1_amd64.deb")], check=True, stdout=subprocess.DEVNULL)
pathlib.Path({str(args_file)!r}).write_text(json.dumps(sys.argv[1:]))
''')
            fake.chmod(0o755)
            relative_target = os.path.relpath(target, DESKTOP)
            environment = dict(os.environ, CARGO_TARGET_DIR=relative_target,
                               PATH=f"{binary_dir}:{os.environ.get('PATH', '')}")
            result = subprocess.run([sys.executable, str(SCRIPT), "--target", "x86_64-unknown-linux-gnu"],
                                    cwd=DESKTOP, env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(args_file.read_text()),
                             ["build", "--bundles", "deb", "--target", "x86_64-unknown-linux-gnu"])
            deb_dir = target / "x86_64-unknown-linux-gnu/release/bundle/deb"
            self.assertTrue((deb_dir / "codifix-desktop_0.2.0~rc.1_amd64.deb").is_file())
            self.assertFalse((deb_dir / "Codifix Desktop_0.2.0-rc.1_amd64.deb").exists())


if __name__ == "__main__":
    unittest.main()
