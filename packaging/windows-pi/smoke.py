#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Native Pi transport/no-tools proof using the existing strict RPC validators.

Disposable Windows VM only, synthetic loopback provider only. This deliberately
does NOT claim AppContainer, inherited-broker or deployed-supervisor acceptance.
"""
import argparse
import asyncio
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile


def candidate_check(path, expected):
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("invalid candidate digest")
    with path.open("rb") as source:
        header = source.read(64)
        if len(header) != 64 or header[:2] != b"MZ":
            raise ValueError("native PE candidate required")
        offset = int.from_bytes(header[60:64], "little")
        if offset < 64 or offset > 1024 * 1024:
            raise ValueError("invalid PE header")
        source.seek(offset)
        if source.read(6) != b"PE\0\0\x64\x86":
            raise ValueError("native AMD64 PE required")
        source.seek(0)
        if hashlib.file_digest(source, "sha256").hexdigest() != expected:
            raise ValueError("candidate digest mismatch")


def protocol_library(path, expected):
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise ValueError("shared protocol library differs")
    spec = importlib.util.spec_from_file_location("codifix_pi_strict_protocol", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def windows_root():
    buffer = ctypes.create_unicode_buffer(32768)
    length = ctypes.windll.kernel32.GetWindowsDirectoryW(buffer, len(buffer))
    if not 0 < length < len(buffer):
        raise ValueError("Windows directory unavailable")
    return Path(buffer.value)


async def run_case(shared, candidate, scratch, status, tools_off, system):
    provider = shared.FakeProvider(status)
    server = await asyncio.start_server(provider.handle, "127.0.0.1", 0, limit=8192)
    port = server.sockets[0].getsockname()[1]
    child = None
    tasks = []
    try:
        with tempfile.TemporaryDirectory(prefix="native-pi-", dir=scratch) as directory:
            env, cwd = shared.prepare(directory, port, tools_off)
            # Keep only explicitly synthetic configuration. Never inherit user
            # model credentials, config, history or the broker-handle switch.
            env.update({"SystemRoot": str(system), "USERPROFILE": env["HOME"],
                        "PATH": str(candidate.parent) + ";" + str(system / "System32"),
                        "TEMP": directory, "TMP": directory})
            for name in ("APPDATA", "LOCALAPPDATA"):
                path = Path(directory) / name.lower()
                path.mkdir()
                env[name] = str(path)
            arguments = [str(candidate), "--mode", "rpc", "--provider", "openai",
                         "--model", shared.MODEL, "--thinking", "off", "--no-tools",
                         "--no-extensions", "--no-skills", "--no-context-files",
                         "--no-prompt-templates", "--no-themes", "--no-session",
                         "--no-migrations", "--max-tool-iterations", "1", "--max-time", "10",
                         "--request-timeout", "5", "--hide-cwd-in-prompt", "--system-prompt", shared.SYSTEM]
            try:
                async with asyncio.timeout(shared.CASE_SECONDS):
                    child = await asyncio.create_subprocess_exec(
                        *arguments, env=env, cwd=cwd,
                        creationflags=subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.CREATE_NO_WINDOW,
                        stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
                        stderr=asyncio.subprocess.PIPE, limit=shared.MAX_FRAME)
                    tasks = [asyncio.create_task(shared.protocol(child, status)),
                             asyncio.create_task(shared.discard_stderr(child.stderr))]
                    stdout_bytes, stderr_bytes = await asyncio.gather(*tasks)
                    if provider.tasks:
                        await asyncio.gather(*list(provider.tasks))
                    if provider.failure:
                        raise provider.failure
                    shared.require(provider.calls == provider.completed == 1, "provider_call_count")
            finally:
                if child is not None and child.returncode is None:
                    killer = await asyncio.create_subprocess_exec(
                        str(system / "System32" / "taskkill.exe"), "/PID", str(child.pid), "/T", "/F",
                        stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.DEVNULL,
                        stderr=asyncio.subprocess.DEVNULL,
                        env={"SystemRoot": str(system), "PATH": str(system / "System32")})
                    await asyncio.wait_for(killer.wait(), 5)
                    await asyncio.wait_for(child.wait(), 5)
                for task in tasks:
                    if not task.done():
                        task.cancel()
                await asyncio.gather(*tasks, return_exceptions=True)
        return {"http_status": status, "explicit_tools_off": tools_off,
                "synthetic_provider_calls": provider.calls, "tool_declarations": 0,
                "stdout_bytes": stdout_bytes, "stderr_bytes_discarded": stderr_bytes,
                "process": "reaped", "result": "passed"}
    finally:
        server.close()
        await server.wait_closed()
        for task in list(provider.tasks):
            task.cancel()
        await asyncio.gather(*list(provider.tasks), return_exceptions=True)


async def run_all(shared, candidate, scratch, system):
    return [await run_case(shared, candidate, scratch, status, off, system)
            for status, off in ((200, False), (200, True), (429, True))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--candidate-sha256", required=True)
    parser.add_argument("--protocol-library", type=Path, required=True)
    parser.add_argument("--protocol-sha256", required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt" or platform.machine().upper() not in {"AMD64", "X86_64"} or os.environ.get("COMPUTERNAME") != "MEMORY-WINLAB":
        raise ValueError("disposable native AMD64 Windows lab required")
    candidate_check(args.candidate, args.candidate_sha256)
    shared = protocol_library(args.protocol_library, args.protocol_sha256)
    report = {"format": "codifix-windows-pi-smoke-v1", "binary_sha256": args.candidate_sha256,
              "real_provider_calls": 0, "real_credentials": False, "native_sandbox_verified": False,
              "inherited_broker_verified": False, "deployed_supervisor_verified": False}
    report["cases"] = asyncio.run(run_all(shared, args.candidate, args.scratch, windows_root()))
    report["result"] = "passed"
    print(json.dumps(report))


if __name__ == "__main__":
    main()
