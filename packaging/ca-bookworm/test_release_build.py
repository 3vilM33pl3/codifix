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
        (self.repo / "Cargo.toml").write_text('[workspace.package]\nversion = "2.1.0"\n')
        helper.write_text('#!/bin/bash\nprintf fixture > "$2/output/memory-layer_2.1.0_$4.deb"\n')
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
        self.assertEqual(metadata["application_version"], "2.1.0")
        self.assertEqual(metadata["builder_image"], IMAGE)
        self.assertFalse(metadata["installed_on_host"])
        package = self.work / "output/memory-layer_2.1.0_amd64.deb"
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
        self.assertNotEqual(self.build().returncode, 0, "must never overwrite an earlier release build")

    def test_rejects_mutable_inputs_and_shared_work(self):
        for values in ({"revision": "main"}, {"image": "builder:latest"}, {"architecture": "riscv64"}):
            with self.subTest(values=values):
                self.assertNotEqual(self.build(**values).returncode, 0)
        self.work.chmod(0o755)
        self.assertNotEqual(self.build().returncode, 0)


if __name__ == "__main__":
    unittest.main()
