"""Regression checks for the public plugin/MCP configuration contract."""

from contextlib import redirect_stdout
import importlib.util
import io
from pathlib import Path
import shutil
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]


class PluginDocumentationContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="codifix-plugin-docs-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for relative in (
            "Cargo.toml",
            "plugins/codifix/.codex-plugin/plugin.json",
            "plugins/codifix/.mcp.json",
            "plugins/codifix/skills/codifix-codex/SKILL.md",
            ".agents/plugins/marketplace.json",
            "docs/user/codex-desktop-plugin.md",
            "docs/user/cli/mcp.md",
        ):
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPO / relative, target)

        spec = importlib.util.spec_from_file_location(
            "codifix_plugin_contract", REPO / "scripts/check-codex-plugin.py"
        )
        self.contract = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.contract)
        self.contract.ROOT = self.root
        self.contract.PLUGIN_ROOT = self.root / "plugins/codifix"

    def replace(self, relative, old, new):
        target = self.root / relative
        original = target.read_text(encoding="utf-8")
        self.assertIn(old, original)
        target.write_text(original.replace(old, new), encoding="utf-8")

    def test_current_documented_configuration_passes(self):
        with redirect_stdout(io.StringIO()):
            self.assertEqual(self.contract.main(), 0)

    def test_stale_compatibility_version_is_rejected(self):
        guide = "docs/user/codex-desktop-plugin.md"
        version = self.contract.load_json(
            self.contract.PLUGIN_ROOT / ".codex-plugin/plugin.json"
        )["version"]
        self.replace(guide, f"both declare `{version}`", "both declare `0.0.0`")
        with self.assertRaisesRegex(AssertionError, "compatibility version"):
            self.contract.main()

    def test_old_codex_executable_is_rejected(self):
        self.replace("docs/user/cli/mcp.md", 'command = "codifix"', 'command = "memory"')
        with self.assertRaisesRegex(AssertionError, "executable"):
            self.contract.main()

    def test_missing_scoped_token_forwarding_is_rejected(self):
        self.replace("docs/user/cli/mcp.md", 'env_vars = ["CODIFIX_CLIENT_TOKEN"]', "")
        with self.assertRaisesRegex(AssertionError, "scoped client token"):
            self.contract.main()

    def test_old_claude_executable_is_rejected(self):
        self.replace("docs/user/cli/mcp.md", '"command": "codifix"', '"command": "memory"')
        with self.assertRaisesRegex(AssertionError, "executable"):
            self.contract.main()


if __name__ == "__main__":
    unittest.main()
