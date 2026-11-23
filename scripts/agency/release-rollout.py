#!/usr/bin/env python3
"""Test a candidate artifact, publish its evidence, and request an enrolled upgrade."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def certify(args):
    artifact = args.artifact.resolve(strict=True)
    original = digest(artifact)
    receipt = {"version": args.version, "url": args.release_url, "sha256": original}
    with tempfile.TemporaryDirectory(dir=args.receipt.parent) as run:
        stage = Path(run)
        for name, executable in (("artifact_test", args.artifact_test), ("upgrade_test", args.upgrade_test)):
            with (stage / name).open("w+b") as log:
                os.fchmod(log.fileno(), 0o600)
                subprocess.run([executable.resolve(strict=True), artifact], check=True, stdout=log,
                               stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, timeout=3600)
                log.seek(0)
                receipt[name + "_sha256"] = hashlib.file_digest(log, "sha256").hexdigest()
        if original != digest(artifact):
            raise ValueError("Artifact changed during release testing")
        # Keep the two successful test outputs addressable by the hashes sent to
        # the CA. Never include raw CI logs in the private or public messages.
        for name in ("artifact_test", "upgrade_test"):
            path = args.receipt.parent / (receipt[name + "_sha256"] + ".log")
            try:
                os.link(stage / name, path)
            except FileExistsError:
                if digest(path) != receipt[name + "_sha256"]:
                    raise ValueError("Stored test evidence has a different digest")
        record = stage / "receipt"
        record.write_text(json.dumps(receipt, indent=2) + "\n")
        os.link(record, args.receipt)



def deploy(args):
    cfg = json.loads(args.rollout.read_text())
    receipt = json.loads(args.receipt.read_text())
    template = cfg["delegation"]
    downstream = template["payload"]["request"]
    if template["capability"] != "delegate" or downstream["capability"] != "upgrade_candidate":
        raise ValueError("Expected an enrolled upgrade delegation")
    downstream["payload"]["release"] = receipt
    # Re-running the publisher against the same target/release reconciles its existing task.
    template["request_id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, json.dumps(
        [cfg["agency_config"], downstream["resource"], receipt], sort_keys=True)))
    cli = [cfg["agency_binary"], "--config", cfg["agency_config"]]
    publication = ["availability", "--confirm-public-disclosure", "--pds", cfg["pds"],
                   "--did", cfg["did"], "--capability", cfg["help_capability"],
                   "--token-env", cfg["pds_token_env"]]

    def call(*argv):
        result = subprocess.run([*cli, *argv], capture_output=True, check=True, timeout=40)
        return json.loads(result.stdout)

    call(*publication, "--on", "--expires-at", template["deadline"])
    with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as request:
        json.dump(template, request)
        request.flush()
        task = call("submit", "--peer", cfg["local_peer"], "--file", request.name)
    task_id = task["contract"]["id"]
    last = None
    while True:
        # Read one task. List pagination could otherwise hide an older rollout.
        current = call("task", "--peer", cfg["local_peer"], "--id", task_id)
        status = current["contract"]["status"]
        if status != last:
            print(json.dumps({"task_id": task_id, "status": status,
                              "question": current["contract"].get("clarification")}), flush=True)
            last = status
        if status in {"completed", "failed", "rejected", "canceled", "input_required"}:
            call(*publication, "--off")
            return 0 if status == "completed" else 1
        time.sleep(2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    gate = commands.add_parser("certify")
    gate.add_argument("--version", required=True)
    gate.add_argument("--release-url", required=True)
    for name in ("artifact", "artifact-test", "upgrade-test", "receipt"):
        gate.add_argument("--" + name, type=Path, required=True)
    run = commands.add_parser("deploy")
    run.add_argument("--rollout", type=Path, required=True)
    run.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    return certify(args) if args.command == "certify" else deploy(args)


if __name__ == "__main__":
    raise SystemExit(main())
