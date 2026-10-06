#!/usr/bin/env python3
"""Offline, exact-manifest native runtime staging; never execute bundle content.

The producer must build a reviewed redistributable tree for its exact installed
prefix. Hash validation is not Mac loader, architecture or relocation acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

sys.path.insert(0, str(Path(__file__).absolute().parent))
from onboarding_files import Refused, encoded, opened, parent_descriptor, path

TOOLS = {"postgres", "initdb", "pg_ctl", "psql", "pg_config", "pg_controldata"}
BASE = "/usr/local/share/memory-layer/runtime"
DESCRIPTOR = "runtime-prerequisites.json"
MANIFEST = "runtime-manifest.json"
MAX_FILES = 32768
MAX_FILE_BYTES = 1024 * 1024 * 1024
MAX_TOTAL_BYTES = 4 * MAX_FILE_BYTES


def exact(value, keys):
    if not isinstance(value, dict) or set(value) != set(keys):
        raise Refused("runtime manifest fields differ from bounded schema v1")


def relative(value):
    if not isinstance(value, str) or len(value) > 1024 or not value:
        raise Refused("bounded ASCII relative runtime path required")
    parts = value.split("/")
    if len(parts) > 32 or any(not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+\-]{0,127}", p) or p.endswith(".") for p in parts):
        raise Refused("runtime traversal, symlink-style or ambiguous alias path")
    return value


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refused("duplicate runtime manifest keys")
        result[key] = value
    return result


def load_public(file, limit):
    # Build metadata may be public, but must be a trusted no-follow regular file.
    with opened(file, protected=False) as stream:
        if os.fstat(stream.fileno()).st_uid not in {0, os.getuid()}:
            raise Refused("runtime manifest owner is untrusted")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise Refused("runtime metadata exceeds safe bounds")
    try:
        value = json.loads(data, object_pairs_hook=unique)
    except (ValueError, UnicodeError, RecursionError):
        raise Refused("runtime metadata JSON unreadable") from None
    return value, data


def load_manifest(file, arch):
    value, data = load_public(file, 8 * 1024 * 1024)
    validate_manifest(value, arch)
    return value, data


def validate_manifest(value, arch):
    exact(value, {"schema_version", "bundle_id", "arch", "install_prefix", "relocatable_for_prefix",
                  "python", "postgres", "files", "notices"})
    if type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise Refused("runtime manifest schema_version must be 1")
    bundle = value["bundle_id"]
    if not isinstance(bundle, str) or not re.fullmatch(r"[a-z0-9][a-z0-9_\-]{0,79}", bundle):
        raise Refused("bounded lower ASCII bundle_id required")
    if arch not in {"aarch64", "x86_64"} or value["arch"] != arch:
        raise Refused("runtime bundle architecture differs from package architecture")
    if value["install_prefix"] != f"{BASE}/{bundle}" or value["relocatable_for_prefix"] is not True:
        raise Refused("runtime must explicitly be built relocatable for exact versioned install prefix")
    entries = value["files"]
    if not isinstance(entries, dict) or not 1 <= len(entries) <= MAX_FILES:
        raise Refused("runtime requires a bounded exhaustive file inventory")
    aliases, directories = set(), set()
    for name, entry in entries.items():
        relative(name)
        exact(entry, {"sha256", "mode"})
        if not isinstance(entry["sha256"], str) or not re.fullmatch(r"[a-f0-9]{64}", entry["sha256"]) or not isinstance(entry["mode"], str) or entry["mode"] not in {"0444", "0555"}:
            raise Refused("runtime files require exact SHA-256 and immutable 0444/0555 permissions")
        if name.lower() in aliases:
            raise Refused("runtime case aliases are unsupported")
        aliases.add(name.lower())
        for parent in Path(name).parents:
            if str(parent) != ".":
                directories.add(str(parent))
    all_paths = list(entries) + list(directories) + [DESCRIPTOR, MANIFEST]
    if len(directories) > MAX_FILES or len({p.lower() for p in all_paths}) != len(all_paths):
        raise Refused("runtime file/directory/reserved-name aliases are unsupported")
    python = value["python"]
    exact(python, {"path", "version"})
    if not isinstance(python["version"], str) or not re.fullmatch(r"3\.(11|12|13|14)\.[0-9]{1,3}", python["version"]):
        raise Refused("runtime Python requires an exact reviewed 3.11..3.14 patch version")
    pg = value["postgres"]
    exact(pg, {"bin_dir", "version", "vector_library", "vector_control", "vector_version", "vector_sql"})
    if not isinstance(pg["version"], str) or not re.fullmatch(r"(16|17|18)\.[0-9]{1,3}", pg["version"]):
        raise Refused("runtime PostgreSQL requires an exact reviewed 16..18 patch version")
    if not isinstance(pg["vector_version"], str) or not re.fullmatch(r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}", pg["vector_version"]):
        raise Refused("runtime pgvector requires an exact version")
    bin_dir = relative(pg["bin_dir"])
    if not isinstance(pg["vector_sql"], list) or not 1 <= len(pg["vector_sql"]) <= 128 or any(not isinstance(n, str) for n in pg["vector_sql"]) or len(set(pg["vector_sql"])) != len(pg["vector_sql"]):
        raise Refused("runtime requires bounded unique pgvector SQL paths")
    control = relative(pg["vector_control"])
    library = relative(pg["vector_library"])
    if Path(control).name != "vector.control" or Path(library).name != "vector.so":
        raise Refused("runtime pgvector control/library layout unsupported")
    for name in pg["vector_sql"]:
        relative(name)
        if Path(name).parent != Path(control).parent or not re.fullmatch(r"vector--[0-9.\-]+\.sql", Path(name).name):
            raise Refused("runtime pgvector SQL must belong to pinned extension directory")
    required_sql = str(Path(control).parent / f"vector--{pg['vector_version']}.sql")
    if required_sql not in pg["vector_sql"]:
        raise Refused("runtime exact pgvector installation SQL missing")
    executables = [relative(python["path"]), *[f"{bin_dir}/{tool}" for tool in sorted(TOOLS)]]
    for name in executables:
        if name not in entries or entries[name]["mode"] != "0555":
            raise Refused("runtime pinned Python/PostgreSQL executable missing or not executable")
    notices = value["notices"]
    if not isinstance(notices, list) or not 1 <= len(notices) <= 128 or any(not isinstance(n, str) for n in notices) or len(set(notices)) != len(notices):
        raise Refused("runtime requires bounded hash-bound redistribution notices")
    for name in [library, control, *pg["vector_sql"], *notices]:
        relative(name)
        if name not in entries:
            raise Refused("runtime prerequisite/notice is not hash-bound")
    for name in notices:
        if entries[name]["mode"] != "0444":
            raise Refused("runtime notices must be read-only data")
    return directories


def trusted_directory(fd):
    info = os.fstat(fd)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid not in {0, os.getuid()} or stat.S_IMODE(info.st_mode) & ~0o755:
        raise Refused("runtime directory ownership or permissions are unsafe")


def open_directory(file):
    file = path(file)
    parent = parent_descriptor(file)
    try:
        fd = os.open(file.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent)
    finally:
        os.close(parent)
    try:
        trusted_directory(fd)
        return fd
    except BaseException:
        os.close(fd)
        raise


def tree(root_fd, value, destination_fd=None, *, reserved=None, immutable_directories=False):
    """Inspect/copy from pinned directory descriptors, never pathname follows."""
    expected_dirs = validate_manifest(value, value["arch"])
    entries = dict(value["files"])
    entries.update(reserved or {})
    seen_files, seen_dirs, total = set(), set(), 0

    def visit(source_fd, prefix, target_fd):
        nonlocal total
        trusted_directory(source_fd)
        if immutable_directories and stat.S_IMODE(os.fstat(source_fd).st_mode) != 0o555:
            raise Refused("installed runtime directory is not immutable 0555")
        names = os.listdir(source_fd)
        if len(names) > MAX_FILES:
            raise Refused("runtime directory inventory exceeds bounds")
        for name in sorted(names):
            rel = relative(f"{prefix}/{name}" if prefix else name)
            info = os.stat(name, dir_fd=source_fd, follow_symlinks=False)
            if stat.S_ISDIR(info.st_mode):
                if rel not in expected_dirs:
                    raise Refused("runtime contains unmanifested directory")
                seen_dirs.add(rel)
                child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=source_fd)
                output = None
                try:
                    opened_info = os.fstat(child)
                    if (opened_info.st_dev, opened_info.st_ino) != (info.st_dev, info.st_ino):
                        raise Refused("runtime directory changed during protected lookup")
                    if target_fd is not None:
                        os.mkdir(name, 0o700, dir_fd=target_fd)
                        output = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_fd)
                    visit(child, rel, output)
                    if output is not None:
                        os.fchmod(output, 0o555)
                finally:
                    os.close(child)
                    if output is not None:
                        os.close(output)
            elif stat.S_ISREG(info.st_mode):
                if rel not in entries:
                    raise Refused("runtime contains unmanifested file")
                entry = entries[rel]
                if info.st_nlink != 1 or info.st_uid not in {0, os.getuid()} or stat.S_IMODE(info.st_mode) != int(entry["mode"], 8) or info.st_size > MAX_FILE_BYTES:
                    raise Refused("runtime file aliases, ownership, permissions or size unsafe")
                seen_files.add(rel)
                fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=source_fd)
                output = None
                try:
                    before = os.fstat(fd)
                    if (before.st_dev, before.st_ino, before.st_mode, before.st_nlink, before.st_uid, before.st_size) != (info.st_dev, info.st_ino, info.st_mode, info.st_nlink, info.st_uid, info.st_size):
                        raise Refused("runtime file changed during protected lookup")
                    if target_fd is not None:
                        output = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=target_fd)
                    digest, size = hashlib.sha256(), 0
                    while chunk := os.read(fd, 1024 * 1024):
                        size += len(chunk)
                        total += len(chunk)
                        if size > MAX_FILE_BYTES or total > MAX_TOTAL_BYTES:
                            raise Refused("runtime content exceeds safe bounds")
                        digest.update(chunk)
                        if output is not None:
                            view = memoryview(chunk)
                            while view:
                                view = view[os.write(output, view):]
                    after = os.fstat(fd)
                    stable = lambda s: (s.st_dev, s.st_ino, s.st_mode, s.st_nlink, s.st_uid, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
                    if stable(before) != stable(after) or size != before.st_size or digest.hexdigest() != entry["sha256"]:
                        raise Refused("runtime file changed or differs from exact manifest SHA-256")
                    if output is not None:
                        os.fchmod(output, int(entry["mode"], 8))
                        os.fsync(output)
                finally:
                    os.close(fd)
                    if output is not None:
                        os.close(output)
            else:
                raise Refused("runtime symlinks and special files are unsupported")
    visit(root_fd, "", destination_fd)
    if seen_files != set(entries) or seen_dirs != expected_dirs:
        raise Refused("runtime inventory differs from exact manifest")


def descriptor(value, manifest_data):
    prefix, pg, entries = value["install_prefix"], value["postgres"], value["files"]
    installed = lambda name: f"{prefix}/{name}"
    return {"schema_version": 1, "bundle_id": value["bundle_id"], "arch": value["arch"],
            "install_prefix": prefix, "manifest_sha256": hashlib.sha256(manifest_data).hexdigest(),
            "python": {"path": installed(value["python"]["path"]), "version": value["python"]["version"],
                       "sha256": entries[value["python"]["path"]]["sha256"]},
            "postgres": {"bin_dir": installed(pg["bin_dir"]), "major": int(pg["version"].split(".")[0]),
                         "tool_sha256": {tool: entries[f"{pg['bin_dir']}/{tool}"]["sha256"] for tool in sorted(TOOLS)},
                         "vector_library": installed(pg["vector_library"]), "vector_library_sha256": entries[pg["vector_library"]]["sha256"],
                         "vector_control": installed(pg["vector_control"]), "vector_control_sha256": entries[pg["vector_control"]]["sha256"],
                         "vector_version": pg["vector_version"],
                         "vector_sql": {Path(n).name: entries[n]["sha256"] for n in pg["vector_sql"]}},
            "files": {installed(n): entry for n, entry in sorted(entries.items())},
            "notices": [installed(n) for n in value["notices"]],
            "acceptance_pending": ["native_macos_architecture", "loader_dependencies", "exact_prefix_relocatability"]}


def validate_bundle(source, manifest, arch):
    value, data = load_manifest(manifest, arch)
    fd = open_directory(source)
    try:
        tree(fd, value)
    finally:
        os.close(fd)
    return value, data


def verify_installed(descriptor_path, arch):
    """Read-only verification; architecture/loader/native execution stay pending."""
    selected, descriptor_data = load_public(descriptor_path, 16 * 1024 * 1024)
    exact(selected, {"schema_version", "bundle_id", "arch", "install_prefix", "manifest_sha256", "python",
                     "postgres", "files", "notices", "acceptance_pending"})
    bundle_id = selected["bundle_id"]
    if not isinstance(bundle_id, str) or not re.fullmatch(r"[a-z0-9][a-z0-9_\-]{0,79}", bundle_id):
        raise Refused("installed runtime bundle_id is invalid")
    prefix = Path(f"{BASE}/{bundle_id}")
    if selected["install_prefix"] != str(prefix) or str(path(descriptor_path)) != str(prefix / DESCRIPTOR):
        raise Refused("installed runtime descriptor must belong to its exact versioned prefix")
    value, manifest_data = load_manifest(prefix / MANIFEST, arch)
    expected = descriptor(value, manifest_data)
    if selected != expected or descriptor_data != encoded(expected):
        raise Refused("installed runtime descriptor differs from its exact hash-bound manifest")
    reserved = {name: {"sha256": hashlib.sha256(data).hexdigest(), "mode": "0444"}
                for name, data in [(MANIFEST, manifest_data), (DESCRIPTOR, descriptor_data)]}
    fd = open_directory(prefix)
    try:
        tree(fd, value, reserved=reserved, immutable_directories=True)
    finally:
        os.close(fd)
    return selected


def preinstall_guard(value):
    # Manifest prefix contains only fixed literals and bounded lower ASCII ID.
    return ("#!/bin/bash\nset -eu\n"
            "if [[ ${3:-} != / ]]; then\n"
            "  echo 'Runtime package requires the current startup volume (/).' >&2\n  exit 1\nfi\n"
            "for ancestor in /usr /usr/local /usr/local/share /usr/local/share/memory-layer /usr/local/share/memory-layer/runtime; do\n"
            "  if [[ -L $ancestor ]]; then\n"
            "    echo 'Symlinked runtime installation ancestor refused; existing paths preserved.' >&2\n    exit 1\n  fi\ndone\n"
            f"runtime_prefix='{value['install_prefix']}'\n"
            "if [[ -e $runtime_prefix || -L $runtime_prefix ]]; then\n"
            "  echo 'Runtime bundle ID already exists; choose a reviewed new bundle ID. Existing runtime preserved.' >&2\n  exit 1\nfi\n").encode()


def stage_bundle(source, manifest, arch, payload, *, scripts=None):
    value, data = validate_bundle(source, manifest, arch)
    payload = path(payload)
    # Only a private build staging area, never the current host's install root.
    parent_info = payload.parent.stat()
    if payload.name != "payload" or parent_info.st_uid != os.getuid() or stat.S_IMODE(parent_info.st_mode) != 0o700:
        raise Refused("runtime staging requires payload in a current-user private 0700 build directory")
    source = path(source)
    source_info = source.stat()
    payload_info = payload.stat()
    source_identity = (source_info.st_dev, source_info.st_ino)
    payload_identity = (payload_info.st_dev, payload_info.st_ino)
    def identities(file):
        return {(p.stat().st_dev, p.stat().st_ino) for p in [file, *file.parents]}
    if source_identity in identities(payload) or payload_identity in identities(source):
        raise Refused("runtime source and staging payload must be separate")
    target_parent = open_directory(payload)
    source_fd = None
    target_fd = None
    try:
        source_fd = open_directory(source)
        for component in value["install_prefix"].split("/")[1:-1]:
            created = False
            try:
                os.mkdir(component, 0o755, dir_fd=target_parent)
                created = True
            except FileExistsError:
                pass
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_parent)
            try:
                trusted_directory(child)
                if created:
                    os.fchmod(child, 0o755)
            except BaseException:
                os.close(child)
                raise
            os.close(target_parent)
            target_parent = child
        # Create-only even for identical bundles. Partial build stages are not
        # candidates and stay private until the builder's own bounded cleanup.
        os.mkdir(value["bundle_id"], 0o700, dir_fd=target_parent)
        target_fd = os.open(value["bundle_id"], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=target_parent)
        tree(source_fd, value, target_fd)
        tree(source_fd, value)  # Recheck additions/replacements during staging.
        for name, content in [(MANIFEST, data), (DESCRIPTOR, encoded(descriptor(value, data)))]:
            fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=target_fd)
            with os.fdopen(fd, "wb") as stream:
                stream.write(content)
                stream.flush()
                os.fchmod(stream.fileno(), 0o444)
                os.fsync(stream.fileno())
        os.fchmod(target_fd, 0o555)
        if scripts is not None:
            scripts = path(scripts)
            if scripts.parent != payload.parent or scripts.name != "scripts":
                raise Refused("runtime preinstall must be staged in exact private build scripts directory")
            scripts_fd = open_directory(scripts)
            try:
                guard_fd = os.open("preinstall", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o700, dir_fd=scripts_fd)
                with os.fdopen(guard_fd, "wb") as stream:
                    stream.write(preinstall_guard(value))
                    stream.flush()
                    os.fchmod(stream.fileno(), 0o755)
                    os.fsync(stream.fileno())
            finally:
                os.close(scripts_fd)
        return payload / value["install_prefix"].lstrip("/") / DESCRIPTOR
    finally:
        if source_fd is not None:
            os.close(source_fd)
        os.close(target_parent)
        if target_fd is not None:
            os.close(target_fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["validate", "stage"])
    parser.add_argument("--bundle", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--arch", required=True, choices=["aarch64", "x86_64"])
    parser.add_argument("--payload")
    parser.add_argument("--scripts")
    args = parser.parse_args()
    try:
        if sys.version_info < (3, 11):
            raise Refused("runtime bundle validation requires build-host Python 3.11+")
        if args.action == "stage":
            if not args.payload:
                raise Refused("stage requires explicit private --payload")
            print(stage_bundle(args.bundle, args.manifest, args.arch, args.payload, scripts=args.scripts))
        else:
            if args.payload or args.scripts:
                raise Refused("validate does not accept staging outputs")
            value, _ = validate_bundle(args.bundle, args.manifest, args.arch)
            print(f"Hash-bound runtime {value['bundle_id']} validated; native Mac acceptance pending.")
    except (Refused, OSError, ValueError, TypeError, KeyError) as error:
        print(f"Runtime bundle refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
