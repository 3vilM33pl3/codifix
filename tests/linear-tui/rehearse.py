#!/usr/bin/env python3
"""Drive first installation and a verified Linear request through the real TUI."""
import argparse
from contextlib import ExitStack
from datetime import datetime, timedelta, timezone
import importlib.util
import json
import os
import re
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
from urllib.request import Request, urlopen
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[2]
HOMELAB_PROJECT = "e3eef73c-065a-4ac0-9f43-a50411507ef4"
CODIFIX_PROJECT = "01b99d74-38fe-4c67-9f47-adefbbe37ee1"
TEAM = "dbac4c6d-0a2a-4401-a7e5-d14f97d9a1b6"
IN_PROGRESS = "f329f1f9-650d-466f-b500-c83c81bd0d1e"
DONE = "18bb3348-212a-4b68-b4cc-85121b0d695e"


def checked(command, **kwargs):
    return subprocess.check_output(command, text=True, **kwargs).strip()


def wait(label, check, timeout):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if check():
            return
        time.sleep(.25)
    raise RuntimeError("Timed out waiting for " + label)


def enroll_codifix(profile):
    mandate = str(uuid4())
    settings = {"version": 1, "projects": {"codifix": {
        "project_id": CODIFIX_PROJECT, "team_id": TEAM, "mandate_id": mandate,
        "project_name": "Codifix", "in_progress_state_id": IN_PROGRESS, "done_state_id": DONE,
    }}}
    path = profile / "linear-projects.json"
    with open(path, "x", opener=lambda path, flags: os.open(path, flags, 0o600)) as stream:
        json.dump(settings, stream)
    return mandate


def rehearse_projects(profile, issues, mandate):
    local = json.loads((profile / "tui.json").read_text())
    token = json.loads((profile / "installation.json").read_text())["roles"]["homelab"]["operator"]
    tracking_key = str(uuid4())

    def call(path, body=None):
        envelope = Request(local["endpoint"] + path,
                           headers={"Authorization": "Bearer " + token, "A2A-Version": "1.0",
                                    "Content-Type": "application/json"},
                           data=json.dumps(body).encode() if body is not None else None)
        with urlopen(envelope, timeout=12) as response:
            return json.load(response)

    def send(lifecycle, project=CODIFIX_PROJECT, grant=mandate):
        deadline = (datetime.now(timezone.utc) + timedelta(minutes=4)).isoformat()
        payload = {"kind": "linear_track", "tracking_key": tracking_key, "lifecycle": lifecycle,
                   "title": "Track the prepared project automatically",
                   "description": "Use the central agent and verify the saved issue."}
        if lifecycle == "blocked":
            payload["blocker"] = "The provider is temporarily unavailable."
        remote = {"request_id": str(uuid4()), "mandate_id": grant, "specialist_id": local["remote_specialist"],
                  "objective": "Track the scoped lifecycle event", "capability": "linear_track",
                  "resource": {"kind": "linear_project", "id": project},
                  "acceptance_criteria": ["linear_issue_readback_verified"], "deadline": deadline,
                  "budget": {"micro_usd": 0, "calls": 20, "turns": 8, "delegation_depth": 0}, "payload": payload}
        response = call("/v1/tasks", {
            "request_id": str(uuid4()), "mandate_id": local["local_mandate_id"],
            "specialist_id": local["local_specialist"], "objective": "Delegate to the resident Linear peer",
            "capability": "delegate", "resource": {"kind": "peer", "id": local["remote_peer"]},
            "acceptance_criteria": ["delegated_result_reported"], "deadline": deadline,
            "budget": {"micro_usd": 0, "calls": 30, "turns": 12, "delegation_depth": 1},
            "payload": {"kind": "delegate", "peer": local["remote_peer"], "request": remote},
        })
        id = response["contract"]["id"]
        result = {}

        def finished():
            result.update(call("/v1/tasks/" + id)["contract"])
            return result["status"] not in {"submitted", "working"}

        wait("scoped private ATProto receipt", finished, 40)
        return result["status"]

    original, = json.loads(issues.read_text()).values()
    assert original["project"]["id"] == HOMELAB_PROJECT
    assert send("started", project=HOMELAB_PROJECT) != "completed"
    assert send("started", grant=local["remote_mandate_id"]) != "completed"
    assert list(json.loads(issues.read_text()).values()) == [original]

    assert send("started") == "completed"
    items = json.loads(issues.read_text())
    first, = (item for item in items.values() if item["project"]["id"] == CODIFIX_PROJECT)
    assert first["state"]["id"] == IN_PROGRESS
    assert send("started") == "completed"
    assert json.loads(issues.read_text())[first["id"]]["writes"] == 1
    unavailable = issues.with_suffix(".unavailable")
    unavailable.touch()
    try:
        assert send("blocked") != "completed"
    finally:
        unavailable.unlink()
    assert send("blocked") == "completed"
    assert "temporarily unavailable" in json.loads(issues.read_text())[first["id"]]["description"]
    assert send("completed") == "completed"
    assert send("started") == "completed"
    final = json.loads(issues.read_text())
    assert len(final) == 2 and final[original["id"]] == original
    assert final[first["id"]]["state"]["id"] == DONE
    print("Passed two projects on one resident agent over private ATProto, cross-grant denial, retry, and lifecycle readback.")


def remove_test_containers(profile):
    # Only resources belonging to this rehearsal's fresh synthetic installation.
    spec = importlib.util.spec_from_file_location("linear_launcher", ROOT / "scripts/agency/linear-tui.py")
    launcher = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(launcher)
    launcher.DOCKER_GROUP = launcher.docker_access()
    filters = []
    database_volume = None
    installation = profile / "installation.json"
    if installation.exists():
        installation_id = json.loads(installation.read_text())["id"]
        assert re.fullmatch(r"[a-f0-9]{16}", installation_id)
        database_volume = "codifix-linear-db-" + installation_id
        filters.append(("container", "org.codifix.linear.installation=" + installation_id))
    env = profile / "private-pds/pds.env"
    if env.exists():
        match = re.search(r"^PRIVATE_AT_PROJECT=(codifix-pds-[a-f0-9]{16})$", env.read_text(), re.M)
        assert match
        label = "com.docker.compose.project=" + match[1]
        filters.extend((kind, label) for kind in ("container", "network", "volume"))
    for kind, label in filters:
        options = ["-a"] if kind == "container" else []
        items = launcher.docker(kind, "ls", *options, "-q", "--filter", "label=" + label,
                                capture_output=True, text=True).stdout.split()
        if items:
            options = ["-f", "-v"] if kind == "container" else []
            launcher.docker(kind, "rm", *options, *items, stdout=subprocess.DEVNULL)
    if database_volume:
        known = launcher.docker("volume", "ls", "-q", "--filter", "name=" + database_volume,
                                capture_output=True, text=True).stdout.splitlines()
        if database_volume in known:
            launcher.docker("volume", "rm", database_volume, stdout=subprocess.DEVNULL)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    entry = parser.add_mutually_exclusive_group(required=True)
    entry.add_argument("--bin-dir", type=Path)
    entry.add_argument("--launcher", type=Path, help="Exercise the installed codifix-linear command.")
    parser.add_argument("--resident", action="store_true", help="Also check a resident test service across TUI exit and restart.")
    parser.add_argument("--multi-project", action="store_true", help="Exercise a second project through the resident agent.")
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
        cleanup.callback(remove_test_containers, profile)
        provider = subprocess.Popen([sys.executable, ROOT / "tests/linear-tui/provider.py", issues, address_file],
                                    stdout=log, stderr=log)
        cleanup.callback(provider.wait, 5)
        cleanup.callback(provider.terminate)
        wait("loopback Linear fixture", address_file.exists, 10)
        address = address_file.read_text().strip()
        cli = ([args.launcher.resolve()] if args.launcher else
               [sys.executable, ROOT / "scripts/agency/linear-tui.py", "--bin-dir", args.bin_dir.resolve()])
        base_command = [*cli, "--state", profile]
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
            assert "Verify the compiled path with a real private PDS." in screen(), "TUI did not show exact issue wording"
            keys("C-e")
            wait("correction editor", lambda: "Correct Linear proposal" in screen(), 10)
            keys("C-u")
            expected = "Verify the private PDS path and read the issue back from Linear."
            keys("-l", expected)
            keys("C-s")
            wait("corrected wording review", lambda: "Review Linear proposal" in screen() and expected in screen(), 35)
            assert json.loads(issues.read_text()) == {}, "Linear was written before revised wording was approved"
            keys("Enter")
            wait("verified issue link", lambda: "linear.app/example/issue/COD-42" in screen(), 40)
            saved, = json.loads(issues.read_text()).values()
            assert saved["title"] == "Terminal workflow acceptance" and saved["description"] == expected
            assert "Verified Linear issue" in screen(), "TUI did not report readback verification"
            keys("Escape")
            wait("terminal exit", exited, 10)
            if args.resident or args.multi_project:
                unit = directory.name + ".service"
                ctl = ["systemctl", "--user"]
                cleanup.callback(subprocess.run, [*ctl, "stop", unit], capture_output=True)
                service = ["systemd-run", "--user", "--collect", "--unit", unit,
                           "--property=UMask=0077", *map(str, cli), "--serve"]
                marker = profile / "resident.json"

                def started():
                    if checked([*ctl, "show", "--property=ActiveState", "--value", unit]) in {"inactive", "failed"}:
                        raise RuntimeError("Resident test service exited before readiness.")
                    return marker.exists()

                mandate = enroll_codifix(profile) if args.multi_project else None
                checked(service)
                wait("resident startup", started, 120)
                first = json.loads(marker.read_text())
                previous_ports = (profile / "ports.json").read_bytes()
                checked([*socket, "respawn-pane", "-k", "-t", "linear:0.0",
                         shlex.join([*map(str, base_command), "--attach"])])
                wait("attached draft screen", draft_visible, 15)
                keys("Escape")
                wait("attached terminal exit", exited, 10)
                assert checked([*ctl, "is-active", unit]) == "active"
                assert json.loads(marker.read_text()) == first
                with urlopen(json.loads((profile / "tui.json").read_text())["endpoint"] + "/health", timeout=3) as response:
                    assert json.load(response)["status"] == "ok"
                checked([*ctl, "restart", unit])
                wait("new resident", lambda: marker.exists() and json.loads(marker.read_text())["pid"] != first["pid"], 120)
                assert json.loads(marker.read_text())["linear_peer_endpoint"] == first["linear_peer_endpoint"]
                assert (profile / "ports.json").read_bytes() == previous_ports
                if mandate:
                    rehearse_projects(profile, issues, mandate)
                checked([*ctl, "stop", unit])
                assert not marker.exists()
                print("Passed detached operator session, stable private peer endpoint and resident restart.")
            print(f"Passed fresh installation, private ATProto review and correction, explicit approval and verified issue link. Evidence: {directory}")
        except (RuntimeError, AssertionError):
            print(f"Rehearsal evidence: {directory}\n{screen()}", file=sys.stderr)
            if scope.exists():
                print(scope.read_text(errors="replace")[-1000:], file=sys.stderr)
            raise


if __name__ == "__main__":
    main()
