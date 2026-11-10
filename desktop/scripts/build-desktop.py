#!/usr/bin/env python3
"""Run Tauri and normalize its Debian package under the same target directory."""
import os
from pathlib import Path
import subprocess
import sys

DESKTOP_ROOT = Path(__file__).resolve().parents[1]


def rust_target(arguments):
    found = None
    for index, argument in enumerate(arguments):
        if argument == "--target":
            if index + 1 >= len(arguments) or arguments[index + 1].startswith("-") or found is not None:
                raise ValueError("--target requires one target triple")
            found = arguments[index + 1]
        elif argument.startswith("--target="):
            if found is not None:
                raise ValueError("only one --target is supported")
            found = argument.partition("=")[2]
    if found is not None and not found.replace("-", "").replace("_", "").replace(".", "").isalnum():
        raise ValueError("invalid Rust target triple")
    return found


def main():
    environment = os.environ.copy()
    target = environment.get("CARGO_TARGET_DIR")
    if target:
        target_path = Path(target)
        if not target_path.is_absolute():
            target_path = (DESKTOP_ROOT / target_path).resolve()
        environment["CARGO_TARGET_DIR"] = str(target_path)

    build = subprocess.run(["tauri", "build", "--bundles", "deb", *sys.argv[1:]],
                           cwd=DESKTOP_ROOT, env=environment)
    if build.returncode:
        return build.returncode
    normalize_command = [sys.executable, "scripts/normalize-deb-version.py"]
    target = rust_target(sys.argv[1:])
    if target is not None:
        normalize_command.extend(["--target", target])
    normalize = subprocess.run(normalize_command, cwd=DESKTOP_ROOT, env=environment)
    return normalize.returncode


if __name__ == "__main__":
    raise SystemExit(main())
