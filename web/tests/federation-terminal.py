# SPDX-License-Identifier: AGPL-3.0-or-later
"""Real PTY with tmux's VT screen parser on a private disposable socket.

Ratatui sends cell diffs, not complete lines. Assert rendered screen contents.
"""
import json
import subprocess
import sys
import time
import uuid

data = json.load(sys.stdin)
socket = "memory-e-" + uuid.uuid4().hex
prefix = ["tmux", "-L", socket, "-f", "/dev/null"]

def tmux(*args):
    return subprocess.run(prefix + list(args), capture_output=True, text=True, check=True).stdout

def wait_for(text):
    deadline = time.monotonic() + 20
    screen = ""
    while time.monotonic() < deadline:
        screen = tmux("capture-pane", "-p", "-t", "federation")
        if text in screen:
            return
        time.sleep(0.1)
    raise AssertionError("Terminal did not reach " + text + "\nSynthetic screen:\n" + screen)

def key(name):
    tmux("send-keys", "-t", "federation", name)

def text(value):
    tmux("send-keys", "-t", "federation", "-l", value)

try:
    tmux("new-session", "-d", "-s", "federation", "-x", "180", "-y", "60",
         data["binary"], "--config", data["config"], "federation", "--project", data["project"], "tui")
    wait_for("Federation")
    for _ in range(7):
        key("F3")
    wait_for("Shared context")
    text(data["origin"])
    key("Tab")
    text(data["canonical"])
    key("F5")
    wait_for("as_of_sequence")
    key("F3")
    wait_for("Reviewed two-organization correction")
    key("C-u")
    text("Second terminal correction for rejection")
    key("F5")
    wait_for("Review every disclosed byte and destination before F6")
    key("F6")
    wait_for("queued")
    key("Escape")
    print("PTY federation context and confirmed correction passed")
finally:
    subprocess.run(prefix + ["kill-server"], capture_output=True)
