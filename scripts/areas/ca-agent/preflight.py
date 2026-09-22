#!/usr/bin/python3
"""Verify the fixed CA handoff without installation, database access or networking.

Writes only a fresh private evidence/staging directory. The schema-60 assertion
exercises the signed release, not the schema of an observed database. Run as the
ordinary local operator, never as root or against a concurrently modified bundle.
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import subprocess
import sys
import tempfile

SIGNER = "4EB55288032C60EF65C73A0352EE81AD6D98F325"
SOURCE = "b35faf2983d20c69bc086fb9bcb7e95c51bd8f47"
PINS = {
    "SHA256SUMS": "de6339d324bff0a2abd9ff4f1aa697692b7e10aed69d2417baad21b7662517fb",
    "manifest.json": "5d06127ec4db38e0d56eb0942a41f26bee32987645f1bf81391fb929e19ee1d4",
    "manifest.sig": "8f1efa484c661533b1f271690dcfa95d83a3de263835accfcaabf5d22692d215",
    "signing-public-key.asc": "a00d7f6f5e1ac2664da6e956237c871440df7932e31ed29c0af19b071a94f76c",
    "tools/release.py": "669eb1a491286a1296e5f10d543771497371744005f0257f79d441e846319af7",
}
CLEAN_ENV = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "LANG": "C"}


class Blocked(Exception):
    """A sanitized, actionable refusal."""


def require(condition, reason):
    if not condition:
        raise Blocked(reason)


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def regular_member(root, name):
    parts = PurePosixPath(name)
    require(not parts.is_absolute() and parts.parts and
            all(part not in (".", "..") for part in parts.parts) and
            str(parts) == name and "\\" not in name, "unsafe_inventory_path")
    current = root
    for part in parts.parts:
        current = current / part
        require(not current.is_symlink(), "symlink_in_release")
    require(stat.S_ISREG(current.stat().st_mode), "non_regular_release_member")
    return current


def verify_inventory(root):
    require(root.is_dir() and not root.is_symlink(), "release_directory_missing_or_linked")
    for name, expected in PINS.items():
        require(digest(regular_member(root, name)) == expected, "pinned_release_input_changed")
    entries = {}
    for line in (root / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (.+)", line)
        require(match is not None, "malformed_checksum_inventory")
        expected, name = match.groups()
        require(name not in entries, "duplicate_inventory_member")
        require(digest(regular_member(root, name)) == expected, "release_checksum_mismatch")
        entries[name] = expected
    require(len(entries) == 55 and all(name in entries for name in PINS if name != "SHA256SUMS"),
            "incomplete_release_inventory")
    members = set()
    for path in root.rglob("*"):
        require(not path.is_symlink(), "symlink_in_release")
        if not path.is_dir():
            require(path.is_file(), "non_regular_release_member")
            members.add(path.relative_to(root).as_posix())
    require(members == set(entries) | {"SHA256SUMS"}, "unexpected_release_members")
    return len(entries)


def host_compatibility():
    machine = platform.machine()
    architecture = {"x86_64": "amd64", "aarch64": "arm64"}.get(machine)
    require(architecture is not None, "unsupported_native_architecture")
    require(sys.version_info >= (3, 11), "python_3_11_or_newer_required")
    try:
        libc = os.confstr("CS_GNU_LIBC_VERSION") or ""
    except (AttributeError, OSError, ValueError):
        libc = ""
    match = re.fullmatch(r"glibc ([0-9]+)\.([0-9]+)", libc)
    require(match is not None, "glibc_version_unknown")
    require(tuple(map(int, match.groups())) >= (2, 36), "glibc_below_tested_2_36_floor")
    gpg = subprocess.run(["/usr/bin/gpg", "--version"], env=CLEAN_ENV, cwd="/",
                         stdin=subprocess.DEVNULL, capture_output=True, text=True,
                         timeout=10, check=False)
    version = re.match(r"gpg \(GnuPG\) ([0-9.]+)", gpg.stdout)
    require(gpg.returncode == 0 and version is not None, "gpg_version_unavailable")
    return {"architecture": architecture, "glibc": match[0],
            "python": platform.python_version(), "gpg": version[1],
            "native_memory_execution_tested": False, "native_sandbox_tested": False}


def verify_signature(root, evidence, architecture):
    # Execute a newly copied, rehashed verifier from our own private directory,
    # never Python from the writable USB or the caller's module search path.
    verifier = evidence / "release.py"
    verifier.write_bytes(regular_member(root, "tools/release.py").read_bytes())
    require(digest(verifier) == PINS["tools/release.py"], "verifier_changed_during_copy")
    result = subprocess.run([
        sys.executable, "-I", "-S", str(verifier), "verify-stage",
        "--manifest", str(root / "manifest.json"),
        "--signature", str(root / "manifest.sig"),
        "--public-key", str(root / "signing-public-key.asc"),
        "--trusted-signing-fingerprint", SIGNER,
        "--architecture", architecture, "--schema-current", "60",
        "--artifacts-dir", str(root), "--staging-parent", str(evidence),
    ], env=CLEAN_ENV, cwd=str(evidence), stdin=subprocess.DEVNULL,
        capture_output=True, text=True, timeout=180, check=False)
    # The frozen verifier sanitizes errors. Retain its result locally, not as
    # unrestricted diagnostic output to paste into Notion.
    (evidence / "signature.stdout.json").write_text(result.stdout, encoding="utf-8")
    (evidence / "signature.stderr.json").write_text(result.stderr, encoding="utf-8")
    require(result.returncode == 0, "signature_or_staging_rejected_see_local_receipt")
    report = json.loads(result.stdout)
    require(report.get("status") == "authenticated_and_staged" and
            report.get("source_revision") == SOURCE and
            report.get("signing_fingerprint") == SIGNER and
            report.get("manifest_sha256") == PINS["manifest.json"] and
            report.get("architecture") == architecture and
            report.get("installation_authorized") is False,
            "unexpected_signature_verification_result")
    return {"status": "authenticated_and_staged", "signing_subkey": SIGNER,
            "receipt_sha256": digest(evidence / "signature.stdout.json")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-dir", type=Path,
                        default=Path(__file__).resolve().parent / "resources/release")
    parser.add_argument("--output-parent", type=Path, default=Path(tempfile.gettempdir()))
    args = parser.parse_args()
    evidence = None
    report = {"format": "memory-ca-agent-preflight-v1", "status": "blocked",
              "observed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "release": "2.1.0-rc.1", "source_revision": SOURCE,
              "manifest_sha256": PINS["manifest.json"],
              "schema": {"release_test_assertion": 60, "database_observed": False},
              "database_accessed": False, "services_changed": False,
              "installation_authorized": False, "activation_authorized": False}
    try:
        require(os.geteuid() != 0, "run_as_ordinary_operator_not_root")
        os.umask(0o077)
        parent = args.output_parent.absolute()
        require(parent.is_dir() and not parent.is_symlink(), "output_parent_missing_or_linked")
        evidence = Path(tempfile.mkdtemp(prefix="memory-ca-preflight-", dir=parent))
        report["host"] = host_compatibility()
        root = args.release_dir.absolute()
        report["checksum_files_verified"] = verify_inventory(root)
        report["signature"] = verify_signature(root, evidence, report["host"]["architecture"])
        report["status"] = "release_preflight_passed_not_installed"
    except Blocked as error:
        report["reason"] = str(error)
    except subprocess.TimeoutExpired:
        report["reason"] = "verification_timed_out_evidence_retained"
    except (OSError, ValueError, UnicodeError):
        report["reason"] = "local_file_tool_or_output_error_evidence_retained"
    if evidence is not None:
        target = evidence / "preflight.json"
        target.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print("Private receipt: " + str(target), file=sys.stderr)
        print("Receipt SHA-256: " + digest(target), file=sys.stderr)
    print(json.dumps(report, sort_keys=True))
    return 0 if report["status"] == "release_preflight_passed_not_installed" else 2


if __name__ == "__main__":
    sys.exit(main())
