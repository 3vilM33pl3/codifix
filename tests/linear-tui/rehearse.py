#!/usr/bin/env python3
"""Drive first installation and a verified Linear request through the real TUI."""
import argparse
from contextlib import ExitStack
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]


def checked(command, **kwargs):
    return subprocess.check_output(command, text=True, **kwargs).strip()


def wait(label, check, timeout):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if check():
            return
        time.sleep(.25)
    raise RuntimeError("Timed out waiting for " + label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    entry = parser.add_mutually_exclusive_group(required=True)
    entry.add_argument("--bin-dir", type=Path)
    entry.add_argument("--launcher", type=Path, help="Exercise the installed codifix-linear command.")
    args = parser.parse_args()
    directory = Path(tempfile.mkdtemp(prefix="codifix-linear-e2e-"))
    profile = directory / "profile"
    issues = directory / "issues.json"
    address_file = directory / "url"
    tmux = ["tmux", "-L", directory.name, "-f", "/dev/null"]
    socket = ["tmux", "-L", directory.name]
    scope = directory / "terminal.log"
    provider_log = directory / "provider.log"

    def keys(*args):
        checked([*socket, "send-keys", "-t", "linear:0.0", *args])

    def screen():
        result = subprocess.run([*socket, "capture-pane", "-p", "-t", "linear:0.0"],
                                text=True, capture_output=True)
        return result.stdout

    def exited():
        return checked([*socket, "display-message", "-p", "-t", "linear:0.0", "#{pane_dead}"]) == "1"

    def draft_visible():
        if exited():
            raise RuntimeError("Installer exited before opening the draft.")
        return "Title" in screen() and "Description" in screen()

    with ExitStack() as cleanup, provider_log.open("w") as log:
        provider = subprocess.Popen([sys.executable, ROOT / "tests/linear-tui/provider.py", issues, address_file],
                                    stdout=log, stderr=log)
        cleanup.callback(provider.wait, 5)
        cleanup.callback(provider.terminate)
        wait("loopback Linear fixture", address_file.exists, 10)
        address = address_file.read_text().strip()
        cli = ([args.launcher.resolve()] if args.launcher else
               [sys.executable, ROOT / "scripts/agency/linear-tui.py", "--bin-dir", args.bin_dir.resolve()])
        cli.extend(["--state", profile, "--fixture-endpoint", address])
        shell = "CODIFIX_LINEAR_TOKEN=synthetic-linear-key " + shlex.join(map(str, cli))
        shell += "; status=$?; exit $status"
        checked([*tmux, "new-session", "-d", "-x", "120", "-y", "35", "-s", "linear"])
        cleanup.callback(subprocess.run, [*socket, "kill-server"], capture_output=True)
        checked([*socket, "set-option", "-t", "linear", "remain-on-exit", "on"])
        checked([*socket, "pipe-pane", "-o", "-t", "linear:0.0", "cat >> " + shlex.quote(str(scope))])
        checked([*socket, "respawn-pane", "-k", "-t", "linear:0.0", "sh", "-c", shell])
        try:
            wait("Linear draft screen", draft_visible, 150)
            keys("-l", "Terminal workflow acceptance")
            keys("Tab")
            keys("-l", "Verify the compiled path with a real private PDS.")
            keys("C-s")
            wait("draft review", lambda: "Review Linear proposal" in screen(), 35)
            assert json.loads(issues.read_text()) == {}, "Linear was written before confirmation"
            expected = "Verify the compiled path with a real private PDS."
            assert expected in screen(), "TUI did not show exact issue wording"
            keys("Enter")
            wait("verified issue link", lambda: "linear.app/example/issue/COD-42" in screen(), 40)
            saved, = json.loads(issues.read_text()).values()
            assert saved["title"] == "Terminal workflow acceptance" and saved["description"] == expected
            assert "Verified Linear issue" in screen(), "TUI did not report readback verification"
            keys("Escape")
            wait("terminal exit", exited, 10)
            print(f"Passed fresh installation, private ATProto review, explicit approval and verified issue link. Evidence: {directory}")
        except (RuntimeError, AssertionError):
            print(f"Rehearsal evidence: {directory}\n{screen()}", file=sys.stderr)
            if scope.exists():
                print(scope.read_text(errors="replace")[-1000:], file=sys.stderr)
            raise


if __name__ == "__main__":
    main()
