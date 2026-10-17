"""Release orchestration tests with a temporary Git repo and fake Docker only."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("release.sh")
IMAGE = "sha256:" + "a" * 64


class ReleaseBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.work = self.root / "work"
        self.work.mkdir(mode=0o700)
        helper = self.repo / "packaging/ca-bookworm/build.sh"
        helper.parent.mkdir(parents=True)
        (self.repo / "Cargo.toml").write_text('[workspace.package]\nversion = "3.0.0"\n')
        helper.write_text('#!/bin/bash\nprintf fixture > "$2/output/codifix_3.0.0_$4.deb"\n')
        self.git("init", "-q")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "add", ".")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")
        self.revision = self.git("rev-parse", "HEAD").stdout.strip()
        binary = self.root / "bin"
        binary.mkdir()
        docker = binary / "docker"
        docker.write_text("#!/usr/bin/env python3\nimport json,sys\n"
                          f"if sys.argv[1:3] == ['image','inspect']: print('{IMAGE}')\n"
                          "else: print(json.dumps(sys.argv[1:]))\n")
        docker.chmod(0o700)
        self.env = dict(os.environ, PATH=f"{binary}:/usr/bin:/bin")

    def git(self, *arguments):
        return subprocess.run(["git", *arguments], cwd=self.repo, capture_output=True,
                              text=True, check=True)

    def build(self, revision=None, image=IMAGE, architecture="amd64"):
        return subprocess.run(["bash", str(SCRIPT), revision or self.revision,
                               str(self.work), image, architecture], cwd=self.repo,
                              env=self.env, capture_output=True, text=True, timeout=20)

    def test_frozen_source_separate_fetch_offline_validation_and_artifact_hashes(self):
        result = self.build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.work / "source.tar").is_file())
        metadata = json.loads((self.work / "output/build-amd64.json").read_text())
        self.assertEqual(metadata["source_commit"], self.revision)
        self.assertEqual(metadata["application_version"], "3.0.0")
        self.assertEqual(metadata["builder_image"], IMAGE)
        self.assertFalse(metadata["installed_on_host"])
        package = self.work / "output/codifix_3.0.0_amd64.deb"
        self.assertEqual(metadata["artifacts"][package.name], hashlib.sha256(package.read_bytes()).hexdigest())
        preparation = json.loads((self.work / "dependency-preparation.log").read_text())
        self.assertNotIn("--network", preparation)
        self.assertIn("cargo fetch --locked", preparation[-1])
        validation = json.loads((self.work / "output/validation-amd64.log").read_text())
        self.assertEqual(validation[validation.index("--network") + 1], "none")
        self.assertIn("/candidate:rw,nosuid,nodev,exec,size=1g", validation)
        self.assertIn("/tmp:rw,nosuid,nodev,size=2g", validation)
        self.assertNotIn("/candidate:rw,nosuid,nodev,exec,size=1g", preparation)
        self.assertIn("--read-only", validation)
        self.assertNotIn("--privileged", validation)
        self.assertIn("${binaries[@]:0:3}", validation[-3])
        self.assertIn("expected private gateway configuration file", validation[-3])
        self.assertIn('timeout 15s "${runner[@]}" "${binaries[3]}"', validation[-3])
        self.assertNotEqual(self.build().returncode, 0, "must never overwrite an earlier release build")

    def test_rejects_mutable_inputs_and_shared_work(self):
        for values in ({"revision": "main"}, {"image": "builder:latest"}, {"architecture": "riscv64"}):
            with self.subTest(values=values):
                self.assertNotEqual(self.build(**values).returncode, 0)
        self.work.chmod(0o755)
        self.assertNotEqual(self.build().returncode, 0)

    def test_gateway_probe_accepts_only_exact_missing_argument_error(self):
        # Execute the actual bounded probe used by the release harness. Loader
        # failure, timeout, malformed output and unexpected success must reject.
        probe = "gateway_status=0\n" + SCRIPT.read_text().split("  gateway_status=0\n", 1)[1].split("  test -s", 1)[0]
        fake = self.root / "fake-gateway"
        fake.write_text('#!/bin/sh\nprintf "%s\\n" "$FAKE_OUTPUT"\nexit "$FAKE_STATUS"\n')
        fake.chmod(0o700)
        expected = 'Error: "expected private gateway configuration file"'
        for status, output, accepted in ((1, expected, True), (126, expected, False),
                                         (124, expected, False), (0, expected, False),
                                         (1, "configuration unavailable", False)):
            with self.subTest(status=status, output=output):
                environment = dict(os.environ, FAKE_STATUS=str(status), FAKE_OUTPUT=output)
                result = subprocess.run(["bash", "-euc", 'runner=(); binaries=(unused unused unused "$1");\n' + probe,
                                         "bash", str(fake)], env=environment, capture_output=True, timeout=20)
                self.assertEqual(result.returncode == 0, accepted)


if __name__ == "__main__":
    unittest.main()
