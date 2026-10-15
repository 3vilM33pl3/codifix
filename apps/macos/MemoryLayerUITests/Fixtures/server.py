#!/usr/bin/env python3
"""Loopback-only, in-memory HTTP fixtures for native acceptance checks.

Never proxies to a real service. All mutations disappear when this process exits.
Run with --port 4261, then connect the app to that address and synthetic-project.
POST /__scenario {"name":"empty|normal|insufficient|failed|restricted|auth|loading"}
changes test responses. GET /__requests returns method/path only, never secrets.
"""
import argparse
import copy
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).parent
STAMP = "2026-10-15T10:00:00Z"
PROJECT = "synthetic-project"


def fixture(name):
    return json.loads((ROOT / (name + ".json")).read_text())


class State:
    scenario = "normal"
    requests = []
    proposals = fixture("proposals")["proposals"]
    approvals = [{"id": "approval-1", "run_id": None, "project": PROJECT,
                  "loop_id": "loop / one", "action_type": "update",
                  "proposed_action": {"summary": "Fixture proposal for review"},
                  "risk_reason": "Illustrative fixture; no command is executed.",
                  "status": "pending", "created_at": STAMP}]
    tokens = []
    memberships = []


def principal():
    return {"id": "fixture-admin", "kind": "human_oidc", "display_name": "Fixture administrator",
            "email": "fixture@example.test", "groups": [], "global_role": "admin", "projects": []}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, payload, status=200, headers=None):
        raw = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        for name, value in headers or []:
            self.send_header(name, value)
        self.end_headers()
        try:
            self.wfile.write(raw)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        self.handle_request()

    def do_POST(self):
        self.handle_request()

    def do_PUT(self):
        self.handle_request()

    def do_DELETE(self):
        self.handle_request()

    def handle_request(self):
        path = unquote(urlsplit(self.path).path)
        body = self.rfile.read(min(int(self.headers.get("Content-Length", 0)), 2 * 1024 * 1024))
        try:
            data = json.loads(body) if body else {}
        except ValueError:
            data = {}
        if path == "/__requests":
            return self.reply(State.requests)
        if path == "/__scenario":
            State.scenario = data.get("name", "normal")
            if data.get("reset"):
                State.proposals = fixture("proposals")["proposals"]
                for approval in State.approvals:
                    approval["status"] = "pending"
                    approval["proposed_action"] = {"summary": "Fixture proposal for review"}
                State.requests = []
            return self.reply({"scenario": State.scenario})
        State.requests.append({"method": self.command, "path": path})
        if State.scenario == "loading":
            time.sleep(3)
        if State.scenario in ("failed", "auth", "restricted") and path != "/healthz":
            status = {"failed": 500, "auth": 401, "restricted": 403}[State.scenario]
            return self.reply({"error": {"code": "fixture", "message": "Explicit fixture failure"}}, status)
        if path == "/healthz":
            return self.reply({"version": "fixture-dev", "status": "ok"})
        if path == "/v1/auth/me":
            return self.reply({"authenticated": True, "mode": "single_user", "read_only": False,
                               "principal": principal()})
        if path == "/v1/auth/session/bootstrap":
            return self.reply({"authenticated": True}, headers=[
                ("Set-Cookie", "memory_session=fixture-session; Path=/; HttpOnly; SameSite=Lax"),
                ("Set-Cookie", "memory_csrf=fixture-csrf; Path=/; SameSite=Lax")])
        if path == "/v1/agents":
            return self.reply({"collected_at": STAMP, "sessions": [{"session_id": "fixture-agent", "status": "working"}],
                               "orphan_ports": [], "rate_limits": []})
        if path == "/v1/config/llm-audit":
            return self.reply({"enabled": False, "redacted": True, "profile": "Fixture", "max_message_chars": 8000, "max_total_chars": 32000})
        if path.endswith("/activities"):
            return self.reply(fixture("activity"))
        if path.endswith("/up-to-speed"):
            return self.reply(fixture("briefing"))
        if path.endswith("/resume"):
            return self.reply(fixture("resume"))
        if path.endswith("/overview"):
            return self.reply(fixture("overview"))
        if path == "/v1/skills":
            return self.reply(fixture("skills"))
        if path.startswith("/v1/skills/"):
            return self.reply(fixture("skill"))
        if path == "/v1/embeddings/backends":
            result = fixture("backends")
            if State.scenario == "backend-unavailable":
                result["backends"][0]["ready"] = False
                result["backends"][0]["active"] = False
                result["active"] = None
            return self.reply(result)
        if path.endswith("/memories"):
            result = fixture("list")
            if State.scenario == "empty":
                result.update(items=[], total=0)
            return self.reply(result)
        if path.startswith("/v1/memory/"):
            return self.reply(fixture("history" if path.endswith("/history") else "detail"))
        if path == "/v1/query":
            result = fixture("query")
            if State.scenario == "insufficient":
                result.update(answer="There is insufficient evidence for this question.",
                              insufficient_evidence=True, results=[], answer_citations=[])
            return self.reply(result)
        if path.endswith("/replacement-policy"):
            return self.reply({"project": PROJECT, "replacement_policy": "balanced",
                               "repo_root": "/synthetic/repo", "writable": True})
        if path.endswith("/replacement-proposals"):
            return self.reply({"project": PROJECT, "proposals": State.proposals})
        if "/replacement-proposals/" in path:
            p = fixture("proposals")["proposals"][0]
            State.proposals = []
            return self.reply({"project": PROJECT, "proposal_id": p["id"],
                               "status": "approved" if path.endswith("/approve") else "rejected",
                               "policy": "balanced", "target_memory_id": p["target_memory_id"],
                               "target_summary": p["target_summary"], "candidate_summary": p["candidate_summary"],
                               "new_memory_id": None})
        if path == "/v1/loops":
            return self.reply(fixture("loops"))
        if path == "/v1/loops/global-kill-switch":
            return self.reply({"kill_switch_enabled": False, "updated_at": STAMP})
        if path == "/v1/loops/runs":
            return self.reply(fixture("runs"))
        if path.startswith("/v1/loops/runs/") or path.endswith("/run"):
            if self.command == "POST" and data.get("dry_run") is not True:
                return self.reply({"error": "Only dry-run fixture requests allowed"}, 400)
            return self.reply(fixture("run"))
        if path == "/v1/loops/approvals":
            return self.reply({"approvals": [x for x in State.approvals if x["status"] == "pending"], "total_returned": 1})
        if path.startswith("/v1/loops/approvals/"):
            item = State.approvals[0]
            action = path.rsplit("/", 1)[-1]
            if action == "edit":
                item["proposed_action"] = data.get("edited_action", item["proposed_action"])
            else:
                item["status"] = "approved" if action == "approve" else "rejected"
            return self.reply({"approval": item})
        if path == "/v1/loops/memory-proposals":
            return self.reply({"proposals": [], "total_returned": 0})
        if path.startswith("/v1/loops/"):
            return self.reply(fixture("loop_detail"))
        if "/bundle/" in path:
            if path.endswith("/export/preview"):
                result = fixture("export_preview"); result["options"] = data
                return self.reply(result)
            if path.endswith("/export"):
                return self.reply((ROOT / "sample.mlbundle.zip").read_bytes())
            return self.reply(fixture("import_preview" if path.endswith("/preview") else "import_result"))
        if path == "/v1/auth/principals":
            return self.reply([principal()])
        if path == "/v1/auth/tokens":
            if self.command == "GET":
                return self.reply(State.tokens)
            token = {"id": "fixture-token-1", "principal_id": "fixture-service", "name": data.get("name", "Fixture"),
                     "token_prefix": "fixture_only", "created_at": STAMP,
                     "projects": [{"project": data.get("project", PROJECT), "role": data.get("role", "reader"), "source": "direct"}]}
            State.tokens.append(copy.deepcopy(token))
            token["token"] = "DUMMY_FIXTURE_TOKEN_NOT_VALID_FOR_ANY_SERVICE"
            return self.reply(token)
        if path.endswith("/revoke") and path.startswith("/v1/auth/tokens/"):
            for token in State.tokens:
                token["revoked_at"] = STAMP
            return self.reply(State.tokens[0])
        if path == "/v1/auth/memberships":
            if self.command == "GET":
                return self.reply(State.memberships)
            item = dict(data, id="fixture-membership", source="direct", created_at=STAMP)
            State.memberships.append(item)
            return self.reply(item)
        if path.startswith("/v1/auth/memberships/"):
            item = State.memberships[0]
            State.memberships = []
            return self.reply(item)
        return self.reply({"error": {"message": "No acceptance fixture for " + path}}, 404)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=4261)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Fixture service http://127.0.0.1:{server.server_port} project={PROJECT}", flush=True)
    server.serve_forever()
