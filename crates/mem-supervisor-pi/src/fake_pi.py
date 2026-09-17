# Synthetic executable body for unit tests only. MODE and PID_FILE are prepended
# by the test fixture; this file is never an adapter runtime dependency.
import json
import os
from pathlib import Path
import sys
import time

SENTINEL = "DO_NOT_LEAK_PROVIDER_CREDENTIAL_OR_PRIVATE_TEXT"
Path(PID_FILE).write_text(str(os.getpid()))

allowed = {"HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME",
           "XDG_RUNTIME_DIR", "TMPDIR", "PI_CODING_AGENT_DIR", "PI_PACKAGE_DIR", "PI_SESSIONS_DIR",
           "PI_CONFIG_PATH", "PATH", "LANG"}
assert set(os.environ) == allowed, "ambient environment leaked"
assert os.environ["PATH"] == "/usr/bin:/bin"
root = Path(os.environ["TMPDIR"])
assert root.stat().st_mode & 0o7777 == 0o700
for key in allowed - {"PATH", "LANG"}:
    assert Path(os.environ[key]).is_relative_to(root), "ambient resource path leaked"
assert Path.cwd() == root / "workspace"
assert list(Path.cwd().iterdir()) == []
assert not (Path(os.environ["PI_CODING_AGENT_DIR"]) / "auth.json").exists()
settings = json.loads(Path(os.environ["PI_CONFIG_PATH"]).read_text())
assert settings == {"retry": {"enabled": False, "maxRetries": 0, "fallbackChains": {}, "maxFailoversPerTurn": 0},
                    "compaction": {"enabled": False}}
assert sys.argv[1:] == ["--mode", "rpc", "--provider", "openai", "--model", "gpt-5.6-sol",
                       "--thinking", "off", "--no-tools", "--no-extensions", "--no-skills", "--no-context-files",
                       "--no-prompt-templates", "--no-themes", "--no-session", "--no-migrations",
                       "--max-tool-iterations", "1", "--max-time", "10", "--request-timeout", "5", "--hide-cwd-in-prompt"]

if MODE == "timeout_before_response":
    time.sleep(30)
if MODE == "stderr_limit":
    sys.stderr.write(SENTINEL * 1000)
    sys.stderr.flush()
    time.sleep(30)
if MODE == "stdout_limit":
    sys.stdout.write("x" * 20000)
    sys.stdout.flush()
    time.sleep(30)

commands = ["set_auto_retry", "set_auto_compaction", "get_state"]
for index, expected in enumerate(commands):
    request = json.loads(sys.stdin.readline())
    expected_request = {"id": f"control-{index + 1}", "type": expected}
    if index < 2:
        expected_request["enabled"] = False
    assert request == expected_request, "unexpected RPC or prompt supplied"
    if MODE == "slow_handshake":
        time.sleep(0.12)
    if index == 0:
        if MODE == "malformed":
            print("{invalid " + SENTINEL, flush=True)
            time.sleep(30)
        if MODE == "invalid_utf8":
            sys.stdout.buffer.write(b"\xff\n")
            sys.stdout.buffer.flush()
            time.sleep(30)
        if MODE == "unterminated":
            sys.stdout.write('{"type":"response"}')
            sys.stdout.flush()
            sys.exit(0)
        if MODE == "duplicate":
            print('{"type":"response","id":"control-1","command":"set_auto_retry","success":false,"success":true}', flush=True)
            time.sleep(30)
        if MODE == "eof":
            sys.exit(0)
        if MODE == "startup_error":
            print(json.dumps({"type": "error", "phase": "startup", "code": "auth.missing_api_key", "message": SENTINEL}), flush=True)
            sys.stderr.write(SENTINEL)
            sys.stderr.flush()
            sys.exit(1)
        if MODE == "terminal_error":
            print(json.dumps({"type": "error", "phase": "run", "code": "internal", "message": SENTINEL}), flush=True)
            sys.exit(1)
    response = {"type": "response", "id": request["id"], "command": expected, "success": True}
    if index == 0 and MODE == "uncorrelated":
        response["id"] = "unrelated-request"
    if index == 0 and MODE == "wrong_command":
        response["command"] = "prompt"
    if index == 0 and MODE == "unrequested_event":
        response["type"] = "agent_start"
    if index == 0 and MODE == "rejected":
        response.update(success=False, error=SENTINEL)
    if index == 1 and MODE == "replayed":
        response.update(id="control-1", command="set_auto_retry")
    if index == 2:
        state = {"model": {"provider": "openai", "id": "gpt-5.6-sol"},
                 "autoRetryEnabled": False, "autoCompactionEnabled": False,
                 "isStreaming": False, "isCompacting": False,
                 "messageCount": 0, "pendingMessageCount": 0, "sessionFile": None, "sessionName": None,
                 "sessionId": SENTINEL, "thinkingLevel": "off", "durabilityMode": "balanced"}
        if MODE == "wrong_model":
            state["model"]["id"] = "gpt-5.4"
        if MODE == "wrong_provider":
            state["model"]["provider"] = "other"
        if MODE == "thinking_enabled":
            state["thinkingLevel"] = "high"
        for mode, field in [("retry_enabled", "autoRetryEnabled"), ("compaction_enabled", "autoCompactionEnabled"),
                            ("streaming", "isStreaming"), ("compacting", "isCompacting")]:
            if MODE == mode:
                state[field] = True
        for mode, field in [("context_loaded", "messageCount"), ("pending", "pendingMessageCount")]:
            if MODE == mode:
                state[field] = 1
        if MODE == "session_file":
            state["sessionFile"] = SENTINEL
        if MODE == "session_name":
            state["sessionName"] = SENTINEL
        if MODE == "missing_state":
            del state["autoRetryEnabled"]
        response["data"] = state
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()

assert sys.stdin.read() == "", "extra RPC after the control handshake"
if MODE == "trailing_event":
    print(json.dumps({"type": "agent_start", "private": SENTINEL}), flush=True)
sys.stderr.write(SENTINEL)
sys.stderr.flush()
if MODE == "linger_after_eof":
    os.close(1)
    os.close(2)
    time.sleep(30)
if MODE == "exit_failure":
    sys.exit(9)
