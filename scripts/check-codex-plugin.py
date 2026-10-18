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
    guide = (ROOT / "docs/user/codex-desktop-plugin.md").read_text(encoding="utf-8")
    documented_version = re.search(
        r"plugin manifest and workspace both declare `([^`]+)`", guide
    )
    assert documented_version, "plugin guide must identify its checked-in manifest version"
    assert documented_version.group(1) == manifest["version"], (
        "plugin guide compatibility version must match the checked-in manifest"
    )
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

    mcp_guide = (ROOT / "docs/user/cli/mcp.md").read_text(encoding="utf-8")
    toml_examples = re.findall(r"```toml\n(.*?)\n```", mcp_guide, re.DOTALL)
    documented_servers = [
        tomllib.loads(example).get("mcp_servers", {}).get("codifix")
        for example in toml_examples
    ]
    documented_servers = [entry for entry in documented_servers if entry is not None]
    assert documented_servers, "MCP guide must include its Codex server configuration"
    assert all(entry["command"] == server["command"] for entry in documented_servers), (
        "MCP guide executable must match the plugin configuration"
    )
    assert all(entry.get("env_vars") == server["env_vars"] for entry in documented_servers), (
        "Codex MCP guide must explicitly forward the scoped client token variable"
    )
    for example in re.findall(r"```json\n(.*?)\n```", mcp_guide, re.DOTALL):
        configured = json.loads(example).get("mcpServers", {})
        if "codifix" in configured:
            assert configured["codifix"]["command"] == server["command"], (
                "MCP guide executable must match the plugin configuration"
            )

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
