"""Inspect the configured systemd service and its checkout for incident response."""
import json
import subprocess
import sys

unit, checkout = sys.argv[1:]


def capture(*args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=5)
    if result.returncode:
        return "Unavailable"
    return result.stdout.strip()[:1200] or "No observations"


state = capture("/usr/bin/systemctl", "show", unit, "-p", "ActiveState", "--value")
# Forward only journal event counts. Journal text may contain private request data.
journal = capture("/usr/bin/journalctl", "-u", unit, "-p", "err", "--since",
                  "15 minutes ago", "--quiet", "--no-pager", "-n", "20", "-o", "json")
n_errors = sum(line.startswith("{") for line in journal.splitlines())
print(json.dumps({
    "impact": f"Catalogue request reported unavailable. Unit state is {state}.",
    "logs": ("Journal unavailable" if journal == "Unavailable" else
             f"Recent journal sample contains {n_errors} error entries."),
    "metrics": capture("/usr/bin/systemctl", "show", unit, "-p", "NRestarts", "-p", "Result"),
    "recent_changes": capture("/usr/bin/git", "-C", checkout, "log", "-1", "--format=%h %cI"),
    "cause": f"The unit is {state}. The underlying cause requires journal inspection.",
    "repair": "restart_failed_unit" if state == "failed" else "",
    "runbook_note": "",
    "followups": []
}))
