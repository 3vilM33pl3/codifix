#!/usr/bin/env python3
"""Offline macOS packaging contracts. No native package, service or provider calls."""
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[3]
BUILDER = REPO / "packaging/build-pkg.sh"
MACOS = REPO / "packaging/macos"


class PackageContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="memory-pkg-contract-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.env = dict(os.environ, PATH=f"{self.tools}:/usr/bin:/bin",
                        SUDO_USER="", TMPDIR=str(self.root / "temporary"))
        Path(self.env["TMPDIR"]).mkdir()

    def script(self, name, body):
        path = self.tools / name
        path.write_text("#!/bin/bash\nset -eu\n" + body)
        path.chmod(0o755)
        return path

    def run_script(self, path, *args):
        return subprocess.run(["/bin/bash", str(path), *args], env=self.env,
                              capture_output=True, text=True, timeout=15)

    def fixture(self, *, darwin=True, pkgbuild=True):
        repo = self.root / "repo"
        for directory in ("packaging/macos", ".codex/agents", ".agents/skills",
                          "web/dist", "target/macos-pkg", "target/x86_64-apple-darwin/release"):
            (repo / directory).mkdir(parents=True, exist_ok=True)
        shutil.copy2(BUILDER, repo / "packaging/build-pkg.sh")
        for name in ("postinstall.sh", "seed-user.sh", "onboard.py", "onboarding_files.py",
                     "onboarding_native.py", "onboarding_journey.py", "onboarding_agents.py", "operator-README.md"):
            shutil.copy2(MACOS / name, repo / "packaging/macos" / name)
        for name in ("config.toml", "agents/astra_adviser.toml"):
            shutil.copy2(REPO / ".codex" / name, repo / ".codex" / name)
        (repo / "Cargo.toml").write_text('version = "0.0.0-test"\n')
        (repo / "README.md").write_text("Fixture only\n")
        (repo / "memory-layer.toml.example").write_text("# fixture config\n")
        (repo / "web/dist/index.html").write_text("fixture\n")
        (repo / "target/macos-pkg/keep").write_text("existing workspace staging\n")
        binary = repo / "target/x86_64-apple-darwin/release/memory"
        binary.write_text('#!/bin/bash\n[[ "$1" == completion ]] || exit 99\necho fixture-completion\n')
        binary.chmod(0o755)
        trace = self.root / "build-trace"
        self.env.update(TRACE=str(trace), CAPTURE=str(self.root / "capture"),
                        CARGO_TARGET_DIR=str(repo / "target"))
        self.script("uname", f'if [[ "$1" == -s ]]; then echo {"Darwin" if darwin else "Linux"}; else echo x86_64; fi\n')
        self.script("cargo", 'printf "%s\\n" "$@" >> "$TRACE"\n')
        self.script("npm", 'printf "npm %s\\n" "$*" >> "$TRACE"\n')
        if pkgbuild:
            self.script("pkgbuild", '''payload=""; scripts=""; output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) payload="$2"; shift 2 ;;
    --scripts) scripts="$2"; shift 2 ;;
    --*) shift 2 ;;
    *) output="$1"; shift ;;
  esac
done
mkdir -p "$CAPTURE"
cp -R "$payload" "$CAPTURE/payload"
cp -R "$scripts" "$CAPTURE/scripts"
touch "$output"
''')
        return repo, trace

    def assert_no_build(self, repo, trace, *args):
        result = self.run_script(repo / "packaging/build-pkg.sh", *args)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(trace.exists(), "Build must not start")
        self.assertEqual(list(Path(self.env["TMPDIR"]).iterdir()), [])
        self.assertEqual((repo / "target/macos-pkg/keep").read_text(), "existing workspace staging\n")
        return result

    def test_platform_preflight_does_not_build_or_clear_staging(self):
        repo, trace = self.fixture(darwin=False)
        result = self.assert_no_build(repo, trace)
        self.assertIn("requires Darwin", result.stderr)

    def test_target_mismatch_fails_before_build(self):
        repo, trace = self.fixture()
        result = self.assert_no_build(repo, trace, "--arch", "x86_64", "--target", "aarch64-apple-darwin")
        self.assertIn("requires --target x86_64-apple-darwin", result.stderr)

    def test_missing_tool_fails_before_staging(self):
        repo, trace = self.fixture(pkgbuild=False)
        result = self.assert_no_build(repo, trace)
        self.assertIn("tool unavailable: pkgbuild", result.stderr)

    def test_missing_flag_value_fails_before_build(self):
        repo, trace = self.fixture()
        result = self.assert_no_build(repo, trace, "--target")
        self.assertIn("requires a value", result.stderr)

    def test_failed_build_cleans_private_stage_only(self):
        repo, _ = self.fixture()
        self.script("cargo", "exit 7\n")
        result = self.run_script(repo / "packaging/build-pkg.sh")
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        self.assertEqual(list(Path(self.env["TMPDIR"]).iterdir()), [])
        self.assertEqual((repo / "target/macos-pkg/keep").read_text(), "existing workspace staging\n")

    def test_locked_build_exact_read_only_templates_and_private_stage(self):
        repo, trace = self.fixture()
        result = self.run_script(repo / "packaging/build-pkg.sh", "--arch", "x86_64")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("--locked\n", trace.read_text())
        self.assertEqual(list(Path(self.env["TMPDIR"]).iterdir()), [])
        self.assertTrue((repo / "target/macos-pkg/keep").exists())
        capture = Path(self.env["CAPTURE"])
        for name in ("config.toml", "agents/astra_adviser.toml"):
            staged = capture / "payload/usr/local/share/memory-layer/onboarding/.codex" / name
            self.assertEqual(staged.read_bytes(), (REPO / ".codex" / name).read_bytes())
            self.assertEqual(stat.S_IMODE(staged.stat().st_mode), 0o444)
        for name in ("onboard.py", "onboarding_files.py", "onboarding_native.py", "onboarding_journey.py", "onboarding_agents.py", "operator-README.md"):
            staged = capture / "payload/usr/local/share/memory-layer/onboarding/operator" / name
            self.assertEqual(staged.read_bytes(), (MACOS / name).read_bytes())
            self.assertEqual(stat.S_IMODE(staged.stat().st_mode), 0o444)
        self.assertNotIn("python", trace.read_text())
        self.assertFalse((capture / "payload/Users").exists())
        self.assertFalse((capture / "payload/var/root").exists())
        self.assertEqual((capture / "scripts/postinstall").read_bytes(), (MACOS / "postinstall.sh").read_bytes())

    def seed_fixture(self):
        home = self.root / "User With Spaces"
        support = home / "Library/Application Support"
        support.mkdir(parents=True)
        share = self.root / "share"
        share.mkdir()
        (share / "memory-layer.toml.example").write_text("# packaged defaults\n")
        return home, support / "memory-layer", share

    def test_seed_only_missing_files_and_keeps_existing_read_only_metadata(self):
        home, app, share = self.seed_fixture()
        app.mkdir(mode=0o751)
        (app / "log").mkdir(mode=0o711)
        (app / "assets").mkdir()
        config = app / "memory-layer.toml"
        config.write_text("# retained private settings\n")
        config.chmod(0o400)
        existing = [app, app / "log", app / "assets", config]
        before = [(p.stat().st_mode, p.stat().st_uid, p.stat().st_gid) for p in existing]
        result = self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(config.read_text(), "# retained private settings\n")
        self.assertEqual(before, [(p.stat().st_mode, p.stat().st_uid, p.stat().st_gid) for p in existing])
        self.assertTrue((app / "run").is_dir())
        self.assertEqual(stat.S_IMODE((app / "memory-layer.env").stat().st_mode), 0o600)
        (app / "memory-layer.env").write_text("# retained env\n")
        (app / "memory-layer.env").chmod(0o400)
        self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual((app / "memory-layer.env").read_text(), "# retained env\n")
        self.assertEqual(stat.S_IMODE((app / "memory-layer.env").stat().st_mode), 0o400)
        self.assertFalse((home / ".codex").exists())

    def test_seed_preserves_dangling_file_and_relocated_directory(self):
        home, app, share = self.seed_fixture()
        app.mkdir()
        (app / "memory-layer.toml").symlink_to(self.root / "unavailable-config")
        (app / "memory-layer.env").symlink_to(self.root / "unavailable-env")
        result = self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / "unavailable-config").exists())
        self.assertFalse((self.root / "unavailable-env").exists())
        shutil.rmtree(app)
        relocated = self.root / "relocated"
        relocated.mkdir()
        app.symlink_to(relocated, target_is_directory=True)
        result = self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(relocated.iterdir()), [])

    def test_new_seed_is_private_and_does_not_create_general_user_parents(self):
        home, app, share = self.seed_fixture()
        result = self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((app / "memory-layer.toml").read_bytes(), (share / "memory-layer.toml.example").read_bytes())
        self.assertEqual(stat.S_IMODE(app.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((app / "memory-layer.toml").stat().st_mode), 0o600)
        empty_home = self.root / "other-user"
        empty_home.mkdir()
        result = self.run_script(MACOS / "seed-user.sh", str(empty_home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(empty_home.iterdir()), [])

    def test_postinstall_without_user_never_runs_memory_or_sudo(self):
        self.script("stat", "echo root\n")
        self.script("sudo", "exit 99\n")
        result = self.run_script(MACOS / "postinstall.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("not changed", result.stdout)

    def test_postinstall_delegates_seed_as_verified_user_with_spaced_home(self):
        home, _, _ = self.seed_fixture()
        self.env.update(TEST_HOME=str(home), TRACE=str(self.root / "seed-trace"))
        self.script("stat", "echo operator\n")
        self.script("id", "echo 501\n")
        self.script("dscl", 'printf "NFSHomeDirectory: %s\\n" "$TEST_HOME"\n')
        self.script("sudo", 'printf "%s\\n" "$@" > "$TRACE"\nexit 1\n')
        result = self.run_script(MACOS / "postinstall.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        args = Path(self.env["TRACE"]).read_text().splitlines()
        self.assertEqual(args[:4], ["-u", "operator", "--", "/bin/bash"])
        self.assertEqual(args[-2:], [str(home), "/usr/local/share/memory-layer"])
        self.assertIn("existing settings were preserved", result.stdout)
        self.assertIn("no services were started or restarted", result.stdout)

    @unittest.skipIf(os.geteuid() == 0, "Readonly access checks require a non-root test user")
    def test_read_only_legacy_directory_is_untouched(self):
        home, app, share = self.seed_fixture()
        app.mkdir()
        (app / "settings").write_text("legacy assets/settings\n")
        app.chmod(0o500)
        self.addCleanup(app.chmod, 0o700)
        before = (app.stat().st_mode, app.stat().st_uid, app.stat().st_gid)
        result = self.run_script(MACOS / "seed-user.sh", str(home), str(share))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([p.name for p in app.iterdir()], ["settings"])
        self.assertEqual(before, (app.stat().st_mode, app.stat().st_uid, app.stat().st_gid))
        self.assertEqual((app / "settings").read_text(), "legacy assets/settings\n")

    def test_lifecycle_has_no_service_control_or_recursive_ownership(self):
        for name in ("postinstall.sh", "seed-user.sh"):
            text = (MACOS / name).read_text()
            self.assertNotIn("restart-all", text)
            self.assertNotIn("launchctl", text)
            self.assertNotIn("chown", "\n".join(line for line in text.splitlines() if not line.startswith("#")))
            self.assertNotIn("/usr/local/bin/memory", text)
            self.assertNotIn(".codex", text)
            result = subprocess.run(["/bin/bash", "-n", str(MACOS / name)], capture_output=True)
            self.assertEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
