import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import urlopen
from uuid import uuid4

import stage


class CanaryTest(unittest.TestCase):
    def test_scoped_render_recovery_and_receipts(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            scope = stage.load_scope(stage.HERE / "scope.example.json")
            stage.render(scope, root)
            profile = stage.read(root / "incident-profile.json")
            self.assertEqual(list(profile["services"]), [stage.SERVICE])
            self.assertFalse(profile["services"][stage.SERVICE]["repairs"]["clear_canary_fault"]["autonomous"])
            self.assertEqual(stage.read(root / "home-grant.json")["mandate"]["resources"], [{"kind": "service", "id": stage.SERVICE}])
            base = [sys.executable, str(stage.HERE / "service.py"), "--state", str(root)]
            server = subprocess.Popen(base + ["--port", "0", "serve"], stdout=subprocess.PIPE, text=True)
            try:
                port = server.stdout.readline().strip()
                cmd = base + ["--port", port]
                def run(*args, value=None, check=True):
                    return subprocess.run(cmd + list(args), input=json.dumps(value), text=True, capture_output=True, check=check)
                run("user-request")
                event = json.loads(run("begin").stdout)
                stage.exercise(scope, event, root)
                self.assertNotEqual(run("begin", check=False).returncode, 0)
                with self.assertRaises(HTTPError) as error:
                    urlopen(f"http://127.0.0.1:{port}/health", timeout=3)
                self.assertEqual(error.exception.code, 503)
                error.exception.close()
                self.assertNotEqual(run("user-request", check=False).returncode, 0)
                inspector = json.loads(run("inspect").stdout)
                self.assertEqual(inspector["repair"], "clear_canary_fault")
                submission = stage.read(root / "reporter-delegate.json")
                self.assertEqual(submission["payload"]["request"]["payload"]["starts_at"], event["started"])
                run("condition")
                incident = str(uuid4())
                run("repair", value={"incident_id":incident})
                self.assertNotEqual(run("repair", value={"incident_id":incident}, check=False).returncode, 0)
                self.assertNotEqual(run("condition", check=False).returncode, 0)
                run("user-request")
                with urlopen(f"http://127.0.0.1:{port}/health", timeout=3) as response:
                    self.assertTrue(json.load(response)["healthy"])
                final = json.loads(run("state").stdout)
                self.assertEqual(final["repairs"], 1)
                self.assertEqual(final["repaired_by"], incident)
                def task(value):
                    task_id = str(uuid4())
                    return {"id":task_id, "status":{"state":"TASK_STATE_COMPLETED"}, "artifacts":[{"artifactId":task_id + ":evidence","parts":[{"data":value}]}]}
                report = task({"reported_result":task({"incident_id":incident})})
                events = [{"state":state} for state in ("waiting_for_operator","approved","recovered")]
                events += [dict(state="acknowledged",source=peer,impact=event["id"]) for peer in (stage.ALERT_PEER,scope["reporter"]["peer"])]
                status = {"id":incident, "service":stage.SERVICE, "state":"recovered", "verified":{"health":True,"user_request":True},
                          "ticket":{"issue_id":str(uuid4()),"url":"https://linear.example.invalid/test","readback_verified":True},
                          "events":[{"event":e,"delivery_problem":None} for e in events]}
                delivered = dict(incident_id=incident,events=len(events),all_delivered=True)
                inboxes = [dict(incident_id=incident, conversation=scope[k], received_recovery=True) for k in ("operator_conversation","reporter_conversation")]
                stage.check(scope, final, status, report, delivered, *inboxes)
                inboxes[1]["received_recovery"] = False
                with self.assertRaisesRegex(ValueError,"Inbox receipt"):
                    stage.check(scope, final, status, report, delivered, *inboxes)
            finally:
                server.terminate()
                server.wait(timeout=5)
                server.stdout.close()


if __name__ == "__main__":
    unittest.main()
