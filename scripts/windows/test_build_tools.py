# SPDX-License-Identifier: AGPL-3.0-or-later
import json
from pathlib import Path
import unittest
from urllib.parse import urlsplit


class BuildToolsManifestTests(unittest.TestCase):
    def test_native_postgres_inputs_and_lab_boundary(self):
        root = Path(__file__).parent
        manifest = json.loads((root / 'native-postgres.json').read_text())
        self.assertEqual(manifest['format'], 'memory-windows-postgres-proof-v1')
        self.assertEqual(manifest['postgres_version'], '17.11')
        self.assertEqual(manifest['pgvector_version'], '0.8.7')
        for kind, host in [('postgres', 'get.enterprisedb.com'), ('pgvector', 'codeload.github.com')]:
            artifact = manifest[kind]
            url = urlsplit(artifact['url'])
            self.assertEqual(url.scheme, 'https')
            self.assertEqual(url.hostname, host)
            self.assertIsNone(url.username)
            self.assertIsNone(url.password)
            self.assertFalse(url.query)
            self.assertEqual(Path(artifact['file']).name, artifact['file'])
            self.assertEqual(len(artifact['sha256']), 64)
            self.assertTrue(all(c in '0123456789abcdef' for c in artifact['sha256']))
        script = (root / 'guest-native-postgres.ps1').read_text()
        self.assertIn("$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", script)
        self.assertIn("-h 127.0.0.1 -p 55471", script)
        self.assertIn("--auth-host=scram-sha-256", script)
        self.assertIn("Prepared native runtime does not match", script)
        self.assertNotIn('--trust', script)

    def test_exact_tool_set_has_bounded_verified_inputs(self):
        manifest = json.loads((Path(__file__).parent / 'build-tools.json').read_text())
        self.assertEqual(manifest['format'], 'memory-windows-build-tools-v1')
        self.assertEqual(manifest['rust_toolchain'], '1.99.1')
        self.assertEqual(manifest['wix_version'], '4.0.5')
        artifacts = manifest['artifacts']
        self.assertEqual(sorted(a['id'] for a in artifacts),
                         ['dotnet', 'git', 'msvc', 'node', 'rustup'])
        for artifact in artifacts:
            location = urlsplit(artifact['url'])
            self.assertEqual(location.scheme, 'https')
            self.assertIsNone(location.username)
            self.assertIsNone(location.password)
            self.assertFalse(location.query)
            self.assertEqual(Path(artifact['file']).name, artifact['file'])
            size = {'SHA256': 64, 'SHA512': 128}[artifact['algorithm']]
            self.assertEqual(len(artifact['hash']), size)
            self.assertTrue(all(c in '0123456789abcdef' for c in artifact['hash']))
        self.assertTrue(next(a for a in artifacts if a['id'] == 'msvc')['microsoft_signature'])


if __name__ == '__main__':
    unittest.main()
