#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Runs only a rebased launcher and fake executable inside temporary storage."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="memory-launcher-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.launcher = self.root / "run-instance"
        source = (REPO / "packaging/debian/run-instance").read_text()
        for base in ("/opt/memory-layer", "/var/lib/memory-layer", "/var/cache/memory-layer", "/run/credentials"):
            source = source.replace(base, str(self.root) + base)
        self.launcher.write_text(source)
        self.launcher.chmod(0o700)
        self.credentials = self.root / "run/credentials/memory-layer@homelab.service"
        self.credentials.mkdir(parents=True, mode=0o700)
        self.config = self.credentials / "memory-layer.toml"
        self.config.write_text("# synthetic; never parsed\n")
        self.executable = self.root / "opt/memory-layer/instances/homelab/release/bin/memory"
        self.executable.parent.mkdir(parents=True)
        self.executable.write_text("#!/usr/bin/python3 -I\nimport os,sys,json\nprint(json.dumps({'args':sys.argv[1:],'env':dict(os.environ)}))\n")
        self.executable.chmod(0o700)
        self.marker = self.root / "forbidden-bash-env-was-executed"
        evil = self.root / "ambient.bash"
        evil.write_text(f"/usr/bin/touch '{self.marker}'\n")
        self.env = {
            "PATH": "/forbidden/ambient/path", "BASH_ENV": str(evil), "ENV": str(evil),
            "OPENAI_API_KEY": "SYNTHETIC_KEY_NOT_FOR_CHILD", "ANTHROPIC_API_KEY": "SYNTHETIC_OTHER_KEY",
            "MEMORY_LAYER_FEDERATION_CONFIG": "/forbidden/ambient-federation.json",
            "MEMORY_LAYER__DATABASE__URL": "postgresql://wrong/area",
            "MEMORY_LAYER__SERVICE__READ_ONLY": "false",
            "MEMORY_LAYER__SUPERVISOR__DETERMINISTIC_CHECKS": "true",
            "MEMORY_LAYER_DEV": "1", "MEMORY_LAYER_PROFILE": "dev",
            "HTTP_PROXY": "http://wrong.invalid", "HTTPS_PROXY": "http://wrong.invalid",
            "PYTHONPATH": "/forbidden/modules", "SSH_AUTH_SOCK": "/forbidden/agent",
            "XDG_CONFIG_HOME": "/forbidden/config", "XDG_STATE_HOME": "/forbidden/state",
        }

    def run_launcher(self, *args):
        return subprocess.run([str(self.launcher), *map(str, args)], env=self.env,
                              cwd=self.root, capture_output=True, text=True, timeout=5, check=False)

    def test_exact_arguments_clean_environment_and_no_shell_startup_code(self):
        result = self.run_launcher("homelab", self.credentials)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.marker.exists())
        output = json.loads(result.stdout)
        self.assertEqual(output["args"], ["--config", str(self.config), "service", "run"])
        child = output["env"]
        self.assertEqual(child["MEMORY_LAYER_PROFILE"], "prod")
        self.assertEqual(child["PATH"], "/usr/bin:/bin")
        self.assertEqual(child["XDG_STATE_HOME"], str(self.root / "var/lib/memory-layer/instances/homelab/service/state"))
        self.assertEqual(set(child) - {"LC_CTYPE"}, {"PATH", "LANG", "MEMORY_LAYER_PROFILE", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"})
        self.assertNotIn("SYNTHETIC_KEY", result.stdout)

    def test_federation_requires_explicit_flag_and_exact_local_credential(self):
        self.assertNotEqual(self.run_launcher("homelab", self.credentials, "--federation").returncode, 0)
        federation = self.credentials / "federation.json"
        federation.write_text("{}")
        disabled = self.run_launcher("homelab", self.credentials)
        self.assertNotIn("MEMORY_LAYER_FEDERATION_CONFIG", json.loads(disabled.stdout)["env"])
        enabled = self.run_launcher("homelab", self.credentials, "--federation")
        self.assertEqual(enabled.returncode, 0, enabled.stderr)
        self.assertEqual(json.loads(enabled.stdout)["env"]["MEMORY_LAYER_FEDERATION_CONFIG"], str(federation))
        federation.unlink()
        federation.symlink_to(self.config)
        self.assertNotEqual(self.run_launcher("homelab", self.credentials, "--federation").returncode, 0)

    def test_wrong_identity_path_symlinks_and_flags_fail_before_execution(self):
        for args in [(), ("../homelab", self.credentials), ("Trust", self.credentials),
                     ("trust", self.credentials), ("homelab", self.root),
                     ("homelab", self.credentials, "--enable-models"),
                     ("homelab", self.credentials, "--federation", "extra")]:
            result = self.run_launcher(*args)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
        self.config.unlink()
        self.config.symlink_to(self.executable)
        self.assertNotEqual(self.run_launcher("homelab", self.credentials).returncode, 0)


if __name__ == "__main__":
    unittest.main()
