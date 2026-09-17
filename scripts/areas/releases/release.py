#!/usr/bin/env python3
"""Create or authenticate/stage local release artifacts. Never install or execute."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile

FORMAT = "memory-release-v1"
ARCHITECTURES = {"amd64", "arm64"}
KINDS = {"binary", "debian-package", "archive"}
MAX_ARTIFACT_BYTES = 16 * 1024**3
REVISION = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
FINGERPRINT = re.compile(r"(?:[0-9A-F]{40}|[0-9A-F]{64})\Z")
NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,179}\Z")
RESERVED_NAMES = {"manifest.json", "manifest.sig", "signing-public-key.asc", "verification.json"}
PI_SOURCE_REVISION = "7c018475d54d4e09c907691e47a520f1f09e3288"
PI_SOURCE_REPOSITORY = "https://github.com/Dicklesworthstone/pi_agent_rust"
PI_SOURCE_URLS = {
    f"{PI_SOURCE_REPOSITORY}/commit/{PI_SOURCE_REVISION}",
    f"{PI_SOURCE_REPOSITORY}/archive/{PI_SOURCE_REVISION}.tar.gz",
}


class ReleaseError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise ReleaseError(message)


def exact_fields(value, fields, label):
    require(isinstance(value, dict) and set(value) == set(fields), f"Unexpected or missing {label} fields")


def natural(value):
    return type(value) is int and 0 <= value <= 2**53 - 1


def digest(value):
    return isinstance(value, str) and DIGEST.fullmatch(value) is not None


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate JSON object key")
        result[key] = value
    return result


def validate(manifest):
    exact_fields(manifest, {"format", "release_version", "source_revision", "platform", "schema", "upgrade", "pi", "artifacts"}, "manifest")
    require(manifest["format"] == FORMAT, "Unsupported manifest format")
    require(isinstance(manifest["release_version"], str) and
            re.fullmatch(r"2\.1\.0-rc\.[1-9][0-9]{0,5}", manifest["release_version"]), "Expected a v2.1.0 release candidate version")
    require(isinstance(manifest["source_revision"], str) and REVISION.fullmatch(manifest["source_revision"]), "Invalid full source revision")
    require(manifest["platform"] == "linux", "Only Linux release artifacts are supported")
    schema = manifest["schema"]
    exact_fields(schema, {"floor", "head"}, "schema")
    require(natural(schema["floor"]) and natural(schema["head"]) and schema["floor"] <= schema["head"], "Invalid schema interval")
    upgrade = manifest["upgrade"]
    exact_fields(upgrade, {"classification", "migrations", "trust_changes", "permission_changes"}, "upgrade")
    require(upgrade["classification"] in ("maintenance", "approval-required"), "Unknown upgrade classification")
    require(all(type(upgrade[field]) is bool for field in ("migrations", "trust_changes", "permission_changes")), "Upgrade change flags must be booleans")
    if upgrade["classification"] == "maintenance":
        require(not any(upgrade[field] for field in ("migrations", "trust_changes", "permission_changes"))
                and schema["floor"] == schema["head"], "Maintenance must use one unchanged schema and make no migration, trust or permission changes")
    if schema["floor"] != schema["head"]:
        require(upgrade["migrations"], "A schema transition must explicitly declare migrations")
    pi = manifest["pi"]
    exact_fields(pi, {"version", "source_url", "source_revision", "source_sha256", "artifact_sha256"}, "Pi pin")
    require(pi["version"] == "0.5.1", "This manifest format pins Pi Rust v0.5.1")
    require(pi["source_revision"] == PI_SOURCE_REVISION, "Pi v0.5.1 source revision differs from the reviewed pin")
    require(isinstance(pi["source_url"], str) and pi["source_url"] in PI_SOURCE_URLS,
            "Pi source must be the reviewed official commit or its exact source archive")
    require(pi["source_sha256"] is None or digest(pi["source_sha256"]), "Invalid optional Pi source digest")
    require(isinstance(pi["artifact_sha256"], dict) and set(pi["artifact_sha256"]).issubset(ARCHITECTURES)
            and all(digest(value) for value in pi["artifact_sha256"].values()), "Invalid optional Pi artifact digests")
    artifacts = manifest["artifacts"]
    require(isinstance(artifacts, list) and 2 <= len(artifacts) <= 64, "Expected 2-64 artifacts covering both supported architectures")
    names = set()
    architectures = set()
    for artifact in artifacts:
        exact_fields(artifact, {"name", "architecture", "kind", "size_bytes", "sha256"}, "artifact")
        name = artifact["name"]
        require(isinstance(name, str) and NAME.fullmatch(name) and ".." not in name and name.casefold() not in RESERVED_NAMES,
                "Artifact name must be a safe basename, not a path or reserved evidence name")
        require(name.casefold() not in names, "Duplicate artifact name")
        names.add(name.casefold())
        require(isinstance(artifact["architecture"], str) and artifact["architecture"] in ARCHITECTURES, "Unsupported artifact architecture")
        require(isinstance(artifact["kind"], str) and artifact["kind"] in KINDS, "Unsupported artifact kind")
        require(natural(artifact["size_bytes"]) and 0 < artifact["size_bytes"] <= MAX_ARTIFACT_BYTES, "Invalid artifact size")
        require(digest(artifact["sha256"]), "Invalid artifact digest")
        architectures.add(artifact["architecture"])
    require(architectures == ARCHITECTURES, "Release must include AMD64 and ARM64 artifacts")
    require(artifacts == sorted(artifacts, key=lambda item: item["name"]), "Artifacts must be sorted by name")
    return manifest


def absolute_parts(path):
    path = Path(path)
    require(path.is_absolute() and ".." not in path.parts, "All input/output paths must be explicit absolute paths without traversal")
    return path, path.parts[1:]


def open_directory(path):
    """Walk using directory descriptors: no symlink ancestors, including races."""
    _, parts = absolute_parts(path)
    descriptor = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in parts:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def open_regular(path):
    path, _ = absolute_parts(path)
    parent = open_directory(path.parent)
    try:
        descriptor = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
    finally:
        os.close(parent)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise ReleaseError("Input must be a regular file")
    return os.fdopen(descriptor, "rb")


def read_small(path, limit):
    with open_regular(path) as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "Metadata file exceeds the size limit")
    return data


def inode_identity(metadata):
    return metadata.st_dev, metadata.st_ino


def is_private_directory(metadata):
    return stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == os.geteuid() and stat.S_IMODE(metadata.st_mode) == 0o700


def write_new(directory, name, contents, created_files=None):
    descriptor = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
    with os.fdopen(descriptor, "wb") as stream:
        if created_files is not None:
            created_files[name] = inode_identity(os.fstat(stream.fileno()))
        stream.write(contents)
        stream.flush()
        os.fsync(stream.fileno())


def measure(path):
    checksum = hashlib.sha256()
    size = 0
    with open_regular(path) as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(chunk)
            size += len(chunk)
            require(size <= MAX_ARTIFACT_BYTES, "Artifact exceeds the size limit")
    return size, checksum.hexdigest()


def create(arguments):
    artifacts = []
    for value in arguments.artifact:
        fields = value.split(":", 2)
        require(len(fields) == 3, "Artifact input must be architecture:kind:absolute-path")
        architecture, kind, path = fields
        size, checksum = measure(path)
        artifacts.append({"name": Path(path).name, "architecture": architecture, "kind": kind,
                          "size_bytes": size, "sha256": checksum})
    pi_hashes = {}
    for value in arguments.pi_artifact_sha256:
        architecture, separator, checksum = value.partition(":")
        require(separator and architecture not in pi_hashes, "Invalid or duplicate Pi artifact digest")
        pi_hashes[architecture] = checksum
    manifest = validate({"format": FORMAT, "release_version": arguments.version,
        "source_revision": arguments.source_revision, "platform": "linux",
        "schema": {"floor": arguments.schema_floor, "head": arguments.schema_head},
        "upgrade": {"classification": arguments.classification, "migrations": arguments.migrations,
                    "trust_changes": arguments.trust_changes, "permission_changes": arguments.permission_changes},
        "pi": {"version": "0.5.1", "source_url": arguments.pi_source_url,
               "source_revision": arguments.pi_source_revision, "source_sha256": arguments.pi_source_sha256,
               "artifact_sha256": pi_hashes},
        "artifacts": sorted(artifacts, key=lambda item: item["name"])})
    output, _ = absolute_parts(arguments.output)
    directory = open_directory(output.parent)
    try:
        write_new(directory, output.name, canonical(manifest))
    finally:
        os.close(directory)
    return {"status": "unsigned_manifest_created", "manifest": str(output), "installation_authorized": False}


def gpg(home, options):
    executable = shutil.which("gpg")
    require(executable, "gpg is required for signature verification")
    try:
        result = subprocess.run([executable, "--no-options", "--homedir", str(home), "--batch", "--no-tty",
                "--no-autostart", "--no-auto-key-retrieve", "--no-auto-key-import", *options],
                env={"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LC_ALL": "C", "GNUPGHOME": str(home)},
                cwd=str(home), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ReleaseError("GPG verification command unavailable or timed out") from error
    require(result.returncode == 0, "GPG rejected the supplied key or detached signature")
    return result.stdout.decode("utf-8", errors="strict")


def authenticate(manifest_bytes, signature, public_key, trusted_fingerprint):
    require(FINGERPRINT.fullmatch(trusted_fingerprint), "Supply the full uppercase trusted signing fingerprint separately")
    # Accept only public ASCII armor, rejecting private-key armor before its body
    # is read. The isolated GPG listing additionally rejects secret packets.
    with open_regular(public_key) as stream:
        header = stream.readline(100)
        require(header.rstrip(b"\r\n") == b"-----BEGIN PGP PUBLIC KEY BLOCK-----", "Only explicit ASCII-armored public keys are accepted")
        key_bytes = header + stream.read(1024 * 1024)
        require(len(key_bytes) <= 1024 * 1024, "Public key exceeds size limit")
    signature_bytes = read_small(signature, 64 * 1024)
    require(signature_bytes, "Detached signature is required")
    with tempfile.TemporaryDirectory(prefix="memory-release-keyring-") as directory:
        home = Path(directory)
        os.chmod(home, 0o700)
        descriptor = open_directory(home)
        try:
            write_new(descriptor, "manifest.json", manifest_bytes)
            write_new(descriptor, "manifest.sig", signature_bytes)
            write_new(descriptor, "key.asc", key_bytes)
        finally:
            os.close(descriptor)
        listing = gpg(home, ["--with-colons", "--import-options", "show-only", "--dry-run", "--import", str(home / "key.asc")])
        records = [line.split(":") for line in listing.splitlines()]
        require(not any(row[0] in ("sec", "ssb") for row in records), "Secret key material is not accepted")
        require(sum(row[0] == "pub" for row in records) == 1, "Supply exactly one public primary key and its subkeys")
        fingerprints = {row[9] for row in records if row[0] == "fpr" and len(row) > 9}
        require(trusted_fingerprint in fingerprints, "Supplied public key does not contain the trusted signing fingerprint")
        gpg(home, ["--import", str(home / "key.asc")])
        status = gpg(home, ["--status-fd", "1", "--verify", str(home / "manifest.sig"), str(home / "manifest.json")])
        parsed = [line.split() for line in status.splitlines() if line.startswith("[GNUPG:] ")]
        signatures = [row for row in parsed if len(row) > 2 and row[1] == "VALIDSIG"]
        require(len(signatures) == 1 and signatures[0][2] == trusted_fingerprint,
                "Detached signature must match the exact trusted signing key, including its subkey identity")
        require(not any(row[1] in {"BADSIG", "ERRSIG", "EXPSIG", "EXPKEYSIG", "REVKEYSIG", "NO_PUBKEY", "KEYREVOKED"}
                        for row in parsed if len(row) > 1), "Expired, revoked or invalid signatures are rejected")
        # VALIDSIG fields: fpr date timestamp expiry version reserved pubalgo
        # hashalgo class [primary-fpr]. Require a binary detached signature and
        # SHA-256/384/512/224; SHA-1/MD5 are never accepted.
        require(len(signatures[0]) >= 11 and signatures[0][9] in {"8", "9", "10", "11"}
                and signatures[0][10] == "00", "Unsupported signature digest or class")
    return signature_bytes, key_bytes


def load_manifest(path):
    data = read_small(path, 1024 * 1024)
    try:
        manifest = json.loads(data, object_pairs_hook=unique_object,
                              parse_constant=lambda value: (_ for _ in ()).throw(ReleaseError("Invalid JSON number")))
    except (ValueError, UnicodeError) as error:
        raise ReleaseError("Invalid manifest JSON") from error
    validate(manifest)
    require(canonical(manifest) == data, "Manifest must use the deterministic canonical JSON representation")
    return manifest, data


def stage_matches(parent_fd, name, stage_fd, expected_identity):
    """Fail closed if directory ownership, permissions or name binding changed."""
    try:
        named = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
        opened = os.fstat(stage_fd)
        return (is_private_directory(os.fstat(parent_fd)) and is_private_directory(named)
                and is_private_directory(opened) and inode_identity(named) == expected_identity
                and inode_identity(opened) == expected_identity)
    except OSError:
        return False


def clean_created_files(parent_fd, name, stage_fd, expected_identity, created_files):
    # No directory listing or recursive cleanup: an unexpected entry belongs to
    # someone else. Private directory ownership excludes other users racing the
    # following checks; identity checks also fail closed on unexpected changes.
    if not stage_matches(parent_fd, name, stage_fd, expected_identity):
        return
    for filename, expected_file in created_files.items():
        try:
            current = os.stat(filename, dir_fd=stage_fd, follow_symlinks=False)
            if stat.S_ISREG(current.st_mode) and inode_identity(current) == expected_file:
                os.unlink(filename, dir_fd=stage_fd)
        except OSError:
            pass  # Preserve uncertain or replaced files; never broaden cleanup.
    if stage_matches(parent_fd, name, stage_fd, expected_identity):
        try:
            os.rmdir(name, dir_fd=parent_fd)
        except OSError:
            pass  # A nonempty/replaced directory is retained, not traversed.


def verify_stage(arguments):
    manifest, data = load_manifest(arguments.manifest)
    require(arguments.architecture in ARCHITECTURES, "Unsupported requested architecture")
    require(natural(arguments.schema_current) and
            manifest["schema"]["floor"] <= arguments.schema_current <= manifest["schema"]["head"],
            "Current database schema is outside the signed compatibility interval; no downgrade is allowed")
    signature, key = authenticate(data, arguments.signature, arguments.public_key, arguments.trusted_signing_fingerprint)
    artifacts_dir, _ = absolute_parts(arguments.artifacts_dir)
    # Validate the directory itself even before individual selected files exist.
    os.close(open_directory(artifacts_dir))
    parent, _ = absolute_parts(arguments.staging_parent)
    parent_fd = open_directory(parent)
    name = "memory-release-" + secrets.token_hex(16)
    selected = [item for item in manifest["artifacts"] if item["architecture"] == arguments.architecture]
    stage_fd = None
    stage_identity = None
    created_files = {}
    try:
        require(is_private_directory(os.fstat(parent_fd)),
                "Staging parent must be owned by the current effective user and have exactly mode 0700")
        free = os.fstatvfs(parent_fd)
        require(sum(item["size_bytes"] for item in selected) + len(data) + len(key) + len(signature) + 65536
                <= free.f_bavail * free.f_frsize, "Insufficient staging space")
        os.mkdir(name, 0o700, dir_fd=parent_fd)
        stage_identity = inode_identity(os.stat(name, dir_fd=parent_fd, follow_symlinks=False))
        stage_fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
        require(stage_matches(parent_fd, name, stage_fd, stage_identity), "New staging directory identity changed")
        for artifact in selected:
            expected_size = artifact["size_bytes"]
            checksum = hashlib.sha256()
            copied = 0
            with open_regular(artifacts_dir / artifact["name"]) as source:
                require(os.fstat(source.fileno()).st_size == expected_size, "Artifact size differs from the signed manifest")
                destination = os.open(artifact["name"], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                      0o600, dir_fd=stage_fd)
                with os.fdopen(destination, "wb") as output:
                    created_files[artifact["name"]] = inode_identity(os.fstat(output.fileno()))
                    for chunk in iter(lambda: source.read(1024 * 1024), b""):
                        copied += len(chunk)
                        require(copied <= expected_size, "Artifact changed while staging")
                        checksum.update(chunk)
                        output.write(chunk)
                    output.flush()
                    os.fsync(output.fileno())
            require(copied == expected_size and checksum.hexdigest() == artifact["sha256"],
                    "Artifact content differs from the signed digest")
        report = {"format": "memory-release-verification-v1", "status": "authenticated_and_staged",
                  "release_version": manifest["release_version"], "source_revision": manifest["source_revision"],
                  "architecture": arguments.architecture, "schema_current": arguments.schema_current,
                  "manifest_sha256": hashlib.sha256(data).hexdigest(), "signing_fingerprint": arguments.trusted_signing_fingerprint,
                  "selected_artifacts": [item["name"] for item in selected],
                  "upgrade_classification": manifest["upgrade"]["classification"],
                  "installation_authorized": False, "archives_unpacked": False,
                  "unselected_architecture_bytes_verified": False,
                  "pi_artifact_digest_present": arguments.architecture in manifest["pi"]["artifact_sha256"]}
        write_new(stage_fd, "manifest.json", data, created_files)
        write_new(stage_fd, "manifest.sig", signature, created_files)
        write_new(stage_fd, "signing-public-key.asc", key, created_files)
        write_new(stage_fd, "verification.json", canonical(report), created_files)
        os.fsync(stage_fd)
        require(stage_matches(parent_fd, name, stage_fd, stage_identity), "Staging directory identity changed before completion")
    except BaseException:
        if stage_identity is not None and stage_fd is not None:
            clean_created_files(parent_fd, name, stage_fd, stage_identity, created_files)
        raise
    finally:
        if stage_fd is not None:
            os.close(stage_fd)
        os.close(parent_fd)
    return {**report, "staging_directory": str(parent / name)}


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    create_parser = commands.add_parser("create", help="Create an unsigned canonical manifest; refuse existing output")
    create_parser.add_argument("--version", required=True)
    create_parser.add_argument("--source-revision", required=True)
    create_parser.add_argument("--schema-floor", type=int, required=True)
    create_parser.add_argument("--schema-head", type=int, required=True)
    create_parser.add_argument("--classification", choices=("maintenance", "approval-required"), required=True)
    for flag in ("migrations", "trust-changes", "permission-changes"):
        create_parser.add_argument("--" + flag, action="store_true")
    create_parser.add_argument("--pi-source-url", required=True)
    create_parser.add_argument("--pi-source-revision", required=True)
    create_parser.add_argument("--pi-source-sha256")
    create_parser.add_argument("--pi-artifact-sha256", action="append", default=[])
    create_parser.add_argument("--artifact", action="append", required=True)
    create_parser.add_argument("--output", required=True)
    verify_parser = commands.add_parser("verify-stage", help="Authenticate and privately stage local files; never authorize installation")
    for flag in ("manifest", "signature", "public-key", "trusted-signing-fingerprint", "artifacts-dir", "staging-parent"):
        verify_parser.add_argument("--" + flag, required=True)
    verify_parser.add_argument("--architecture", choices=sorted(ARCHITECTURES), required=True)
    verify_parser.add_argument("--schema-current", type=int, required=True)
    return root


def main():
    arguments = parser().parse_args()
    try:
        result = create(arguments) if arguments.command == "create" else verify_stage(arguments)
    except (ReleaseError, OSError, UnicodeError) as error:
        # Avoid dumping command stderr or tracebacks, which may include unrelated
        # local environment details. Specific validation messages contain no keys.
        message = str(error) if isinstance(error, ReleaseError) else "Local file or verification operation failed"
        print(json.dumps({"status": "rejected", "reason": message, "installation_authorized": False}), file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
