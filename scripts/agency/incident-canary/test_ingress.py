"""Exercise the canary ingress over trusted TLS with an isolated backend."""
from http.client import HTTPSConnection
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from ingress import MAX_BODY, ROUTE, enrolled_alerts

HERE = Path(__file__).resolve().parent
TOKEN = "canary-test-token-" + "0" * 36
LABELS = {
    "alertname": "CodifixIncidentCanaryDown",
    "service": "codifix-incident-canary",
    "codifix_canary": "true",
}


class IngressSmoke(unittest.TestCase):
    def test_real_alert_is_exactly_scoped_and_does_not_forward_annotations(self):
        fields = {"alertname": "HomelabAppEndpointDown", "instance": "https://example.invalid/healthz",
                  "job": "blackbox_http", "notification_class": "notify"}
        rules = {"example": fields}
        row = {"labels": fields | {"service": "example"}, "status": "firing",
               "fingerprint": "fp", "startsAt": "2026-11-26T00:00:00Z",
               "annotations": {"description": "unrequested private detail"}}
        result = enrolled_alerts(json.dumps({"alerts": [row]}).encode(), rules)
        self.assertEqual(json.loads(result)["alerts"][0]["annotations"], {"summary": "Availability alert for example"})
        row["labels"]["instance"] = "https://unapproved.invalid/health"
        self.assertIsNone(enrolled_alerts(json.dumps({"alerts": [row]}).encode(), rules))

    def test_private_canary_only(self):
        receipts = []

        class Backend(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                receipts.append((self.path, self.headers["Authorization"], json.loads(body)))
                self.send_response(200)
                self.send_header("Content-Length", "0")
                self.end_headers()

        with tempfile.TemporaryDirectory() as temp, HTTPServer(("127.0.0.1", 0), Backend) as backend:
            directory = Path(temp)
            cert, key = directory / "tls.crt", directory / "tls.key"
            subprocess.run(
                ["openssl", "req", "-x509", "-nodes", "-newkey", "rsa:2048", "-days", "1",
                 "-subj", "/CN=Canary test", "-addext", "subjectAltName=IP:127.0.0.1",
                 "-addext", "keyUsage=critical,digitalSignature,keyCertSign",
                 "-keyout", str(key), "-out", str(cert)],
                check=True, capture_output=True,
            )
            (directory / "alert.token").write_text(TOKEN + "\n")
            tls = ssl.create_default_context(cafile=str(cert))
            with socket.socket() as reserve:
                reserve.bind(("127.0.0.1", 0))
                port = reserve.getsockname()[1]
            backend_thread = threading.Thread(target=backend.serve_forever, daemon=True)
            backend_thread.start()
            child = subprocess.Popen(
                [sys.executable, "-I", HERE / "ingress.py", "--listen", "127.0.0.1",
                 "--port", str(port), "--upstream-port", str(backend.server_port)],
                env=os.environ | {"CREDENTIALS_DIRECTORY": temp},
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            self.addCleanup(child.stdout.close)
            self.addCleanup(child.stderr.close)

            def request(method, path, body=None, bearer=TOKEN, **changes):
                conn = HTTPSConnection("127.0.0.1", port, context=tls, timeout=3)
                try:
                    headers = {"Authorization": "Bearer " + bearer, "Content-Type": "application/json"} | changes
                    conn.request(method, path, body, headers)
                    return conn.getresponse().status
                finally:
                    conn.close()

            payload = {"alerts": [{"labels": LABELS}]}
            try:
                for _ in range(40):
                    try:
                        self.assertEqual(request("GET", ROUTE), 501)
                        break
                    except ConnectionRefusedError:
                        time.sleep(.05)
                else:
                    self.fail("Ingress never became available over pinned TLS.")
                self.assertEqual(request("POST", ROUTE + "?other=1", json.dumps(payload)), 404)
                self.assertEqual(request("POST", ROUTE, json.dumps(payload), "incorrect"), 401)
                self.assertEqual(request("POST", ROUTE, json.dumps({"alerts": [{"labels": {}}]})), 400)
                self.assertEqual(request("POST", ROUTE, "", **{"Content-Length": str(MAX_BODY + 1)}), 413)
                self.assertEqual(receipts, [])
                self.assertEqual(request("POST", ROUTE, json.dumps(payload)), 200)
                self.assertEqual(receipts, [(ROUTE, "Bearer " + TOKEN, payload)])
            finally:
                child.terminate()
                out, err = child.communicate(timeout=5)
                backend.shutdown()
                backend_thread.join(timeout=5)
            self.assertEqual((out, err), (b"", b""))


if __name__ == "__main__":
    unittest.main()
