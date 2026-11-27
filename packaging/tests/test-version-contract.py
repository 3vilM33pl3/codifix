#!/usr/bin/env python3
"""Check coordinated source versions and Debian package ordering."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CORE = "3.1.0"
DESKTOP = "0.2.0-rc.1"


def command(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--desktop-deb", type=Path)
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=ROOT, text=True))
    wrong = [(package["name"], package["version"]) for package in metadata["packages"]
             if package["version"] != CORE]
    assert not wrong, f"core Cargo package versions differ from {CORE}: {wrong}"

    for directory in ("web", "docs-site"):
        package = json.loads((ROOT / directory / "package.json").read_text())
        lock = json.loads((ROOT / directory / "package-lock.json").read_text())
        assert package["version"] == CORE, directory
        assert lock["version"] == lock["packages"][""]["version"] == CORE, f"{directory} lock"

    plugin = json.loads((ROOT / "plugins/codifix/.codex-plugin/plugin.json").read_text())
    assert plugin["version"] == CORE, "Codifix plugin"
    for skill in (ROOT / ".agents/skills").iterdir():
        if not skill.is_dir():
            continue
        for manifest in (skill / "SKILL.md", skill / "agents/openai.yaml"):
            if manifest.is_file():
                assert f"version: {CORE}" in manifest.read_text(), str(manifest.relative_to(ROOT))

    desktop_manifest = ROOT / "desktop/src-tauri/Cargo.toml"
    assert f'version = "{DESKTOP}"' in desktop_manifest.read_text()
    for path in ("desktop/package.json", "desktop/src-tauri/tauri.conf.json"):
        assert json.loads((ROOT / path).read_text())["version"] == DESKTOP, path
    desktop_lock = json.loads((ROOT / "desktop/package-lock.json").read_text())
    assert desktop_lock["version"] == desktop_lock["packages"][""]["version"] == DESKTOP

    for candidate, lower, upper in (
        ("3.0.0~rc.1", "2.0.0", "3.0.0"),
        ("3.1.0~rc.1", "3.0.0", "3.1.0~rc.2"),
        ("3.1.0~rc.2", "3.1.0~rc.1", "3.1.0~rc.3"),
        ("3.1.0~rc.3", "3.1.0~rc.2", CORE),
        ("0.2.0~rc.1", "0.1.4", "0.2.0"),
    ):
        command("dpkg", "--compare-versions", candidate, "gt", lower)
        command("dpkg", "--compare-versions", candidate, "lt", upper)

    if args.desktop_deb:
        package = args.desktop_deb.resolve(strict=True)
        actual = subprocess.check_output(["dpkg-deb", "-f", str(package), "Version"], text=True).strip()
        assert actual == "0.2.0~rc.1", f"Tauri Debian control Version is {actual!r}"
    print("PASS: core, desktop, plugin/skill metadata and Debian version ordering")


if __name__ == "__main__":
    try:
        main()
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
