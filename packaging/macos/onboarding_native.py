"""Explicit native command boundary. Tests inject a fake; CLI has no bypass."""
import os
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
from onboarding_files import Refused, read, sha


def clean_env():
    # Do not inherit company/server DB credentials, model keys, config overlays,
    # DYLD injection, psql startup routing or global Memory service settings.
    return {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
            "LANG": "en_US.UTF-8", "LC_ALL": "C", "PGCONNECT_TIMEOUT": "5"}


class Native:
    def command(self, args, *, data=None, env=None, allowed=(0,), timeout=30):
        try:
            result = subprocess.run([str(a) for a in args], input=data, env=env or clean_env(),
                                    stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                    timeout=timeout, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise Refused("native operation unconfirmed; retain journal and inspect (output suppressed)") from None
        if len(result.stdout) > 65536 or result.returncode not in allowed:
            raise Refused("native operation denied/unconfirmed; retain journal (output suppressed)")
        return result.returncode, result.stdout.decode("utf-8", errors="strict").strip()

    def preflight(self, plan):
        if sys.platform != "darwin" or os.getuid() == 0 or os.geteuid() != os.getuid():
            raise Refused("native provisioning requires Darwin, a non-root logged-in user and Python 3.11+; no native effects")
        if sys.version_info < (3, 11):
            raise Refused("explicit Python 3.11+ prerequisite unavailable; installer is not self-contained")
        arch = {"arm64": "arm64", "x86_64": "x86_64"}.get(platform.machine())
        if not arch:
            raise Refused("unsupported Mac architecture")
        for pin in plan["pins"]:
            if sha(pin["path"]) != pin["sha256"]:
                raise Refused("installed prerequisite changed from reviewed SHA-256")
            if pin["kind"] == "mach-o":
                self.command(["/usr/bin/lipo", "-verify_arch", arch, pin["path"]])
        memory = plan["input"]["memory"]
        if self.command([memory["path"], "--version"])[1] != memory["version"]:
            raise Refused("installed Memory version differs from reviewed prerequisite")
        pg = plan["input"]["postgres"]
        for tool in ["postgres", "initdb", "pg_ctl", "psql", "pg_config", "pg_controldata"]:
            version = self.command([Path(pg["bin_dir"]) / tool, "--version"])[1]
            if not version.startswith((f"{tool} (PostgreSQL) {pg['major']}.", f"PostgreSQL {pg['major']}.")):
                raise Refused("PostgreSQL tools are not the same reviewed major")
        share = self.command([Path(pg["bin_dir"]) / "pg_config", "--sharedir"])[1]
        lib = self.command([Path(pg["bin_dir"]) / "pg_config", "--pkglibdir"])[1]
        if str(Path(share) / "extension/vector.control") != pg["vector_control"] or pg["vector_library"] not in {str(Path(lib) / name) for name in ("vector.so", "vector.dylib")}:
            raise Refused("pgvector does not belong to the selected installed PostgreSQL distribution")

    def review(self, plan):
        for selected in plan["selections"]:
            value = selected["metadata"]
            scope = value["bootstrap"]["scope"]
            args = [plan["input"]["memory"]["path"], "projects", "subscription",
                    "--company", value["company"], "--project", scope["project"],
                    "--area", scope["area"], "--owner-instance", scope["owner"],
                    "--revision", str(value["bootstrap"]["catalog_revision"]),
                    "--output", selected["path"]]
            if value.get("allow_loopback_development"):
                args.append("--allow-loopback")
            if value.get("replication_mode"):
                args.extend(["--replication-mode", value["replication_mode"]])
            _, output = self.command(args)
            try:
                preview = json.loads(output)
            except (ValueError, TypeError):
                raise Refused("subscription preview unreadable; output suppressed, no confirmation") from None
            expected = {"company", "bootstrap", "bootstrap_sha256", "output",
                        "contains_database_credentials", "creates_services", "downloads_knowledge"}
            if value.get("replication_mode"):
                expected.add("replication_mode")
                if preview.get("replication_mode") != value["replication_mode"]:
                    raise Refused("subscription replication mode changed; no local provisioning")
            if value.get("manifest_sha256"):
                expected.add("manifest_sha256")
                if preview.get("manifest_sha256") != value["manifest_sha256"]:
                    raise Refused("subscription project manifest changed; no local provisioning")
            if not isinstance(preview, dict) or set(preview) != expected or preview["company"] != value["company"] or preview["bootstrap"] != value["bootstrap"] or preview["output"] != selected["path"] or any(preview[key] is not False for key in ["contains_database_credentials", "creates_services", "downloads_knowledge"]) or not isinstance(preview["bootstrap_sha256"], str) or not re.fullmatch("[a-f0-9]{64}", preview["bootstrap_sha256"]):
                raise Refused("current subscription preview differs from exact prepared metadata; no confirmation")
            # Use only the CLI's current preview digest. Do not reconstruct a
            # different serialization/hash contract inside the installer.
            self.command([*args, "--bootstrap-sha256", preview["bootstrap_sha256"], "--confirm"])
            if read(selected["path"], 64 * 1024) != value:
                raise Refused("subscription replay identity changed; no local provisioning")

    def loaded(self, label):
        code, _ = self.command(["/bin/launchctl", "print", f"gui/{os.getuid()}/{label}"], allowed=(0, 113))
        return code == 0

    def available(self, port):
        import socket
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
                probe.bind(("127.0.0.1", port))
        except OSError:
            raise Refused("selected loopback port is occupied; no unrelated process is stopped") from None

    def health(self, area):
        import http.client
        import json
        import time
        for _ in range(20):
            connection = http.client.HTTPConnection("127.0.0.1", area["api_port"], timeout=1)
            try:
                connection.request("GET", "/healthz")
                response = connection.getresponse()
                body = response.read(65537)
                if response.status == 200 and len(body) <= 65536:
                    value = json.loads(body)
                    # instance_id is a random boot ID in the existing service;
                    # service_id is the explicitly configured durable local tag.
                    if isinstance(value, dict) and value.get("status") == "ok" and value.get("database") == "up" and value.get("role") == "primary" and value.get("service_id") == "memory-onboarding-" + area["instance_id"]:
                        return
            except (OSError, ValueError, http.client.HTTPException):
                pass
            finally:
                connection.close()
            time.sleep(1)
        raise Refused("exact named local area identity/health not verified; no fallback")

    def bootstrap(self, plist):
        self.command(["/bin/launchctl", "bootstrap", f"gui/{os.getuid()}", plist])

    def sql(self, plan, root, database, script, *, role="memory_onboarding_admin"):
        env = clean_env()
        env["PGPASSFILE"] = str(root / f"{role}.pgpass")
        return self.command([Path(plan["input"]["postgres"]["bin_dir"]) / "psql", "-X", "-w",
                             "-h", "127.0.0.1", "-p", str(plan["input"]["pg_port"]),
                             "-U", role, "-d", database, "-v", "ON_ERROR_STOP=1", "-At"],
                            data=script.encode(), env=env)[1]

    def wait(self, plan, root, database, script, *, role="memory_onboarding_admin"):
        import time
        for _ in range(20):
            try:
                if self.sql(plan, root, database, script, role=role) == "t":
                    return
            except Refused:
                pass
            time.sleep(1)
        raise Refused("named local database/service not verified ready; journal retained, no fallback")
