#!/usr/bin/env python3
"""Render an offline, single-service canary enrollment, exercise or evidence check."""
import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import re
from urllib.parse import urlsplit
from uuid import UUID, uuid5

from service import DEFAULT_STATE, SERVICE

HERE = Path(__file__).resolve().parent
EXAMPLES = HERE.parents[2] / "crates/codifix-agency-runtime/examples"
PYTHON = ["/usr/bin/python3", "-I", "/usr/local/libexec/codifix-incident-canary/service.py"]
ALERT = "CodifixIncidentCanaryDown"
ALERT_PEER = "codifix-canary-alertmanager"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def read(path):
    return json.loads(Path(path).read_text())


def write(target, name, value):
    with (target / name).open("x") as f:
        json.dump(value, f, indent=2)
        f.write("\n")


def load_scope(path):
    s = read(path)
    for role in ("home", "reporter", "linear"):
        for key in ("instance", "project", "mandate"):
            s[role][key] = str(UUID(s[role][key]))
        for key in ("peer", "specialist"):
            require(re.fullmatch(r"[a-zA-Z0-9._-]{1,128}", s[role][key]), f"Invalid {role}.{key}")
    for key in ("operator_conversation", "reporter_conversation", "alert_instance", "linear_project", "linear_team"):
        s[key] = str(UUID(s[key]))
    require(len({s[r]["instance"] for r in ("home", "reporter", "linear")} | {s["alert_instance"]}) == 4, "Use distinct enrolled instances.")
    url = urlsplit(s["alert_webhook"])
    require(url.scheme == "https" and url.hostname and not url.username and not url.password and not url.query and not url.fragment and url.path.endswith("/v1/incidents/alertmanager"), "An explicit private HTTPS incident webhook is required.")
    return s


def budget(calls, turns, depth):
    return dict(micro_usd=0, calls=calls, turns=turns, delegation_depth=depth)


def render(scope, target):
    profile = read(EXAMPLES / "incidents.json")
    home, reporter, linear = (scope[role] for role in ("home", "reporter", "linear"))
    rule = profile["services"].pop("catalogue")
    rule.update(inspect=PYTHON + ["inspect"], health=["/usr/bin/curl", "--fail", "--silent", "--max-time", "4", "http://127.0.0.1:18675/health"],
                user_request=PYTHON + ["user-request"], runbook=DEFAULT_STATE + "/runbook.md", postmortem_directory=DEFAULT_STATE + "/postmortems",
                repairs={"clear_canary_fault": dict(condition=PYTHON + ["condition"], command=PYTHON + ["repair"], autonomous=False,
                                                    action="Clear the active fault in this canary's SQLite row.",
                                                    risks="The canary item becomes available again.", recovery_options="Leave the canary degraded until investigation finishes.")})
    profile.update(alert_peer=ALERT_PEER, specialist=home["specialist"], mandate_id=home["mandate"], services={SERVICE: rule},
                   inbox={"operator_conversation": scope["operator_conversation"], "reporting_peers": [reporter["peer"]]},
                   linear=dict(peer=linear["peer"], specialist=linear["specialist"], mandate_id=linear["mandate"], project_id=scope["linear_project"]))
    write(target, "incident-profile.json", profile)
    expires = (datetime.now(timezone.utc) + timedelta(days=30)).isoformat()
    for name, capability, kind, resource, limits in (
        ("home", "incident_report", "service", SERVICE, budget(64, 48, 0)),
        ("reporter", "delegate", "peer", home["peer"], budget(48, 24, 1)),
        ("linear", "linear_track", "linear_project", scope["linear_project"], budget(512, 128, 0)),
    ):
        role = scope[name]
        write(target, name + "-grant.json", {"project_id": role["project"], "mandate": {
            "id": role["mandate"], "issuer_instance_id": role["instance"], "subject_instance_id": role["instance"],
            "specialist_id": role["specialist"], "capabilities": [capability], "resources": [{"kind": kind, "id": resource}],
            "destinations": [], "expires_at": expires, "status": "active", "budget": limits
        }})
    write(target, "alert-peer.json", {"id": ALERT_PEER, "instance_id": scope["alert_instance"], "token_env": "CODIFIX_CANARY_ALERT_TOKEN",
                                      "operator": False, "mandates": [home["mandate"]]})
    write(target, "linear-tracking-entry.json", {linear["mandate"]: {"project_id": scope["linear_project"], "team_id": scope["linear_team"]}})
    write(target, "reporter-incident-inbox.json", {"incident_conversation": scope["reporter_conversation"]})
    write(target, "alertmanager-route.json", {"receiver": SERVICE, "matchers": [f'alertname="{ALERT}"', f'service="{SERVICE}"', 'codifix_canary="true"'],
                                              "group_by": ["alertname", "service", "exercise"], "group_wait": "1s", "continue": False})
    write(target, "alertmanager-receiver.json", {"name": SERVICE, "webhook_configs": [{"url": scope["alert_webhook"], "send_resolved": False,
        "http_config": {"authorization": {"type": "Bearer", "credentials_file": "/etc/alertmanager/codifix-canary/token"}}}]})


def exercise(scope, run, target):
    event = UUID(run["id"])
    started = datetime.fromisoformat(run["started"])
    require(started.tzinfo is not None and not run.get("repaired_by"), "A current open canary run is required.")
    deadline = started + timedelta(minutes=45)
    require(datetime.now(timezone.utc) < deadline, "Canary event expired.")
    start = started.isoformat()
    home, reporter = scope["home"], scope["reporter"]
    write(target, "alert.json", [{"labels": {"alertname": ALERT, "service": SERVICE, "codifix_canary": "true", "exercise": str(event)},
        "annotations": {"summary": f"Controlled canary outage {event}"}, "startsAt": start, "endsAt": deadline.isoformat()}])
    def task(role, kind, resource, criterion, payload, limits):
        return {"request_id": str(uuid5(event, role["instance"])), "mandate_id": role["mandate"], "specialist_id": role["specialist"],
                "objective": f"Verify isolated incident {event}", "acceptance_criteria": [criterion], "capability": kind,
                "resource": resource, "deadline": deadline.isoformat(), "budget": limits, "payload": payload, "parent_id": None}
    report = task(home, "incident_report", {"kind": "service", "id": SERVICE}, "incident_report_processed",
                  {"kind": "incident_report", "starts_at": start, "summary": f"Reporting agent observed canary outage {event}"}, budget(8, 4, 0))
    write(target, "reporter-delegate.json", task(reporter, "delegate", {"kind": "peer", "id": home["peer"]}, "delegated_result_reported",
          {"kind": "delegate", "peer": home["peer"], "request": report}, budget(12, 8, 1)))


def check(scope, run, incident, task, delivery, home_inbox, reporter_inbox):
    incident_id = str(UUID(incident["id"]))
    require(run["repairs"] == 1 and run["repaired_by"] == incident_id, "Canary repair count or incident binding differs.")
    require(incident["service"] == SERVICE and incident["state"] == "recovered" and incident["verified"] == {"health": True, "user_request": True}, "Recovery probes incomplete.")
    events = [row["event"] for row in incident["events"]]
    sources = {e.get("source") for e in events if e["state"] == "acknowledged" and run["id"] in e.get("impact", "")}
    require({ALERT_PEER, scope["reporter"]["peer"]} <= sources, "Missing matching alert or signed reporting agency.")
    require(all(any(e["state"] == state for e in events) for state in ("waiting_for_operator", "approved", "recovered")), "Approval or recovery absent.")
    ticket = incident.get("ticket") or {}
    require(ticket.get("readback_verified") is True and UUID(ticket["issue_id"]) and urlsplit(ticket["url"]).scheme == "https", "Missing verified Linear readback.")
    def evidence(t):
        require(t["status"]["state"] == "TASK_STATE_COMPLETED", "Waiting for delegated report to finish.")
        return next(p["data"] for a in t["artifacts"] if a["artifactId"] == t["id"] + ":evidence" for p in a["parts"] if "data" in p)
    require(evidence(evidence(task)["reported_result"])["incident_id"] == incident_id, "Signed report returned a different incident.")
    require(all(not row["delivery_problem"] for row in incident["events"]), "Wait for pending deliveries.")
    require(delivery["incident_id"] == incident_id and delivery["events"] == len(events) and delivery["all_delivered"] is True, "Pending notification or Linear tracking.")
    for snapshot, chat in [(home_inbox, scope["operator_conversation"]), (reporter_inbox, scope["reporter_conversation"])]:
        require(snapshot["incident_id"] == incident_id and snapshot["conversation"] == chat and snapshot["received_recovery"] is True, "Missing receiver-owned Inbox receipt.")
    print(f"Verified canary {run['id']}, incident {incident_id}, ticket {ticket['url']}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    for name in ("render", "exercise", "check"):
        p = sub.add_parser(name)
        p.add_argument("scope", type=Path)
        if name != "render":
            p.add_argument("run", type=Path)
        if name == "check":
            p.add_argument("incident", type=Path)
            p.add_argument("report_task", type=Path)
            p.add_argument("delivery", type=Path)
            p.add_argument("home_inbox", type=Path)
            p.add_argument("reporter_inbox", type=Path)
        else:
            p.add_argument("output", type=Path)
    a = ap.parse_args()
    s = load_scope(a.scope)
    if a.command == "check":
        check(s, read(a.run), read(a.incident), read(a.report_task), read(a.delivery), read(a.home_inbox), read(a.reporter_inbox))
    else:
        a.output.mkdir(mode=0o700)
        if a.command == "render":
            render(s, a.output)
        else:
            exercise(s, read(a.run), a.output)


if __name__ == "__main__":
    main()
