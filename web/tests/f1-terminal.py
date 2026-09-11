# SPDX-License-Identifier: AGPL-3.0-or-later
"""Real private tmux PTY; synthetic instance fixture only."""
import json
import subprocess
import sys
import time
import uuid

data = json.load(sys.stdin)
prefix = ["tmux", "-L", "memory-f1-" + uuid.uuid4().hex, "-f", "/dev/null"]

def tmux(*args):
    return subprocess.run(prefix + list(args), capture_output=True, text=True, check=True).stdout

def key(value):
    tmux("send-keys", "-t", "f1", value)

def text(value):
    tmux("send-keys", "-t", "f1", "-l", value)

def wait_for(value):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        screen = tmux("capture-pane", "-p", "-t", "f1")
        if value in screen:
            return screen
        time.sleep(0.1)
    raise AssertionError("F1 terminal did not reach " + value)

try:
    tmux("new-session", "-d", "-s", "f1", "-x", "200", "-y", "150",
         data["binary"], "--config", data["config"], "federation", "--project", data["project"], "tui")
    wait_for("Federation")
    for _ in range(13):
        key("F3")
    key("F5")
    wait_for("Reviewed recovery")
    key("F3")
    text(data["publisher"]); key("Tab"); text(data["collection"]); key("Tab")
    text("What is the reviewed incident diagnosis?")
    key("F5")
    screen = wait_for('"output_destination": "local_human"')
    request = screen.split('"request_id": "', 1)[1].split('"', 1)[0]
    key("F3"); key("F5")
    wait_for("Review every disclosed byte and destination before F6")
    key("F6"); wait_for('"status"')
    key("F3"); text(request)
    for _ in range(10):
        key("F5"); time.sleep(1)
        if '"status": "answered"' in tmux("capture-pane", "-p", "-t", "f1"):
            break
    screen = wait_for('"status": "answered"')
    assert data["publisher"] in screen
    assert "UNSELECTED_SECRET_SENTINEL" not in screen
    key("Escape")
    print("F1 PTY confirmed question and cited answer passed")
finally:
    subprocess.run(prefix + ["kill-server"], capture_output=True)
