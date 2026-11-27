#!/usr/bin/python3 -I
"""CA-local hooks for a scoped TUI upgrade supervisor."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, build_opener

OBJECT = re.compile(r"codifix-releases/ca/[0-9A-Za-z.-]+/tui-companion-footer-[0-9A-Za-z.-]+")
VERSION = re.compile(r"[0-9A-Za-z.+-]{1,64}")


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def fetch(config, key, staged):
    require(OBJECT.fullmatch(key), "invalid CA TUI object key")
    subprocess.run(
        ["/usr/bin/rclone", "--config", config, "copyto", "release:" + key, staged,
         "--max-transfer", "128Mi", "--cutoff-mode", "hard", "--s3-no-check-bucket"],
        check=True,
    )


def install(destination, staged):
    path = Path(destination)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
        temp = Path(output.name)
        try:
            with open(staged, "rb") as source:
                shutil.copyfileobj(source, output)
            os.fchmod(output.fileno(), 0o755)
            output.flush()
            os.fsync(output.fileno())
            os.replace(temp, destination)
        finally:
            temp.unlink(missing_ok=True)


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


def processes(pid):
    """Return the pane and its descendants, including non-exec shell wrappers."""
    pending = [pid]
    while pending:
        current = pending.pop()
        proc = Path("/proc") / str(current)
        yield proc
        try:
            children = (proc / "task" / str(current) / "children").read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue
        pending.extend(map(int, children.split()))


def launched_in_pane(pid, installed):
    for proc in processes(pid):
        try:
            if os.path.samefile(proc / "exe", installed):
                return True
        except (FileNotFoundError, ProcessLookupError):
            continue
    return False


def verify(launcher, health_url, installed, release):
    require(os.geteuid() != 0, "verify as the operator who uses the TUI")
    require(VERSION.fullmatch(release), "invalid client version")
    output = subprocess.check_output([installed, "--version"], text=True, timeout=5)
    require(output.strip() == "codifix " + release, "installed client version mismatch")
    address = urlsplit(health_url)
    require(address.scheme == "http" and address.hostname in {"127.0.0.1", "::1"}
            and address.port and address.path == "/health" and not address.query
            and not address.fragment and address.username is None and address.password is None,
            "invalid reviewed companion health URL")
    client = build_opener(ProxyHandler({}), NoRedirect())
    with client.open(health_url, timeout=5) as response:
        body = response.read(65537)
        require(len(body) <= 65536, "companion health response too large")
        health = json.loads(body)
    companion = health["version"]
    require(health["status"] == "ok" and VERSION.fullmatch(companion), "worker health mismatch")

    with tempfile.TemporaryDirectory() as work:
        tmux = ["/usr/bin/tmux", "-S", work + "/terminal", "-f", "/dev/null"]
        env = os.environ | {"TERM": "xterm-256color", "CODIFIX_PROFILE": "production"}
        wanted = "Companion v" + companion + " fresh"
        subprocess.run(tmux + ["new-session", "-d", "-x", "100", "-y", "24", "/usr/bin/env", "--", launcher],
                       env=env, check=True, timeout=5)
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                pid = int(subprocess.check_output(tmux + ["display-message", "-p", "#{pane_pid}"],
                                                env=env, timeout=5, text=True))
                screen = subprocess.check_output(tmux + ["capture-pane", "-p"],
                                                 env=env, timeout=5, text=True)
                if launched_in_pane(pid, installed) and any(line.startswith(wanted) for line in screen.splitlines()):
                    return
                time.sleep(0.25)
            raise RuntimeError("installed client did not render the fresh companion footer")
        finally:
            subprocess.run(tmux + ["kill-server"], env=env, check=False, timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    for action, parameters in {
        "fetch": ["config", "key", "staged"],
        "install": ["destination", "staged"],
        "verify": ["launcher", "health_url", "installed", "release"],
    }.items():
        command = sub.add_parser(action)
        for parameter in parameters:
            command.add_argument(parameter)
        command.set_defaults(run=globals()[action])
    args = vars(parser.parse_args())
    args.pop("action")
    args.pop("run")(**args)


if __name__ == "__main__":
    main()
