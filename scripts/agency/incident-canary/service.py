#!/usr/bin/env python3
"""Loopback canary and constrained runbook commands for the Homelab Agency."""
import argparse
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import sqlite3
import sys
from uuid import UUID, uuid4
from urllib.request import ProxyHandler, build_opener

SERVICE = "codifix-incident-canary"
DEFAULT_STATE = "/var/lib/codifix-incident-canary"


def database(root):
    db = sqlite3.connect(root / "service.sqlite3", timeout=4)
    db.row_factory = sqlite3.Row
    db.execute("CREATE TABLE IF NOT EXISTS runs(id TEXT PRIMARY KEY, started TEXT NOT NULL, repaired_by TEXT, repairs INTEGER NOT NULL DEFAULT 0)")
    return db


def latest(db):
    row = db.execute("SELECT * FROM runs ORDER BY rowid DESC LIMIT 1").fetchone()
    return dict(row) if row else {}


def serve(root, port):
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path not in ("/health", "/items/42"):
                self.send_error(404)
                return
            with database(root) as db:
                row = latest(db)
            healthy = not row or bool(row["repaired_by"])
            data = ({"healthy": True} if self.path == "/health" else {"id": 42, "name": "canary item"}) if healthy else {"error": "controlled incident"}
            payload = json.dumps(data).encode()
            self.send_response(200 if healthy else 503)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(server.server_port, flush=True)
    server.serve_forever()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", type=Path, default=Path(DEFAULT_STATE))
    parser.add_argument("--port", type=int, default=18675)
    parser.add_argument("command", choices=("serve", "begin", "inspect", "condition", "repair", "state", "user-request"))
    args = parser.parse_args()
    if args.command == "serve":
        serve(args.state, args.port)
        return
    if args.command == "user-request":
        with build_opener(ProxyHandler({})).open(f"http://127.0.0.1:{args.port}/items/42", timeout=4) as result:
            sys.exit(0 if json.load(result) == {"id": 42, "name": "canary item"} else 1)
    with database(args.state) as db:
        db.execute("BEGIN IMMEDIATE")
        run = latest(db)
        down = bool(run and not run["repaired_by"])
        if args.command == "begin":
            if down:
                sys.exit("Finish the existing canary incident first.")
            db.execute("INSERT INTO runs(id,started) VALUES(?,?)", (str(uuid4()), datetime.now(timezone.utc).isoformat()))
            print(json.dumps(latest(db)))
        elif args.command == "condition":
            sys.exit(0 if down else 1)
        elif args.command == "repair":
            incident = str(UUID(json.load(sys.stdin)["incident_id"]))
            if not down:
                sys.exit("The canary is already healthy.")
            db.execute("UPDATE runs SET repaired_by=?, repairs=repairs+1 WHERE id=? AND repaired_by IS NULL", (incident, run["id"]))
        elif args.command == "inspect":
            print(json.dumps({
                "impact": "The isolated canary catalogue item is unavailable." if down else "The canary is healthy.",
                "logs": f"Local canary event {run.get('id', 'none')}.",
                "metrics": f"Canary unavailable={int(down)}, repair count={run.get('repairs', 0)}.",
                "recent_changes": f"Operator canary exercise began at {run.get('started', 'n/a')}.",
                "cause": "Controlled fault in the canary's local state." if down else "No active fault.",
                "repair": "clear_canary_fault" if down else "",
                "runbook_note": "Verified recovery using the catalogue item as well as the health endpoint."
            }))
        else:
            print(json.dumps(run))


if __name__ == "__main__":
    main()
