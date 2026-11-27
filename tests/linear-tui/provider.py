"""Loopback-only GraphQL stand-in for the full Linear installer/TUI rehearsal."""
import json
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import sys

PROJECT = "e3eef73c-065a-4ac0-9f43-a50411507ef4"
TEAM = "dbac4c6d-0a2a-4401-a7e5-d14f97d9a1b6"
STATE = Path(sys.argv[1])
READY = Path(sys.argv[2])


class Graphql(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        assert self.path == "/graphql"
        assert self.headers.get("Authorization") == "synthetic-linear-key"
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        query = request["query"]
        variables = request.get("variables") or {}
        issues = json.loads(STATE.read_text())
        if query.startswith("query Projects"):
            result = {"projects": {"nodes": [{"id": PROJECT, "name": "Codifix", "archivedAt": None,
                       "teams": {"nodes": [{"id": TEAM, "name": "Engineering"}]}}],
                       "pageInfo": {"hasNextPage": False, "endCursor": None}}}
        elif query.startswith("query Project("):
            assert variables["id"] == PROJECT
            result = {"project": {"id": PROJECT, "teams": {"nodes": [{"id": TEAM}]}}}
        elif query.startswith("query AgencyFindIssue") or query.startswith("query AgencyScopedIssue"):
            filt = variables["filter"]
            assert filt["project"]["id"]["eq"] == PROJECT and filt["team"]["id"]["eq"] == TEAM
            if "title" in filt:
                rows = [i for i in issues.values() if i["title"].casefold() == filt["title"]["eqIgnoreCase"].casefold()]
            else:
                issue = issues.get(filt["id"]["eq"])
                rows = [] if issue is None else [issue]
            result = {"issues": {"nodes": rows}}
        elif query.startswith("mutation AgencyWrite"):
            assert "issueCreate(" in query
            value = variables["input"]
            assert value["projectId"] == PROJECT and value["teamId"] == TEAM
            assert value["useDefaultTemplate"] is False and not issues
            issue_id = value["id"]
            assert issue_id
            issue = {"id": issue_id, "title": value["title"], "description": value["description"],
                     "project": {"id": PROJECT}, "team": {"id": TEAM}, "projectMilestone": None, "assignee": None,
                     "priority": 0, "labelIds": [], "dueDate": None, "state": None, "cycle": None, "parent": None,
                     "updatedAt": "2026-11-25T00:00:00Z",
                     "url": "https://linear.app/example/issue/COD-42/terminal-workflow"}
            temp = STATE.with_suffix(".new")
            temp.write_text(json.dumps({issue_id: issue}))
            temp.replace(STATE)
            result = {"effect": {"success": True, "issue": {"id": issue_id}}}
        else:
            raise AssertionError("Unexpected GraphQL operation: " + query)
        response = json.dumps({"data": result}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)


server = ThreadingHTTPServer(("127.0.0.1", 0), Graphql)
STATE.write_text("{}")
READY.write_text(f"http://127.0.0.1:{server.server_port}/graphql\n")
server.serve_forever()
