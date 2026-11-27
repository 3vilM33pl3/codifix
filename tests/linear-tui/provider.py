"""Loopback-only GraphQL stand-in for the full Linear installer/TUI rehearsal."""
import json
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import sys

PROJECT = "e3eef73c-065a-4ac0-9f43-a50411507ef4"
SECOND_PROJECT = "01b99d74-38fe-4c67-9f47-adefbbe37ee1"
TEAM = "dbac4c6d-0a2a-4401-a7e5-d14f97d9a1b6"
IN_PROGRESS = "f329f1f9-650d-466f-b500-c83c81bd0d1e"
DONE = "18bb3348-212a-4b68-b4cc-85121b0d695e"
STATES = [
    {"id": IN_PROGRESS, "name": "In Progress", "type": "started", "archivedAt": None, "team": {"id": TEAM}},
    {"id": DONE, "name": "Done", "type": "completed", "archivedAt": None, "team": {"id": TEAM}},
]
PROJECT_NAMES = {PROJECT: "Homelab", SECOND_PROJECT: "Codifix"}
STATE = Path(sys.argv[1])
READY = Path(sys.argv[2])
UNAVAILABLE = STATE.with_suffix(".unavailable")
first_read_after_write = set()


def project_record(project):
    return {"id": project, "name": PROJECT_NAMES[project], "archivedAt": None,
            "teams": {"nodes": [{"id": TEAM, "name": "Engineering", "states": {"nodes": STATES}}]}}


class Graphql(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        assert self.path == "/graphql"
        assert self.headers.get("Authorization") == "synthetic-linear-key"
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if UNAVAILABLE.exists():
            self.send_response(503)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        query = request["query"]
        variables = request.get("variables") or {}
        issues = json.loads(STATE.read_text())
        if query.startswith("query Projects"):
            result = {"projects": {"nodes": [project_record(PROJECT)],
                       "pageInfo": {"hasNextPage": False, "endCursor": None}}}
        elif query.startswith("query Project("):
            result = {"project": project_record(variables["id"])}
        elif "query AgencyProjectRoute" in query:
            selected = {state["id"]: state for state in STATES}
            result = {"project": project_record(variables["project"]),
                      "started": selected.get(variables["started"]), "done": selected.get(variables["done"])}
        elif "project(id:" in query or "team(id:" in query or "workflowState(id:" in query:
            project = next((value for value in variables.values() if value in PROJECT_NAMES), PROJECT)
            result = {"project": project_record(project),
                      "team": {"id": TEAM, "states": {"nodes": STATES}},
                      "workflowState": next((value for value in STATES if value["id"] in variables.values()), None)}
        elif query.startswith("query AgencyFindIssue") or query.startswith("query AgencyScopedIssue"):
            filt = variables["filter"]
            project = filt["project"]["id"]["eq"]
            assert project in PROJECT_NAMES and filt["team"]["id"]["eq"] == TEAM
            scoped = {id: item for id, item in issues.items() if item["project"]["id"] == project}
            if "title" in filt:
                rows = [i for i in scoped.values() if i["title"].casefold() == filt["title"]["eqIgnoreCase"].casefold()]
            else:
                id = filt["id"]["eq"]
                issue = scoped.get(id)
                rows = [] if issue is None or id in first_read_after_write else [issue]
                first_read_after_write.discard(id)
            result = {"issues": {"nodes": rows}}
        elif query.startswith("mutation AgencyWrite"):
            value = variables["input"]
            if "issueCreate(" in query:
                project = value["projectId"]
                assert project in PROJECT_NAMES and value["teamId"] == TEAM
                assert value["useDefaultTemplate"] is False
                issue_id = value["id"]
                assert issue_id not in issues
                issue = {"id": issue_id, "project": {"id": project}, "team": {"id": TEAM},
                         "projectMilestone": None, "assignee": None, "priority": 0, "labelIds": [],
                         "dueDate": None, "state": None, "cycle": None, "parent": None,
                         "url": ("https://linear.app/example/issue/COD-42/terminal-workflow"
                                 if project == PROJECT else f"https://linear.app/example/issue/{issue_id}")}
            else:
                assert "issueUpdate(" in query
                issue_id = variables["id"]
                issue = issues[issue_id]
            for field in ("title", "description"):
                if field in value:
                    issue[field] = value[field]
            if "stateId" in value:
                issue["state"] = {"id": value["stateId"]}
            issue["writes"] = issue.get("writes", 0) + 1
            issue["updatedAt"] = f"2026-11-25T00:00:{issue['writes']:02}Z"
            issues[issue_id] = issue
            temp = STATE.with_suffix(".new")
            temp.write_text(json.dumps(issues))
            temp.replace(STATE)
            first_read_after_write.add(issue_id)
            result = None
        else:
            raise AssertionError("Unexpected GraphQL operation: " + query)
        response = json.dumps(
            {"data": result} if result is not None else {"errors": [{"message": "fixture lost write acknowledgment"}]}
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)


server = ThreadingHTTPServer(("127.0.0.1", 0), Graphql)
STATE.write_text("{}")
READY.write_text(f"http://127.0.0.1:{server.server_port}/graphql\n")
server.serve_forever()
