#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Offline candidate check, not an installer or a production artifact admission.

Run only inside an independently configured network-none/read-only/non-root
container. No Docker daemon, real credentials, model endpoint, or candidate path
can be supplied here. The explicit SHA binds this disposable compatibility test;
it never changes codifix-supervisor-pi's production pins.
"""

import argparse
import asyncio
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import tempfile

MODEL = "gpt-5.6-sol"
TOKEN = "synthetic-local-probe-not-a-valid-provider-key"
QUESTION = "Synthetic routing check. Return the supplied synthetic response unchanged."
ANSWER = "Synthetic provider result; no real model was called."
SYSTEM = "This is a synthetic local transport test. No tools or external actions."
LIMITATION = "emulated userspace only, not native CA sandbox proof"
CANDIDATE = Path("/candidate/pi")
MAX_FRAME = 16_384
MAX_STDOUT = 65_536
MAX_STDERR = 8_192
CASE_SECONDS = 45


class CheckFailed(Exception):
    """Fixed metadata-only error labels; never attach child/provider content."""


def require(condition, label):
    if not condition:
        raise CheckFailed(label)


def decode_json(raw, allow_provider_error_duplicate=False):
    class Pairs(list):
        pass

    def unique(node, root=False):
        if isinstance(node, Pairs):
            result = {}
            # Pinned Pi agent.rs AgentEvent::ProviderError flattens a summary
            # (error.rs ProviderErrorSummary) which repeats its provider field.
            # Only this exact, identical non-authoritative diagnostic is allowed.
            # Requests, state/model identity and every other field remain strict.
            diagnostic = (root and allow_provider_error_duplicate and
                          [value for key, value in node if key == "type"] == ["provider_error"] and
                          [value for key, value in node if key == "provider"] == ["openai", "openai"])
            for key, value in node:
                require(key not in result or (diagnostic and key == "provider"),
                        "duplicate_json_field")
                result[key] = unique(value)
            return result
        if isinstance(node, list):
            return [unique(value) for value in node]
        return node

    def invalid_constant(_):
        raise CheckFailed("non_finite_json")

    try:
        value = unique(json.loads(raw, object_pairs_hook=Pairs,
                                  parse_constant=invalid_constant), root=True)
    except (ValueError, UnicodeError, RecursionError):
        raise CheckFailed("invalid_json") from None
    require(isinstance(value, dict), "json_object_required")
    return value


def readonly_mount(path, mountinfo):
    """Check the effective longest-prefix mount, including nested bind mounts."""
    matches = []
    for line in mountinfo.splitlines():
        before, separator, _ = line.partition(" - ")
        fields = before.split()
        require(separator and len(fields) >= 6, "mountinfo_format")
        mount = fields[4]
        if mount == "/" or path == mount or path.startswith(mount + "/"):
            matches.append((len(mount), "ro" in fields[5].split(",")))
    require(bool(matches), "mount_not_found")
    return max(matches)[1]


def sandbox_check():
    require(os.environ.get("CODIFIX_PI_SANDBOXED_PROBE") ==
            "network-none-read-only-root", "sandbox_marker_missing")
    require(os.geteuid() != 0, "root_forbidden")
    require(Path("/.dockerenv").is_file(), "container_marker_missing")
    require(sorted(p.name for p in Path("/sys/class/net").iterdir()) == ["lo"],
            "non_loopback_interface")
    status = dict(line.split(":", 1) for line in
                  Path("/proc/self/status").read_text().splitlines() if ":" in line)
    require(status.get("NoNewPrivs", "").strip() == "1", "no_new_privileges_required")
    for field in ("CapEff", "CapPrm", "CapBnd", "CapAmb"):
        require(status.get(field, "").strip() == "0000000000000000",
                "capabilities_not_dropped")
    mounts = Path("/proc/self/mountinfo").read_text()
    require(readonly_mount("/", mounts), "root_mount_not_readonly")
    require(readonly_mount(str(CANDIDATE), mounts), "candidate_mount_not_readonly")
    require(readonly_mount("/usr/aarch64-linux-gnu", mounts), "sysroot_not_readonly")
    require(not readonly_mount("/scratch", mounts), "scratch_not_writable")
    require(not Path("/var/run/docker.sock").exists(), "docker_socket_present")


def candidate_check(expected):
    require(re.fullmatch(r"[0-9a-f]{64}", expected) is not None, "invalid_candidate_digest")
    metadata = CANDIDATE.lstat()
    require(stat.S_ISREG(metadata.st_mode) and 64 <= metadata.st_size <= 268_435_456,
            "candidate_not_bounded_regular_file")
    digest = hashlib.sha256()
    with CANDIDATE.open("rb") as source:
        header = source.read(64)
        require(header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\xb7\x00",
                "candidate_not_arm64_elf")
        digest.update(header)
        for chunk in iter(lambda: source.read(1_048_576), b""):
            digest.update(chunk)
    require(digest.hexdigest() == expected, "candidate_digest_mismatch")


def validate_request(body):
    require(body.get("model") == MODEL, "wrong_model")
    require(body.get("stream") is True and body.get("store") is False,
            "wrong_stream_or_storage")
    require("tools" not in body or body["tools"] == [], "unexpected_tools")
    require(type(body.get("max_output_tokens")) is int and
            body["max_output_tokens"] == 800, "unbounded_output")
    instructions = body.get("instructions")
    prefix = SYSTEM + "\nCurrent date and time: "
    require(isinstance(instructions, str) and instructions.startswith(prefix),
            "unreviewed_system_context")
    date = instructions[len(prefix):]
    require(0 < len(date) <= 128 and date.isascii() and
            all(32 <= ord(char) < 127 for char in date) and "/" not in date,
            "unexpected_date_context")
    inputs = body.get("input")
    require(isinstance(inputs, list) and len(inputs) == 1 and
            isinstance(inputs[0], dict), "unexpected_conversation_context")
    message = inputs[0]
    require(set(message) <= {"role", "content", "type"} and
            message.get("type", "message") == "message" and
            message.get("role") == "user", "unexpected_message_metadata")
    require(message.get("content") in
            (QUESTION, [{"type": "input_text", "text": QUESTION}]),
            "unexpected_question")
    # No affinity IDs, previous response references or caller-supplied context.
    require(set(body) <= {"model", "stream", "store", "tools", "max_output_tokens",
                          "instructions", "input"},
            "unreviewed_request_field")


def parse_headers(raw):
    require(len(raw) <= 8_192, "http_header_bound")
    try:
        lines = raw.decode("ascii").split("\r\n")
    except UnicodeError:
        raise CheckFailed("http_header_encoding") from None
    require(lines[0] == "POST /v1/responses HTTP/1.1", "http_path")
    headers = {}
    for line in lines[1:]:
        if not line:
            continue
        key, colon, value = line.partition(":")
        require(colon and key.lower() not in headers, "duplicate_or_invalid_http_header")
        headers[key.lower()] = value.strip()
    require("transfer-encoding" not in headers, "http_transfer_encoding")
    require(headers.get("authorization") == "Bearer " + TOKEN, "http_fixture_auth")
    length = headers.get("content-length", "")
    require(length.isascii() and length.isdecimal() and 1 <= int(length) <= 65_536,
            "http_body_bound")
    return int(length)


def response_payload(status):
    if status == 200:
        events = [
            {"type": "response.output_text.delta", "item_id": "synthetic-message",
             "content_index": 0, "delta": ANSWER},
            {"type": "response.completed", "response": {
                "id": "synthetic-response", "status": "completed",
                "incomplete_details": None,
                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}},
        ]
        content_type = "text/event-stream"
        payload = "".join("data: " + json.dumps(event) + "\n\n" for event in events)
    else:
        content_type = "application/json"
        payload = '{"error":{"message":"synthetic rate limit","type":"rate_limit_error"}}'
    encoded = payload.encode()
    return (f"HTTP/1.1 {status} Synthetic\r\nContent-Type: {content_type}\r\n"
            f"Content-Length: {len(encoded)}\r\nConnection: close\r\n\r\n").encode() + encoded


class FakeProvider:
    def __init__(self, status):
        self.status = status
        self.calls = 0
        self.completed = 0
        self.failure = None
        self.tasks = set()

    async def handle(self, reader, writer):
        task = asyncio.current_task()
        self.tasks.add(task)
        try:
            self.calls += 1
            require(self.calls == 1, "unexpected_provider_retry")
            async with asyncio.timeout(5):
                header = await reader.readuntil(b"\r\n\r\n")
                body = await reader.readexactly(parse_headers(header))
                validate_request(decode_json(body))
                writer.write(response_payload(self.status))
                await writer.drain()
                self.completed += 1
        except CheckFailed as error:
            self.failure = error
        except (OSError, ValueError, asyncio.IncompleteReadError,
                asyncio.LimitOverrunError, TimeoutError):
            self.failure = CheckFailed("http_fixture_io")
        finally:
            writer.close()
            try:
                await asyncio.wait_for(writer.wait_closed(), timeout=1)
            except (OSError, TimeoutError):
                pass
            self.tasks.discard(task)


class Frames:
    def __init__(self, reader):
        self.reader = reader
        self.total = 0

    async def read(self, allow_eof=False):
        try:
            raw = await self.reader.readline()
        except ValueError:
            raise CheckFailed("stdout_frame_bound") from None
        if not raw and allow_eof:
            return None
        require(bool(raw), "unexpected_stdout_eof")
        self.total += len(raw)
        require(len(raw) <= MAX_FRAME and self.total <= MAX_STDOUT, "stdout_bound")
        require(raw.endswith(b"\n"), "unterminated_json_frame")
        return decode_json(raw, allow_provider_error_duplicate=True)


async def send(child, frame):
    encoded = (json.dumps(frame, separators=(",", ":")) + "\n").encode()
    require(len(encoded) <= 512, "rpc_input_bound")
    child.stdin.write(encoded)
    await child.stdin.drain()


def validate_reply(frame, request_id, command):
    require(frame.get("type") == "response" and frame.get("id") == request_id and
            frame.get("command") == command and frame.get("success") is True,
            "unexpected_rpc_reply")


def validate_state(state):
    require(isinstance(state, dict), "missing_rpc_state")
    model = state.get("model")
    require(isinstance(model, dict) and model.get("provider") == "openai" and
            model.get("id") == MODEL, "rpc_model_mismatch")
    require(state.get("thinkingLevel") == "off", "thinking_not_off")
    for key in ("autoRetryEnabled", "autoCompactionEnabled", "isStreaming", "isCompacting"):
        require(state.get(key) is False, "unsafe_rpc_state")
    for key in ("messageCount", "pendingMessageCount"):
        require(type(state.get(key)) is int and state[key] == 0, "nonempty_rpc_history")
    for key in ("sessionFile", "sessionName"):
        require(key in state and state[key] is None, "session_enabled")


async def protocol(child, status):
    frames = Frames(child.stdout)
    for index, command in enumerate(("set_auto_retry", "set_auto_compaction", "get_state"), 1):
        request_id = f"control-{index}"
        frame = {"id": request_id, "type": command}
        if index < 3:
            frame["enabled"] = False
        await send(child, frame)
        reply = await frames.read()
        validate_reply(reply, request_id, command)
        if index == 3:
            validate_state(reply.get("data"))
        else:
            require(reply.get("data") is None, "unexpected_control_data")
    await send(child, {"type": "prompt", "id": "synthetic-prompt", "message": QUESTION})
    acknowledged = False
    provider_error = False
    while True:
        frame = await frames.read()
        kind = frame.get("type")
        if kind == "response" and not acknowledged:
            validate_reply(frame, "synthetic-prompt", "prompt")
            acknowledged = True
        elif kind in ("agent_start", "turn_start", "message_start", "message_update",
                      "message_end", "turn_end"):
            pass
        elif kind == "provider_error" and status == 429 and not provider_error:
            provider_error = True
        elif kind == "agent_end" and acknowledged:
            require(provider_error == (status == 429) and
                    isinstance(frame.get("error"), str) == (status == 429),
                    "unexpected_provider_outcome")
            break
        else:
            raise CheckFailed("unexpected_generation_frame")
    await send(child, {"type": "get_last_assistant_text", "id": "synthetic-result"})
    reply = await frames.read()
    validate_reply(reply, "synthetic-result", "get_last_assistant_text")
    result = reply.get("data", {})
    require(isinstance(result, dict), "missing_answer_result")
    require(result.get("text") == ANSWER if status == 200 else
            result.get("text") in (None, ""), "unexpected_answer")
    child.stdin.close()
    require(await frames.read(allow_eof=True) is None, "unexpected_final_frame")
    require(await child.wait() == 0, "child_exit_failure")
    return frames.total


async def discard_stderr(reader):
    total = 0
    while chunk := await reader.read(1024):
        total += len(chunk)
        require(total <= MAX_STDERR, "stderr_bound")
    return total


def prepare(directory, port, tools_off):
    root = Path(directory)
    home, agent, cwd = root / "home", root / "agent", root / "cwd"
    for path in (home, agent, cwd, agent / "packages", agent / "sessions"):
        path.mkdir(mode=0o700)
    settings = {"retry": {"enabled": False, "maxRetries": 0, "fallbackChains": {},
                          "maxFailoversPerTurn": 0}, "compaction": {"enabled": False}}
    if tools_off:
        settings["tools"] = {"loadMode": dict.fromkeys(("manage_skill", "xdev", "submit_plan"), "off")}
    settings_path = agent / "settings.json"
    settings_path.write_text(json.dumps(settings))
    (agent / "models.json").write_text(json.dumps({"providers": {"openai": {
        "baseUrl": f"http://127.0.0.1:{port}/v1/responses", "api": "openai-responses",
        "modelOverrides": {MODEL: {"maxTokens": 800, "contextWindow": 16384}}}}}))
    env = {"HOME": str(home), "PATH": "/usr/bin:/bin", "LANG": "C.UTF-8",
           "TMPDIR": str(root), "PI_CODING_AGENT_DIR": str(agent),
           "PI_PACKAGE_DIR": str(agent / "packages"),
           "PI_SESSIONS_DIR": str(agent / "sessions"),
           "PI_CONFIG_PATH": str(settings_path), "OPENAI_API_KEY": TOKEN,
           "PI_CACHE_RETENTION": "none"}
    for name in ("CONFIG", "DATA", "STATE", "CACHE", "RUNTIME"):
        path = home / name.lower()
        path.mkdir(mode=0o700)
        env[f"XDG_{name}_{'DIR' if name == 'RUNTIME' else 'HOME'}"] = str(path)
    return env, cwd


async def run_case(status, tools_off):
    provider = FakeProvider(status)
    server = await asyncio.start_server(provider.handle, "127.0.0.1", 0, limit=8192)
    port = server.sockets[0].getsockname()[1]
    child = None
    stderr_task = None
    protocol_task = None
    try:
        with tempfile.TemporaryDirectory(prefix="pi-bookworm-", dir="/scratch") as directory:
            env, cwd = prepare(directory, port, tools_off)
            arguments = ["/usr/bin/qemu-aarch64", "-L", "/usr/aarch64-linux-gnu",
                         str(CANDIDATE), "--mode", "rpc", "--provider", "openai",
                         "--model", MODEL, "--thinking", "off", "--no-tools",
                         "--no-extensions", "--no-skills", "--no-context-files",
                         "--no-prompt-templates", "--no-themes", "--no-session",
                         "--no-migrations", "--max-tool-iterations", "1", "--max-time", "10",
                         "--request-timeout", "5", "--hide-cwd-in-prompt",
                         "--system-prompt", SYSTEM]
            try:
                async with asyncio.timeout(CASE_SECONDS):
                    child = await asyncio.create_subprocess_exec(
                        *arguments, env=env, cwd=cwd, start_new_session=True,
                        stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
                        stderr=asyncio.subprocess.PIPE, limit=MAX_FRAME)
                    stderr_task = asyncio.create_task(discard_stderr(child.stderr))
                    protocol_task = asyncio.create_task(protocol(child, status))
                    stdout_bytes, stderr_bytes = await asyncio.gather(protocol_task, stderr_task)
                    if provider.tasks:
                        await asyncio.gather(*list(provider.tasks))
                    if provider.failure:
                        raise provider.failure
                    require(provider.calls == provider.completed == 1, "provider_call_count")
            finally:
                # Reap before removing the isolated child's files on every path.
                if child is not None:
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    try:
                        await asyncio.wait_for(child.wait(), timeout=2)
                    except TimeoutError:
                        raise CheckFailed("child_reap_unconfirmed") from None
                for task in (stderr_task, protocol_task):
                    if task is not None and not task.done():
                        task.cancel()
                await asyncio.gather(*(task for task in (stderr_task, protocol_task)
                                       if task is not None), return_exceptions=True)
        return {"http_status": status, "explicit_tools_off": tools_off,
                "synthetic_provider_calls": provider.calls, "tool_declarations": 0,
                "stdout_bytes": stdout_bytes, "stderr_bytes_discarded": stderr_bytes,
                "process": "reaped", "result": "passed"}
    finally:
        server.close()
        await server.wait_closed()
        tasks = list(provider.tasks)
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


async def run_all():
    return [await run_case(status, off) for status, off in ((200, False), (200, True), (429, True))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-sha256", required=True,
                        help="Exact separately reviewed compatibility candidate hash; not a production pin")
    args = parser.parse_args()
    report = {"schema": "codifix.ca-bookworm.pi-smoke.v1", "limitation": LIMITATION,
              "real_provider_calls": 0, "real_credentials": False,
              "production_pins_changed": False, "native_ca_sandbox_proven": False}
    try:
        sandbox_check()
        candidate_check(args.candidate_sha256)
        report["candidate_sha256"] = args.candidate_sha256
        report["cases"] = asyncio.run(run_all())
        report["result"] = "passed"
    except CheckFailed as error:
        report.update(result="failed", failure=str(error))
    except TimeoutError:
        report.update(result="failed", failure="deadline")
    except (OSError, ValueError, RuntimeError, asyncio.IncompleteReadError):
        report.update(result="failed", failure="local_io_or_protocol_failure")
    print(json.dumps(report, sort_keys=True))
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
