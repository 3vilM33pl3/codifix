"""Command-construction tests; never invoke the Docker daemon or a build."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("build.sh")
IMAGE = "sha256:" + "a" * 64


class BuildCommands(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.work = self.root / "work"
        self.work.mkdir(mode=0o700)
        (self.work / "memory").mkdir()
        (self.work / "memory" / "Cargo.lock").touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        docker = self.bin / "docker"
        docker.write_text("#!/usr/bin/env python3\nimport json,sys\n"
                          f"if sys.argv[1:3] == ['image','inspect']: print('{IMAGE}')\n"
                          "else: print(json.dumps(sys.argv[1:]))\n")
        docker.chmod(0o700)
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin")

    def tearDown(self):
        self.tmp.cleanup()

    def run_build(self, phase, work=None, image=IMAGE, extra=()):
        return subprocess.run(["bash", str(SCRIPT), phase, str(work or self.work), image, *extra],
                              env=self.env, text=True, capture_output=True, timeout=10)

    def test_memory_is_offline_unprivileged_and_resource_bounded(self):
        result = self.run_build("memory")
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout.splitlines()[-1])
        for flag, value in [("--network", "none"), ("--cap-drop", "ALL"),
                            ("--security-opt", "no-new-privileges"), ("--cpus", "4"),
                            ("--memory", "14g"), ("--pids-limit", "512"), ("--pull", "never")]:
            self.assertEqual(args[args.index(flag) + 1], value)
        self.assertIn("--read-only", args)
        self.assertIn("--frozen", args)
        self.assertIn("aarch64-unknown-linux-gnu", args)
        self.assertNotIn("--privileged", args)
        self.assertNotIn("--env-file", args)

    def test_mutable_image_and_unknown_phase_fail(self):
        self.assertNotEqual(self.run_build("memory", image="rust:latest").returncode, 0)
        self.assertNotEqual(self.run_build("activate").returncode, 0)

    def test_explicit_architectures_match_build_and_package(self):
        for architecture, target in (("amd64", "x86_64-unknown-linux-gnu"),
                                     ("arm64", "aarch64-unknown-linux-gnu")):
            with self.subTest(architecture=architecture):
                result = self.run_build("memory", extra=(architecture,))
                self.assertEqual(result.returncode, 0, result.stderr)
                args = json.loads(result.stdout.splitlines()[-1])
                self.assertEqual(args[args.index("--target") + 1], target)
                result = self.run_build("package", extra=(architecture,))
                self.assertEqual(result.returncode, 0, result.stderr)
                args = json.loads(result.stdout.splitlines()[-1])
                self.assertEqual(args[args.index("--arch") + 1], architecture)

    def test_architecture_never_becomes_an_arbitrary_argument(self):
        for phase in ("memory", "package"):
            for extra in (("riscv64",), ("--privileged",), ("amd64", "arm64")):
                self.assertNotEqual(self.run_build(phase, extra=extra).returncode, 0)
        self.assertNotEqual(self.run_build("web", extra=("amd64",)).returncode, 0)

    def test_package_keeps_arm64_default(self):
        result = self.run_build("package")
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout.splitlines()[-1])
        self.assertEqual(args[args.index("--arch") + 1], "arm64")

    def test_shared_directory_symlink_and_mount_separator_fail(self):
        self.work.chmod(0o755)
        self.assertNotEqual(self.run_build("memory").returncode, 0)
        self.work.chmod(0o700)
        link = self.root / "link"
        link.symlink_to(self.work)
        self.assertNotEqual(self.run_build("memory", work=link).returncode, 0)
        unsafe = self.root / "work,dst=elsewhere"
        unsafe.mkdir(mode=0o700)
        self.assertNotEqual(self.run_build("memory", work=unsafe).returncode, 0)

    def test_pi_requires_explicit_toolchain(self):
        self.assertNotEqual(self.run_build("pi").returncode, 0)
        toolchain = self.root / "toolchain"
        (toolchain / "bin").mkdir(parents=True)
        for name in ["cargo", "rustc"]:
            path = toolchain / "bin" / name
            path.touch(mode=0o700)
        result = self.run_build("pi", extra=(str(toolchain),))
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout.splitlines()[-1])
        self.assertIn(f"type=bind,src={toolchain},dst=/pi-toolchain,readonly", args)
        self.assertIn("/pi-toolchain/bin/cargo", args)


if __name__ == "__main__":
    unittest.main()
