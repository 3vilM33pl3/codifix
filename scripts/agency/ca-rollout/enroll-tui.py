#!/usr/bin/python3 -I
"""Supervise upgrades for an explicitly enrolled local CA TUI."""
from argparse import ArgumentParser
import importlib.util
import os
from pathlib import Path
import stat
import sys


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("--supervise", action="store_true", required=True)
    parser.add_argument("--settings", type=Path, required=True)
    args = parser.parse_args()
    path = Path(__file__).absolute().with_name("console.py")
    try:
        if os.geteuid() != 0:
            raise PermissionError()
        for ancestor in path.parents:
            info = ancestor.lstat()
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
                raise PermissionError()
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise PermissionError()
    except OSError:
        print("A root-owned CA rollout deployment is required", file=sys.stderr)
        return 1
    spec = importlib.util.spec_from_file_location("codifix_ca_console", path)
    console = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(console)
    return console.main(["--settings", str(args.settings)], mode="tui")


if __name__ == "__main__":
    raise SystemExit(main())
