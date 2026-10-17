#!/usr/bin/env python3
"""Run the installer against fake platform/download/install commands."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().with_name('install.sh')


class ReleaseSelection(unittest.TestCase):
    def run_installer(self, asset):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = {
                'uname': 'case "$1" in -s) echo Linux;; -m) echo x86_64;; esac',
                'dpkg': 'exit 99',
                'pg_isready': 'exit 1',
                'curl': 'case "$*" in *releases/latest*) printf \'%s\\n\' "$INSTALL_TEST_ASSET";; *) printf \'download\\n\' >> "$INSTALL_TEST_EFFECTS";; esac',
                'sudo': 'printf \'install\\n\' >> "$INSTALL_TEST_EFFECTS"',
            }
            for name, body in commands.items():
                path = root / name
                path.write_text('#!/bin/sh\n' + body + '\n')
                path.chmod(0o700)
            effects = root / 'effects'
            result = subprocess.run(['/bin/sh', str(INSTALLER)], env={
                **os.environ, 'PATH': f'{root}:/usr/bin:/bin',
                'INSTALL_TEST_ASSET': '{"browser_download_url":"https://github.com/3vilM33pl3/codifix/releases/download/v3.0.0/' + asset + '"}',
                'INSTALL_TEST_EFFECTS': str(effects),
            }, capture_output=True, text=True, timeout=10)
            return result, effects.read_text() if effects.exists() else ''

    def test_preserved_legacy_release_never_downloads_or_installs(self):
        result, effects = self.run_installer('memory-layer_2.1.0_amd64.deb')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('no Codifix .deb', result.stderr)
        self.assertEqual(effects, '')

    def test_other_architecture_never_downloads_or_installs(self):
        result, effects = self.run_installer('codifix_3.0.0_arm64.deb')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(effects, '')

    def test_codifix_architecture_asset_is_selected(self):
        result, effects = self.run_installer('codifix_3.0.0_amd64.deb')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(effects, 'download\ninstall\n')


if __name__ == '__main__':
    unittest.main()
