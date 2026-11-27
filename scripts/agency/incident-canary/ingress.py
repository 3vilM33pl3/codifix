#!/usr/bin/env python3
"""Receive authenticated Alertmanager webhooks for explicitly enrolled services."""
import argparse
import hmac
from http.client import HTTPConnection, HTTPException
from http.server import BaseHTTPRequestHandler, HTTPServer
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import ssl

ROUTE = "/v1/incidents/alertmanager"
MAX_BODY = 131072
TIMEOUT = 5


class CanaryIngress(HTTPServer):
    def __init__(self, address, tls, expected_auth, upstream_port, rules):
        super().__init__(address, Webhook)
        self.tls = tls
        self.expected_auth = expected_auth
        self.upstream_port = upstream_port
        self.rules = rules

    def get_request(self):
        sock, address = super().get_request()
        sock.settimeout(TIMEOUT)
        try:
            return self.tls.wrap_socket(sock, server_side=True), address
        except OSError:
            sock.close()
            raise

    def finish_request(self, request, client_address):
        signal.setitimer(signal.ITIMER_REAL, TIMEOUT)
        try:
            super().finish_request(request, client_address)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)

    def handle_error(self, *_):
        # Request input, provider responses and authorization stay out of logs.
        pass


class Webhook(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def reply(self, status):
        self.close_connection = True
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.send_header("Connection", "close")
        self.end_headers()

    def send_error(self, code, *_args, **_kwargs):
        self.reply(code)

    def do_POST(self):
        if self.path != ROUTE:
            self.reply(404)
            return
        auth = self.headers.get_all("Authorization") or []
        if len(auth) != 1 or not hmac.compare_digest(auth[0].encode(), self.server.expected_auth):
            self.reply(401)
            return
        lengths = self.headers.get_all("Content-Length") or []
        if len(lengths) != 1 or not re.fullmatch(r"[0-9]{1,8}", lengths[0]) or "Transfer-Encoding" in self.headers:
            self.reply(400)
            return
        length = int(lengths[0])
        if not 0 < length <= MAX_BODY:
            self.reply(413)
            return
        if self.headers.get_content_type() != "application/json":
            self.reply(415)
            return
        payload = self.rfile.read(length)
        forwarded = enrolled_alerts(payload, self.server.rules) if len(payload) == length else None
        if forwarded is None:
            self.reply(400)
            return
        upstream = HTTPConnection("127.0.0.1", self.server.upstream_port, timeout=TIMEOUT)
        try:
            upstream.request("POST", ROUTE, forwarded, {"Content-Type": "application/json", "Authorization": auth[0]})
            status = upstream.getresponse().status
        except (OSError, HTTPException):
            status = 502
        finally:
            upstream.close()
        self.reply(status if 200 <= status < 300 or 400 <= status < 500 else 502)


CANARY = {
    "service": "codifix-incident-canary",
    "alertname": "CodifixIncidentCanaryDown",
    "codifix_canary": "true",
}


def enrolled_alerts(payload, rules):
    try:
        alerts = json.loads(payload)["alerts"]
        if not isinstance(alerts, list) or not 0 < len(alerts) <= 64:
            return None
        if all(all(row["labels"].get(k) == v for k, v in CANARY.items()) for row in alerts):
            return payload
        admitted = []
        for row in alerts:
            labels = row["labels"]
            service = labels["service"]
            expected = rules.get(service)
            if not expected or any(labels.get(key) != value for key, value in expected.items()):
                return None
            if row["status"] not in ("firing", "resolved"):
                return None
            admitted.append({
                "labels": {"service": service},
                "annotations": {"summary": f"Availability alert for {service}"},
                "status": row["status"],
                "fingerprint": row["fingerprint"],
                "startsAt": row["startsAt"],
            })
        return json.dumps({"alerts": admitted}).encode()
    except (AttributeError, ValueError, TypeError, KeyError, RecursionError):
        return None


def request_timeout(*_):
    raise TimeoutError("canary webhook deadline reached")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--listen", type=ipaddress.IPv4Address, default="10.22.6.42")
    parser.add_argument("--port", type=int, default=18676)
    parser.add_argument("--upstream-port", type=int, default=8151)
    parser.add_argument("--rules", type=Path, help="Root-reviewed exact service label sets.")
    args = parser.parse_args()
    credentials = Path(os.environ["CREDENTIALS_DIRECTORY"])
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.minimum_version = ssl.TLSVersion.TLSv1_2
    tls.load_cert_chain(credentials / "tls.crt", credentials / "tls.key")
    with (credentials / "alert.token").open("rb") as handle:
        token = handle.read(257).strip()
    if not re.fullmatch(rb"[A-Za-z0-9_-]{32,256}", token):
        raise SystemExit("Expected a bounded random canary bearer")
    rules = json.loads(args.rules.read_text()) if args.rules else {}
    if not isinstance(rules, dict) or any(
        not re.fullmatch(r"[a-z0-9-]{1,64}", service)
        or not isinstance(fields, dict)
        or set(fields) != {"alertname", "instance", "job", "notification_class"}
        or not all(isinstance(v, str) and v for v in fields.values())
        for service, fields in rules.items()
    ):
        raise SystemExit("Invalid reviewed Alertmanager labels")
    signal.signal(signal.SIGALRM, request_timeout)
    with CanaryIngress((str(args.listen), args.port), tls, b"Bearer " + token, args.upstream_port, rules) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()
