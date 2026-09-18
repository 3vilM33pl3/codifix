#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Real TUI on a PTY, against an explicitly configured rehearsal or preview.

Called by web/tests/supervisor-real-rehearsal.mjs with private temporary config.
No host environment, credentials or terminal contents are printed or retained.
"""
import errno
import fcntl
import json
import os
import pty
import select
import signal
import struct
import sys
import termios
import time
import pyte


class RehearsalFailure(Exception):
    """Static diagnostic, never subprocess output."""


class Utf8Screen:
    """Terminal cursor columns count characters, not the bytes of box borders."""

    def __init__(self, rows, columns):
        self.screen = pyte.Screen(columns, rows)
        self.stream = pyte.ByteStream(self.screen)

    def feed(self, data):
        # PTY reads may split a multibyte character. Preserve decoder state so
        # later cursor-addressed updates cannot corrupt confirmation text.
        self.stream.feed(data)

    def __str__(self):
        return "\n".join(self.screen.display)


def main():
    binary, config, instance = sys.argv[1:4]
    controls = sys.argv[4:] == ["controls"]
    enabled_inspection = sys.argv[4:] == ["enabled-inspection"]
    if sys.argv[4:] and not controls and not enabled_inspection:
        raise RehearsalFailure("unknown rehearsal mode")
    if not os.path.isabs(binary) or not os.path.isabs(config):
        raise RehearsalFailure("explicit rehearsal paths required")
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8",
           "TERM": "xterm-256color", "MEMORY_LAYER_PROFILE": "prod"}
    pid, master = pty.fork()
    if pid == 0:
        os.chdir(os.path.dirname(config))
        os.execve(binary, [binary, "--config", config, "tui", "--project", "synthetic" if controls else "memory"], env)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 55, 180, 0, 0))
    screen = Utf8Screen(55, 180)
    output_bytes = 0
    exited = False

    def drain(duration):
        nonlocal output_bytes
        until = time.monotonic() + duration
        while time.monotonic() < until:
            ready, _, _ = select.select([master], [], [], min(0.1, max(0, until - time.monotonic())))
            if ready:
                try:
                    data = os.read(master, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        return
                    raise
                if not data:
                    return
                output_bytes += len(data)
                screen.feed(data)
                if output_bytes > 4 * 1024 * 1024:
                    raise RehearsalFailure("bounded terminal output exceeded")

    def text():
        return str(screen)

    try:
        drain(2)
        for _ in range(14):
            os.write(master, b"\x1b[Z")  # Shift-Tab; no mutation shortcut.
            drain(0.3)
            if "Press r to inspect this instance" in text():
                break
        else:
            raise RehearsalFailure("supervisor navigation was not visible")
        os.write(master, b"r")
        for _ in range(30):
            drain(0.2)
            if "PolicyInstalled" in text():
                break
        rendered = text()
        expected_policy = "enabled (activation separate)" if enabled_inspection else "paused"
        for label, required in [("identity", instance), ("area", "Workshop"), ("policy", expected_policy), ("activation", "Execution not integrated"), ("history", "PolicyInstalled")]:
            if required not in rendered:
                raise RehearsalFailure(f"supervisor {label} did not render")
        if controls:
            for key, confirm, expected in [(b"e", "CONFIRM RESUME", "enabled (activation separate)"), (b"p", "CONFIRM PAUSE", "paused")]:
                os.write(master, key)
                for _ in range(30):
                    drain(0.1)
                    if confirm in text() and "Enter confirms/retries" in text():
                        break
                else:
                    # Booleans only: diagnose readiness without retaining terminal
                    # bodies or changing the requirement to see both safeguards.
                    raise RehearsalFailure(f"{confirm}: exact supervisor confirmation was not visible "
                        f"(heading={confirm in text()}, terms={'Enter confirms/retries' in text()}, "
                        f"loading={'Loading supervisor' in text()}, controls={'p: prepare pause' in text()})")
                os.write(master, b"\r")
                for _ in range(30):
                    drain(0.2)
                    if "CONFIRM " not in text() and expected in text():
                        break
                else:
                    raise RehearsalFailure("confirmed policy outcome was not visible")
        os.write(master, b"\x03")
        drain(0.5)
        for _ in range(20):
            child, status = os.waitpid(pid, os.WNOHANG)
            if child:
                exited = True
                if not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
                    raise RehearsalFailure("terminal did not exit cleanly")
                break
            drain(0.1)
        if not exited:
            raise RehearsalFailure("terminal did not stop")
        print(json.dumps({"tui_pty": "passed", "identity": "matched", "mutations": controls}))
    finally:
        if not exited:
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        os.close(master)


if __name__ == "__main__":
    try:
        main()
    except RehearsalFailure as error:
        print(f"Synthetic supervisor PTY rehearsal failed: {error}", file=sys.stderr)
        sys.exit(1)
    except Exception:
        print("Synthetic supervisor PTY rehearsal failed; terminal contents suppressed", file=sys.stderr)
        sys.exit(1)
