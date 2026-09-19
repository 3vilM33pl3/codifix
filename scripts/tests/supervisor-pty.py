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
    proposals = sys.argv[4:] == ["proposals"]
    question_preview = sys.argv[4:] == ["question-preview"]
    enabled_inspection = sys.argv[4:] == ["enabled-inspection"] or proposals or question_preview
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
        if proposals:
            os.write(master, b"v")
            for _ in range(30):
                drain(0.2)
                if "Private supervisor proposals" in text() and "Loading private" not in text():
                    break
            os.write(master, b"\r")
            for _ in range(30):
                drain(0.2)
                if "Question:" in text() and "Evidence 1:" in text():
                    break
            else:
                raise RehearsalFailure("private cited proposal did not render")
            os.write(master, b"a")
            drain(0.3)
            if "CONFIRM Approve" not in text() or "Permanent human review only" not in text():
                raise RehearsalFailure("private review confirmation did not render")
            os.write(master, b"c")  # Never approve the user's actual proposal.
            drain(0.3)
            os.write(master, b"\x1b")
            drain(0.3)
        if question_preview:
            os.write(master, b"v")
            for _ in range(30):
                drain(0.2)
                if "Private supervisor proposals" in text() and "Loading private" not in text():
                    break
            else:
                raise RehearsalFailure("private proposals were not ready for question navigation")
            os.write(master, b"q")
            for _ in range(30):
                drain(0.2)
                if "Private questions" in text() and "Checking private question" not in text():
                    break
            else:
                raise RehearsalFailure("private question listing did not render")
            os.write(master, b"n")
            drain(0.2)
            if "Exact question: Enter prepares" not in text():
                raise RehearsalFailure("private question input did not render")
            os.write(master, b"What does the approved collection say about ATProto?")
            drain(0.3)
            if "What does the approved collection say about ATProto?" not in text():
                raise RehearsalFailure("exact question input was not preserved")
            os.write(master, b"\r")  # Prepare only, not confirmation.
            for _ in range(30):
                drain(0.2)
                if "CONFIRMS PAID QUESTION" in text():
                    break
            else:
                raise RehearsalFailure("paid question preview did not render "
                    f"(input={'Exact question: Enter prepares' in text()}, "
                    f"loading={'Checking private question' in text()}, "
                    f"denied={'Question access denied' in text()}, "
                    f"conflict={'Question conflicts' in text()}, "
                    f"unconfirmed={'Question outcome unconfirmed' in text()}, "
                    f"unreadable={'Unreadable question receipt' in text()}, "
                    f"unavailable={'unavailable' in text()}, "
                    f"preview={'Profile digest:' in text()}, "
                    f"destination={'openai' in text()}, "
                    f"auth_transport={'Could not inspect local authentication' in text()})")
            if "openai" not in text() or "gpt-5.6-sol" not in text():
                raise RehearsalFailure("question destination/model not visible")
            os.write(master, b"\x1b")  # Never submit a paid question in this rehearsal.
            drain(0.3)
            if "CONFIRMS PAID QUESTION" in text():
                raise RehearsalFailure("question cancellation was not visible")
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
        # Allow runtime/network shutdown after the UI handles Ctrl-C. Read only;
        # never send a second Enter or interpret slow shutdown as paid consent.
        for _ in range(100):
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
