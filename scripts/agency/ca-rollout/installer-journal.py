#!/usr/bin/python3 -I
"""Read the reviewed companion and installer unit state with bounded journal labels."""
from argparse import ArgumentParser
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

FIELDS = ("ActiveState", "SubState", "Result", "ExecMainCode", "ExecMainStatus")
SAFE_LABELS = ("Main process exited,", "Failed with result", "Failed to start",
               "Succeeded", "Deactivated successfully", "CA rollout stopped", "CA broker callback failed")


def label_journal(lines):
    for line in lines.splitlines():
        event = json.loads(line)
        message, when = event.get("MESSAGE"), event.get("__REALTIME_TIMESTAMP")
        label = next((entry for entry in SAFE_LABELS if isinstance(message, str) and entry in message),
                     "other service message")
        yield {"time": when if isinstance(when, str) and when.isdecimal() else None, "message": label}


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("--settings", type=Path, required=True)
    parser.add_argument("--installer-unit", required=True)
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
    try:
        console.check(os.geteuid() == 0, "Run as the CA administrator")
        settings = console.read_settings(args.settings, "tui")
        installer = args.installer_unit
        console.check(console.UNIT.fullmatch(installer) and installer != settings["unit"], "Invalid installer unit")
        states = {}
        for unit in (settings["unit"], installer):
            fields = console.command([console.SYSTEMCTL, "show", "--property", ",".join(FIELDS), "--", unit], timeout=8)
            values = dict(line.split("=", 1) for line in fields.decode().splitlines() if "=" in line)
            states[unit] = {name: values[name] for name in FIELDS
                            if name in values and re.fullmatch(r"[A-Za-z0-9-]{0,48}", values[name])}
        output = console.command(["/usr/bin/journalctl", "-u", installer, "-n", "15", "-o", "json", "--no-pager"], timeout=10)
        print(json.dumps({"units": states, "journal": list(label_journal(output.decode()))}))
    except (console.Stop, OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError):
        print("Unable to read the reviewed CA installer status", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
