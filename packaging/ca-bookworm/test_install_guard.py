"""Fail-closed hosted installer guards; never execute dpkg or install anything."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("install-test.sh")


class InstallerGuardTests(unittest.TestCase):
    def test_non_hosted_or_unapproved_execution_refused_before_reading_package(self):
        with tempfile.TemporaryDirectory() as directory:
            package = Path(directory) / "candidate.deb"
            package.write_bytes(b"not a package; guard must reject before dpkg")
            environment = dict(os.environ, GITHUB_ACTIONS="false", RUNNER_OS="Linux",
                               RUNNER_ENVIRONMENT="self-hosted", CODIFIX_HOSTED_INSTALL_TEST="no")
            for arguments in ([], [str(package)], [str(package), "unexpected"]):
                with self.subTest(arguments=arguments):
                    result = subprocess.run(["bash", str(SCRIPT), *arguments], env=environment,
                                            capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertIn("Requires an explicitly disposable", result.stderr)
            self.assertEqual(package.read_bytes(), b"not a package; guard must reject before dpkg")


if __name__ == "__main__":
    unittest.main()
