#!/usr/bin/env python3
"""Normalize Tauri's SemVer prerelease to Debian package ordering."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

RAW_VERSION = "0.2.0-rc.1"
DEBIAN_VERSION = "0.2.0~rc.1"
PACKAGE_NAME = "codifix-desktop"


def fields(package):
    result = subprocess.run(
        ["dpkg-deb", "-f", str(package), "Package", "Version", "Architecture"],
        check=True, capture_output=True, text=True)
    parsed = {}
    for line in result.stdout.splitlines():
        key, separator, value = line.partition(": ")
        if not separator or key in parsed:
            raise ValueError("invalid or duplicate Debian control field")
        parsed[key] = value
    return parsed


def normalize(package):
    original = Path(package)
    if original.is_symlink():
        raise ValueError("Tauri package must be a regular file")
    package = original.resolve(strict=True)
    if not package.is_file():
        raise ValueError("Tauri package must be a regular file")
    source = fields(package)
    architecture = source.get("Architecture")
    if source != {"Package": PACKAGE_NAME, "Version": RAW_VERSION,
                  "Architecture": architecture}:
        raise ValueError("unexpected Tauri package identity")
    if architecture not in {"amd64", "arm64"}:
        raise ValueError("unsupported Tauri Debian architecture")
    destination = package.parent / f"codifix-desktop_{DEBIAN_VERSION}_{architecture}.deb"
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(f"refusing to overwrite {destination}")

    with tempfile.TemporaryDirectory(prefix="codifix-desktop-deb-", dir=package.parent) as temp:
        temporary = Path(temp)
        tree = temporary / "package"
        subprocess.run(["dpkg-deb", "--raw-extract", str(package), str(tree)], check=True)
        control = tree / "DEBIAN/control"
        data = control.read_bytes()
        pattern = re.compile(rb"(?m)^Version: " + re.escape(RAW_VERSION.encode()) + rb"\r?$")
        rewritten, count = pattern.subn(b"Version: " + DEBIAN_VERSION.encode(), data)
        if count != 1:
            raise ValueError("Tauri package control has no unique expected Version field")
        control.write_bytes(rewritten)
        staged = temporary / destination.name
        subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(staged)],
                       check=True, stdout=subprocess.DEVNULL)
        expected = {"Package": PACKAGE_NAME, "Version": DEBIAN_VERSION, "Architecture": architecture}
        if fields(staged) != expected:
            raise ValueError("normalized package identity differs")
        os.replace(staged, destination)
    package.unlink()
    return destination


def built_package(target=None):
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1",
         "--locked", "--manifest-path", str(root / "src-tauri/Cargo.toml")],
        cwd=root, text=True))
    target_directory = Path(metadata["target_directory"])
    if target is not None:
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", target):
            raise ValueError("invalid Rust target triple")
        target_directory /= target
    directory = target_directory / "release/bundle/deb"
    matches = sorted(directory.glob(f"Codifix Desktop_{RAW_VERSION}_*.deb"))
    if len(matches) != 1:
        raise FileNotFoundError(f"expected exactly one raw Tauri Debian package under {directory}")
    return matches[0]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=Path, nargs="?")
    parser.add_argument("--target", help="Rust target triple used by Tauri")
    args = parser.parse_args()
    package = args.package if args.package is not None else built_package(args.target)
    print(normalize(package))


if __name__ == "__main__":
    main()
