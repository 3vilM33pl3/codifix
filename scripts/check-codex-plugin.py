#!/usr/bin/env python3
"""Check the repository-owned Codifix Codex plugin contract."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
PLUGIN_ROOT = ROOT / "plugins" / "codifix"


def load_json(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"invalid JSON at {path.relative_to(ROOT)}: {error}") from error
    if not isinstance(payload, dict):
        raise AssertionError(f"{path.relative_to(ROOT)} must contain a JSON object")
    return payload


def main() -> int:
    manifest = load_json(PLUGIN_ROOT / ".codex-plugin" / "plugin.json")
    assert manifest["name"] == "codifix"
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]
    assert manifest["version"] == workspace["version"]
    assert manifest["skills"] == "./skills/"
    assert manifest["mcpServers"] == "./.mcp.json"
    assert manifest["license"] == "AGPL-3.0-or-later"
    assert manifest["interface"]["displayName"] == "Codifix"

    mcp = load_json(PLUGIN_ROOT / ".mcp.json")
    server = mcp["mcpServers"]["codifix"]
    assert server == {
        "command": "codifix",
        "args": ["mcp", "run"],
        "env_vars": ["CODIFIX_CLIENT_TOKEN"],
    }

    skill = PLUGIN_ROOT / "skills" / "codifix-codex" / "SKILL.md"
    contents = skill.read_text(encoding="utf-8")
    assert contents.startswith("---\nname: codifix-codex\n")
    assert "codifix_query" in contents
    assert "codifix checkpoint start-execution" in contents
    assert "codifix remember" in contents

    marketplace = load_json(ROOT / ".agents" / "plugins" / "marketplace.json")
    assert marketplace["name"] == "codifix"
    assert marketplace["plugins"] == [
        {
            "name": "codifix",
            "source": {"source": "local", "path": "./plugins/codifix"},
            "policy": {"installation": "AVAILABLE", "authentication": "ON_INSTALL"},
            "category": "Productivity",
        }
    ]

    print("Codifix Codex plugin contract passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, KeyError) as error:
        print(f"Codifix Codex plugin contract failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
