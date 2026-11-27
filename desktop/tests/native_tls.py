#!/usr/bin/env python3
"""Exercise the production Rust client with isolated Linux CA bundles.

No system trust changes, credentials, production writes or browser mocks.
Requires Python 3, OpenSSL and the desktop Rust build prerequisites.
"""
import http.server
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import tempfile
import threading

DESKTOP = Path(__file__).resolve().parents[1]
MANIFEST = DESKTOP / "src-tauri/Cargo.toml"


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, text=True, capture_output=True).stdout


def main():
    if not sys.platform.startswith("linux"):
        raise SystemExit("This regression exercises the Linux system certificate store.")
    messages = run("cargo", "test", "--manifest-path", str(MANIFEST), "--locked",
                   "--no-run", "--message-format=json")
    binary = next(item["executable"] for line in messages.splitlines()
                  if (item := json.loads(line)).get("reason") == "compiler-artifact"
                  and item.get("executable") and item["profile"]["test"])
    with tempfile.TemporaryDirectory(prefix="codifix-tls-") as directory:
        root = Path(directory)
        (root / "empty-ca-directory").mkdir()
        for name in ("trusted", "unrelated"):
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                "-keyout", f"{name}.key", "-out", f"{name}.pem", "-days", "1",
                "-subj", f"/CN=Codifix {name} test CA", cwd=root)
        run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
            "-keyout", "leaf.key", "-out", "leaf.csr", "-subj", "/CN=localhost", cwd=root)
        (root / "extensions").write_text(
            "basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:localhost\n"
            "keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
        run("openssl", "x509", "-req", "-in", "leaf.csr", "-CA", "trusted.pem",
            "-CAkey", "trusted.key", "-CAcreateserial", "-out", "leaf.pem",
            "-days", "1", "-extfile", "extensions", cwd=root)

        class Fixture(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                assert self.path == "/v1/auth/me"
                assert self.headers.get("Authorization") is None
                body = b'{"fixture":true}'
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(root / "leaf.pem", root / "leaf.key")
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            for label, ca, host, accepted in (
                ("OS-trusted CA and correct hostname", "trusted", "localhost", True),
                ("untrusted CA rejected", "unrelated", "localhost", False),
                ("wrong hostname rejected", "trusted", "127.0.0.1", False),
            ):
                child_env = os.environ.copy()
                child_env.update(
                    SSL_CERT_FILE=str(root / f"{ca}.pem"),
                    SSL_CERT_DIR=str(root / "empty-ca-directory"),
                    CODIFIX_TLS_TEST_ORIGIN=f"https://{host}:{server.server_port}",
                    CODIFIX_TLS_EXPECT_TRUSTED="1" if accepted else "0",
                )
                result = subprocess.run(
                    [binary, "--exact", "tests::tls_system_roots_probe", "--ignored", "--nocapture"],
                    env=child_env, text=True, capture_output=True, timeout=30,
                )
                if result.returncode:
                    raise RuntimeError(f"{label}:\n{result.stdout}\n{result.stderr}")
                print(f"PASS: {label}")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    main()
