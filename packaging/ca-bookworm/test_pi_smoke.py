#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Offline fake-only tests; never execute Pi or contact a real provider."""

import asyncio
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("pi_smoke", Path(__file__).with_name("pi_smoke.py"))
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)


def request():
    return {"model": smoke.MODEL, "stream": True, "store": False, "max_output_tokens": 800,
            "instructions": smoke.SYSTEM + "\nCurrent date and time: 2026-09-20T12:00:00Z",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": smoke.QUESTION}]}]}


def state():
    return {"model": {"provider": "openai", "id": smoke.MODEL}, "thinkingLevel": "off",
            "autoRetryEnabled": False, "autoCompactionEnabled": False, "isStreaming": False,
            "isCompacting": False, "messageCount": 0, "pendingMessageCount": 0,
            "sessionFile": None, "sessionName": None}


def headers(body=b"{}", extra=b"", path=b"/v1/responses"):
    return (b"POST " + path + b" HTTP/1.1\r\nAuthorization: Bearer " + smoke.TOKEN.encode() +
            b"\r\nContent-Length: " + str(len(body)).encode() + b"\r\n" + extra + b"\r\n")


class Contracts(unittest.TestCase):
    def test_reviewed_synthetic_request_and_empty_tools(self):
        body = request()
        smoke.validate_request(body)
        body["tools"] = []
        smoke.validate_request(body)
        body["input"][0]["content"] = smoke.QUESTION
        smoke.validate_request(body)

    def test_unreviewed_request_fields_rejected(self):
        changes = {"model": "another-model", "store": True, "stream": False,
                   "max_output_tokens": 801, "previous_response_id": "anything",
                   "prompt_cache_key": "anything", "metadata": {"private": "context"},
                   "reasoning": {"effort": "high"}, "parallel_tool_calls": True,
                   "tool_choice": "auto", "temperature": 0.5}
        for key, value in changes.items():
            with self.subTest(key=key):
                body = request()
                body[key] = value
                with self.assertRaises(smoke.CheckFailed):
                    smoke.validate_request(body)

    def test_tools_wrong_types_and_regression_rejected(self):
        for tools in (None, {}, "off", [{"name": "xdev"}], [{"name": "submit_plan"}]):
            with self.subTest(tools=tools):
                body = request()
                body["tools"] = tools
                with self.assertRaises(smoke.CheckFailed):
                    smoke.validate_request(body)

    def test_context_and_history_rejected(self):
        for inputs in ([], [{"role": "system", "content": smoke.QUESTION}],
                       [{"role": "user", "content": "other question"}],
                       request()["input"] * 2,
                       [{"role": "user", "content": smoke.QUESTION, "private": "context"}],
                       [{"role": "user", "content": [{"type": "input_file", "file_id": "x"}]}]):
            body = request()
            body["input"] = inputs
            with self.assertRaises(smoke.CheckFailed):
                smoke.validate_request(body)

    def test_system_context_exact_except_bounded_date(self):
        for instructions in (None, smoke.SYSTEM, "other system prompt",
                             request()["instructions"] + "\nlocal context",
                             request()["instructions"] + "/home/user",
                             request()["instructions"] + "x" * 129):
            body = request()
            body["instructions"] = instructions
            with self.assertRaises(smoke.CheckFailed):
                smoke.validate_request(body)

    def test_duplicate_json_and_nonfinite_rejected(self):
        for raw in (b'{"store":true,"store":false}', b'{"max_output_tokens":NaN}',
                    b'{"x":Infinity}', b'{"x":{"secret":1,"secret":2}}', b'[]', b'\xff'):
            with self.assertRaises(smoke.CheckFailed):
                smoke.decode_json(raw)

    def test_only_exact_upstream_provider_error_diagnostic_duplicate_allowed(self):
        raw = b'{"type":"provider_error","provider":"openai","provider":"openai"}'
        self.assertEqual(smoke.decode_json(raw, allow_provider_error_duplicate=True),
                         {"type": "provider_error", "provider": "openai"})
        with self.assertRaises(smoke.CheckFailed):
            smoke.decode_json(raw)
        for rejected in (
                raw.replace(b'"provider_error"', b'"response"'),
                b'{"type":"provider_error","provider":"openai","provider":"other"}',
                b'{"type":"provider_error","provider":"other","provider":"other"}',
                b'{"type":"provider_error","provider":"openai","provider":"openai","provider":"openai"}',
                b'{"type":"provider_error","type":"provider_error","provider":"openai","provider":"openai"}',
                b'{"type":"provider_error","nested":{"provider":"openai","provider":"openai"}}',
                b'{"type":"provider_error","provider":"openai","provider":"openai","model":"x","model":"x"}',
                b'{"type":"response","data":{"model":{"provider":"openai","provider":"openai"}}}'):
            with self.subTest(raw=rejected), self.assertRaises(smoke.CheckFailed):
                smoke.decode_json(rejected, allow_provider_error_duplicate=True)

    def test_http_headers_reject_ambiguous_framing(self):
        self.assertEqual(smoke.parse_headers(headers()), 2)
        for raw in (headers(extra=b"Content-Length: 2\r\n"),
                    headers(extra=b"Transfer-Encoding: chunked\r\n"),
                    headers(extra=b"Authorization: Bearer other\r\n"),
                    headers(path=b"/unreviewed"),
                    headers().replace(b"Content-Length: 2", b"Content-Length: 65537"),
                    headers().replace(smoke.TOKEN.encode(), b"real-looking-but-not-approved"),
                    b"x" * 8193):
            with self.assertRaises(smoke.CheckFailed):
                smoke.parse_headers(raw)

    def test_state_empty_and_controls_disabled(self):
        smoke.validate_state(state())
        for key, value in (("autoRetryEnabled", True), ("autoCompactionEnabled", True),
                           ("thinkingLevel", "high"), ("messageCount", 1),
                           ("messageCount", False), ("sessionName", "existing")):
            body = state()
            body[key] = value
            with self.assertRaises(smoke.CheckFailed):
                smoke.validate_state(body)
        for key in state():
            body = state()
            del body[key]
            with self.assertRaises(smoke.CheckFailed):
                smoke.validate_state(body)

    def test_root_readonly_does_not_mask_writable_candidate_mount(self):
        mounts = "1 0 0:1 / / ro - overlay overlay ro\n2 1 0:2 / /candidate rw - tmpfs tmpfs rw\n"
        self.assertTrue(smoke.readonly_mount("/", mounts))
        self.assertFalse(smoke.readonly_mount("/candidate/pi", mounts))
        self.assertTrue(smoke.readonly_mount("/candidate-other/pi", mounts))

    def test_sandbox_marker_missing_fails_before_execution(self):
        with patch.dict(os.environ, {}, clear=True), self.assertRaisesRegex(
                smoke.CheckFailed, "sandbox_marker_missing"):
            smoke.sandbox_check()

    def test_candidate_hash_and_elf_architecture(self):
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "pi"
            header = bytearray(64)
            header[:6] = b"\x7fELF\x02\x01"
            header[18:20] = b"\xb7\x00"
            candidate.write_bytes(header)
            digest = smoke.hashlib.sha256(header).hexdigest()
            with patch.object(smoke, "CANDIDATE", candidate):
                smoke.candidate_check(digest)
                with self.assertRaisesRegex(smoke.CheckFailed, "candidate_digest_mismatch"):
                    smoke.candidate_check("0" * 64)
                with self.assertRaisesRegex(smoke.CheckFailed, "invalid_candidate_digest"):
                    smoke.candidate_check("not-a-digest")
                header[18:20] = b"\x3e\x00"
                candidate.write_bytes(header)
                with self.assertRaisesRegex(smoke.CheckFailed, "candidate_not_arm64_elf"):
                    smoke.candidate_check(digest)

    def test_isolated_settings_and_synthetic_credentials_only(self):
        with tempfile.TemporaryDirectory() as directory:
            env, cwd = smoke.prepare(directory, 12345, True)
            self.assertEqual(env["OPENAI_API_KEY"], smoke.TOKEN)
            self.assertNotIn("HTTP_PROXY", env)
            self.assertNotIn("QEMU_LD_PREFIX", env)
            self.assertNotIn("LD_PRELOAD", env)
            self.assertEqual(list(cwd.iterdir()), [])
            settings = json.loads(Path(env["PI_CONFIG_PATH"]).read_text())
            self.assertEqual(settings["retry"]["maxRetries"], 0)
            self.assertFalse(settings["compaction"]["enabled"])
            self.assertEqual(set(settings["tools"]["loadMode"].values()), {"off"})
            models = json.loads((Path(env["PI_CODING_AGENT_DIR"]) / "models.json").read_text())
            self.assertEqual(models["providers"]["openai"]["baseUrl"],
                             "http://127.0.0.1:12345/v1/responses")
            self.assertEqual(env["PI_CACHE_RETENTION"], "none")


class AsyncContracts(unittest.IsolatedAsyncioTestCase):
    async def test_rpc_handshake_prompt_and_sanitized_result(self):
        for status in (200, 429):
            child = FixtureChild(status)
            total = await smoke.protocol(child, status)
            self.assertGreater(total, 0)
            self.assertTrue(child.stdin.closed)
            self.assertEqual([frame["type"] for frame in child.sent],
                             ["set_auto_retry", "set_auto_compaction", "get_state",
                              "prompt", "get_last_assistant_text"])

    async def test_rpc_unexpected_tool_execution_fails(self):
        child = FixtureChild(200, tool_event=True)
        with self.assertRaisesRegex(smoke.CheckFailed, "unexpected_generation_frame"):
            await smoke.protocol(child, 200)

    async def test_rpc_wrong_correlated_reply_fails(self):
        child = FixtureChild(200, wrong_id=True)
        with self.assertRaisesRegex(smoke.CheckFailed, "unexpected_rpc_reply"):
            await smoke.protocol(child, 200)

    async def test_jsonl_frames_eof_and_bounds(self):
        for raw in (b'{"type":"okay"}\n', b"x" * (smoke.MAX_FRAME + 1), b'{"x":1}'):
            reader = asyncio.StreamReader(limit=smoke.MAX_FRAME)
            reader.feed_data(raw)
            reader.feed_eof()
            frames = smoke.Frames(reader)
            if raw.startswith(b'{"type"'):
                self.assertEqual((await frames.read())["type"], "okay")
                self.assertIsNone(await frames.read(allow_eof=True))
            else:
                with self.assertRaises(smoke.CheckFailed):
                    await frames.read()

    async def test_jsonl_total_bound(self):
        reader = asyncio.StreamReader()
        reader.feed_data(b'{"x":1}\n')
        frames = smoke.Frames(reader)
        frames.total = smoke.MAX_STDOUT
        with self.assertRaisesRegex(smoke.CheckFailed, "stdout_bound"):
            await frames.read()

    async def test_stderr_is_bounded_and_discarded(self):
        reader = asyncio.StreamReader()
        reader.feed_data(b"synthetic private text")
        reader.feed_eof()
        self.assertEqual(await smoke.discard_stderr(reader), 22)
        reader = asyncio.StreamReader()
        reader.feed_data(b"x" * (smoke.MAX_STDERR + 1))
        reader.feed_eof()
        with self.assertRaisesRegex(smoke.CheckFailed, "stderr_bound"):
            await smoke.discard_stderr(reader)

    async def test_fake_provider_success_and_429_and_retry_detection(self):
        for status in (200, 429):
            provider = smoke.FakeProvider(status)
            server = await asyncio.start_server(provider.handle, "127.0.0.1", 0, limit=8192)
            try:
                port = server.sockets[0].getsockname()[1]
                body = json.dumps(request()).encode()
                reader, writer = await asyncio.open_connection("127.0.0.1", port)
                writer.write(headers(body) + body)
                await writer.drain()
                reply = await asyncio.wait_for(reader.read(), timeout=2)
                writer.close()
                await writer.wait_closed()
                self.assertTrue(reply.startswith(f"HTTP/1.1 {status}".encode()))
                self.assertEqual(provider.calls, 1)
                self.assertEqual(provider.completed, 1)
                self.assertIsNone(provider.failure)
                reader, writer = await asyncio.open_connection("127.0.0.1", port)
                await asyncio.wait_for(reader.read(), timeout=2)
                writer.close()
                await writer.wait_closed()
                self.assertEqual(str(provider.failure), "unexpected_provider_retry")
            finally:
                server.close()
                await server.wait_closed()
                await asyncio.gather(*list(provider.tasks))

    async def test_fake_provider_invalid_tools_never_returns_success(self):
        provider = smoke.FakeProvider(200)
        server = await asyncio.start_server(provider.handle, "127.0.0.1", 0, limit=8192)
        try:
            body = request()
            body["tools"] = [{"name": "forbidden"}]
            body = json.dumps(body).encode()
            reader, writer = await asyncio.open_connection("127.0.0.1", server.sockets[0].getsockname()[1])
            writer.write(headers(body) + body)
            await writer.drain()
            self.assertEqual(await asyncio.wait_for(reader.read(), timeout=2), b"")
            self.assertEqual(str(provider.failure), "unexpected_tools")
            self.assertEqual(provider.completed, 0)
            writer.close()
            await writer.wait_closed()
        finally:
            server.close()
            await server.wait_closed()
            await asyncio.gather(*list(provider.tasks))


class FixtureChild:
    """In-memory protocol peer; neither Pi nor any subprocess is launched."""

    def __init__(self, status, tool_event=False, wrong_id=False):
        self.stdout = asyncio.StreamReader(limit=smoke.MAX_FRAME)
        self.stdin = FixtureInput(self)
        self.status = status
        self.sent = []
        self.tool_event = tool_event
        self.wrong_id = wrong_id

    async def wait(self):
        return 0

    def emit(self, frame):
        self.stdout.feed_data(json.dumps(frame).encode() + b"\n")


class FixtureInput:
    def __init__(self, child):
        self.child = child
        self.closed = False

    def write(self, encoded):
        child = self.child
        frame = json.loads(encoded)
        child.sent.append(frame)
        command = frame["type"]
        reply = {"type": "response", "id": "wrong" if child.wrong_id else frame["id"],
                 "command": command, "success": True}
        if command == "get_state":
            reply["data"] = state()
        elif command == "get_last_assistant_text":
            reply["data"] = {"text": smoke.ANSWER if child.status == 200 else None}
        child.emit(reply)
        if command == "prompt":
            if child.tool_event:
                child.emit({"type": "tool_execution_start"})
            child.emit({"type": "agent_start"})
            if child.status == 429:
                child.emit({"type": "provider_error", "message": "synthetic error body"})
            child.emit({"type": "agent_end", "error": "synthetic error" if child.status == 429 else None})

    async def drain(self):
        pass

    def close(self):
        self.closed = True
        self.child.stdout.feed_eof()


if __name__ == "__main__":
    unittest.main()
