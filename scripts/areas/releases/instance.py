#!/usr/bin/env python3
"""Prepare an inactive selected release and rehearse filesystem rollback only."""
import argparse
import ctypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import secrets
import stat
import sys
import tarfile

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("codifix_release", Path(__file__).with_name("release.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)
require = release.require
INSTANCE = re.compile(r"[a-z][a-z0-9-]{0,18}\Z")
COMPONENT = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,179}\Z")
MAX_FILES = 8192
MAX_BYTES = 2 * 1024**3
PREPARATION = "instance-preparation.json"


class BoundedTarInfo(tarfile.TarInfo):
    def _proc_member(self, archive):
        # Reject extension headers before tarfile reads a potentially unbounded
        # PAX/GNU metadata body. Ordinary ustar files/directories need no such
        # headers. This hook is exercised against the Debian 12 Python API.
        require(self.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE),
                "Archive extensions, links and special records are forbidden")
        require(0 <= self.size <= MAX_BYTES, "Archive record exceeds the byte limit")
        return super()._proc_member(archive)


def instance_name(value):
    require(isinstance(value, str) and INSTANCE.fullmatch(value), "Expected one named instance, not a service pattern")
    return value


def private_directory(path):
    descriptor = release.open_directory(path)
    if not release.is_private_directory(os.fstat(descriptor)):
        os.close(descriptor)
        raise release.ReleaseError("Working directory must be caller-owned with mode 0700")
    return descriptor


def new_directory(parent_fd, name):
    os.mkdir(name, 0o700, dir_fd=parent_fd)
    identity = release.inode_identity(os.stat(name, dir_fd=parent_fd, follow_symlinks=False))
    descriptor = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
    require(release.stage_matches(parent_fd, name, descriptor, identity), "New directory binding changed")
    return descriptor


def read_json(path):
    data = release.read_small(path, 4 * 1024 * 1024)
    try:
        value = json.loads(data, object_pairs_hook=release.unique_object)
    except (ValueError, UnicodeError) as error:
        raise release.ReleaseError("Invalid preparation metadata") from error
    require(release.canonical(value) == data, "Noncanonical preparation metadata")
    return value


def member_path(member):
    # A deliberately narrow release-tree format, not a general tar installer.
    name = member.name
    path = PurePosixPath(name)
    require(not path.is_absolute() and name == path.as_posix() and len(name) <= 512
            and all(COMPONENT.fullmatch(part) and ".." not in part for part in path.parts),
            "Unsafe archive path")
    require(member.isfile() or member.isdir(), "Archive links, devices and special files are forbidden")
    require(not member.sparse, "Sparse archive files are forbidden")
    allowed = (name in ("bin", "web") if member.isdir() else name == "bin/codifix") or name.startswith("web/")
    require(allowed, "Archive may contain only bin/codifix and web files")
    require(not (member.mode & 0o7000), "Archive special permission bits are forbidden")
    return name


def tree_parent(root_fd, name):
    descriptor = os.dup(root_fd)
    try:
        for part in PurePosixPath(name).parts[:-1]:
            try:
                os.mkdir(part, 0o700, dir_fd=descriptor)
            except FileExistsError:
                pass
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            require(release.is_private_directory(os.fstat(child)), "Unexpected release directory permissions")
            os.close(descriptor)
            descriptor = child
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def unpack(archive_path, artifact, output_fd=None):
    """Hash signed archive bytes and bounded members; never use tar extraction."""
    files = {}
    seen = set()
    total = 0
    with release.open_regular(archive_path) as source:
        require(os.fstat(source.fileno()).st_size == artifact["size_bytes"], "Archive size changed")
        checksum = hashlib.file_digest(source, "sha256").hexdigest()
        require(checksum == artifact["sha256"], "Archive hash differs from the signed manifest")
        source.seek(0)
        try:
            with tarfile.open(fileobj=source, mode="r|*", tarinfo=BoundedTarInfo) as archive:
                for member in archive:
                    require(len(seen) < MAX_FILES, "Archive member limit exceeded")
                    name = member_path(member)
                    require(name.casefold() not in seen, "Duplicate or case-colliding archive path")
                    seen.add(name.casefold())
                    if member.isdir():
                        continue
                    require(0 <= member.size <= MAX_BYTES and total + member.size <= MAX_BYTES,
                            "Expanded archive exceeds the byte limit")
                    total += member.size
                    reader = archive.extractfile(member)
                    require(reader is not None, "Missing archive file body")
                    parent_fd = tree_parent(output_fd, name) if output_fd is not None else None
                    destination = None
                    try:
                        if parent_fd is not None:
                            destination = os.open(PurePosixPath(name).name,
                                                  os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                                  0o600, dir_fd=parent_fd)
                        digest = hashlib.sha256()
                        size = 0
                        while chunk := reader.read(1024 * 1024):
                            size += len(chunk)
                            require(size <= member.size, "Archive member exceeds its declared size")
                            digest.update(chunk)
                            if destination is not None:
                                view = memoryview(chunk)
                                while view:
                                    written = os.write(destination, view)
                                    require(written > 0, "Short release file write")
                                    view = view[written:]
                        require(size == member.size, "Truncated archive member")
                        mode = 0o500 if name == "bin/codifix" else 0o400
                        if destination is not None:
                            os.fchmod(destination, mode)
                            os.fsync(destination)
                        files[name] = {"sha256": digest.hexdigest(), "size_bytes": size, "mode": mode}
                    finally:
                        reader.close()
                        if destination is not None:
                            os.close(destination)
                        if parent_fd is not None:
                            os.fsync(parent_fd)
                            os.close(parent_fd)
        except (tarfile.TarError, EOFError) as error:
            raise release.ReleaseError("Invalid release-tree tar archive") from error
        source.seek(0)
        require(hashlib.file_digest(source, "sha256").hexdigest() == checksum, "Archive changed during preparation")
    require("bin/codifix" in files and files["bin/codifix"]["size_bytes"] > 0
            and "web/index.html" in files and files["web/index.html"]["size_bytes"] > 0,
            "Archive must include a nonempty Codifix binary and web/index.html")
    require(not any(any(other.startswith(name + "/") for other in files) for name in files),
            "Archive file is also a parent directory")
    return dict(sorted(files.items()))


def selected_archive(manifest, architecture, name):
    selected = [item for item in manifest["artifacts"]
                if item["name"] == name and item["architecture"] == architecture and item["kind"] == "archive"]
    require(len(selected) == 1, "Select one signed archive for the requested architecture")
    return selected[0]


def preparation(manifest, manifest_bytes, signer, instance, architecture, schema, artifact, files):
    require(release.natural(schema) and manifest["schema"]["floor"] <= schema <= manifest["schema"]["head"],
            "Schema outside signed compatibility interval; no downgrade is permitted")
    return {"format": "codifix-instance-preparation-v1", "status": "inactive_preparation_only",
            "instance": instance_name(instance), "architecture": architecture, "schema_current_asserted": schema,
            "database_schema_verified": False,
            "schema_compatibility": manifest["schema"], "upgrade": manifest["upgrade"],
            "release_version": manifest["release_version"], "source_revision": manifest["source_revision"],
            "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(), "signing_fingerprint": signer,
            "archive": artifact, "files": files, "installation_authorized": False,
            "pi_included": False, "artifact_execution_tested": False, "host_sandbox_tested": False,
            "plan": {"unit": f"codifix@{instance}.service",
                     "concrete_release": f"/opt/codifix/instances/{instance}/release",
                     "binary_digest_credential": f"/etc/codifix-instances/{instance}/codifix.sha256",
                     "before_switch": ["separate operator approval and verified backup/restore",
                                       "verify actual host ABI and sandbox independently",
                                       "confirm fresh target schema and fence every old writer",
                                       "stop only the named instance and verify no remaining writer",
                                       "reverify signature and all binary/web bytes under the installer trust boundary",
                                       "preserve and authenticate prior concrete release and its digest binding"],
                     "switch": ["stage a concrete directory on the selected destination filesystem",
                                "retain the previous release; never replace a shared package binary",
                                "switch selected release and its digest credential while instance is stopped",
                                "start named instance only after separate activation approval; verify health"],
                     "rollback": ["stop only the selected instance and fence writers",
                                  "re-read actual schema; refuse if outside prior signed compatibility interval",
                                  "never downgrade schema or replay an incompatible prior executable",
                                  "reverify retained binary/web files and restore its matching digest binding",
                                  "retain failed candidate for diagnosis; independently verify restart"],
                     "unimplemented": ["host-privileged installer and credential switch",
                                       "systemd stopping, writer fencing and health checks",
                                       "database migration, data rollback and CA isolation enforcement"]}}


def prepare(arguments):
    instance_name(arguments.instance)
    # This reuses every existing signature, schema, copy and symlink check.
    staged = release.verify_stage(arguments)
    root = Path(staged["staging_directory"])
    root_fd = private_directory(root)
    try:
        manifest, data = release.load_manifest(root / "manifest.json")
        require(hashlib.sha256(data).hexdigest() == staged["manifest_sha256"],
                "Authenticated manifest changed before preparation")
        artifact = selected_archive(manifest, arguments.architecture, arguments.release_archive)
        tree_fd = new_directory(root_fd, "release")
        try:
            files = unpack(root / artifact["name"], artifact, tree_fd)
            os.fsync(tree_fd)
        finally:
            os.close(tree_fd)
        report = preparation(manifest, data, arguments.trusted_signing_fingerprint, arguments.instance,
                             arguments.architecture, arguments.schema_current, artifact, files)
        release.write_new(root_fd, PREPARATION, release.canonical(report))
        os.fsync(root_fd)
    finally:
        os.close(root_fd)
    return {**report, "preparation_directory": str(root)}


def verify_tree(root, expected):
    """Check the complete tree, including unexpected files, links and hardlinks."""
    observed = {}
    root_fd = release.open_directory(root)

    def walk(directory, prefix):
        require(release.is_private_directory(os.fstat(directory)), "Changed release directory permissions")
        for name in os.listdir(directory):
            path = prefix + name
            metadata = os.stat(name, dir_fd=directory, follow_symlinks=False)
            if stat.S_ISDIR(metadata.st_mode):
                require(any(key.startswith(path + "/") for key in expected), "Unexpected release directory")
                child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
                try:
                    require(release.inode_identity(os.fstat(child)) == release.inode_identity(metadata),
                            "Release directory binding changed")
                    walk(child, path + "/")
                finally:
                    os.close(child)
            else:
                require(path in expected and stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1,
                        "Unexpected, linked or nonregular release file")
                descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
                with os.fdopen(descriptor, "rb") as stream:
                    current = os.fstat(stream.fileno())
                    require(release.inode_identity(current) == release.inode_identity(metadata)
                            and current.st_uid == os.geteuid(), "Release file binding or owner changed")
                    require(current.st_size == expected[path]["size_bytes"], "Release file size changed")
                    observed[path] = {"sha256": hashlib.file_digest(stream, "sha256").hexdigest(),
                                      "size_bytes": current.st_size, "mode": stat.S_IMODE(current.st_mode)}

    try:
        walk(root_fd, "")
    finally:
        os.close(root_fd)
    require(observed == expected, "Release binary/web contents or permissions changed")


def verify_prepared(path, signer, instance, schema):
    root = Path(path)
    os.close(private_directory(root))
    saved = read_json(root / PREPARATION)
    manifest, data = release.load_manifest(root / "manifest.json")
    release.authenticate(data, root / "manifest.sig", root / "signing-public-key.asc", signer)
    require(isinstance(saved, dict) and saved.get("instance") == instance_name(instance), "Wrong selected instance")
    architecture = saved.get("architecture")
    require(isinstance(architecture, str) and architecture in release.ARCHITECTURES, "Unknown prepared architecture")
    archive = saved.get("archive", {})
    require(isinstance(archive, dict), "Invalid prepared archive binding")
    artifact = selected_archive(manifest, architecture, archive.get("name"))
    files = unpack(root / artifact["name"], artifact)
    expected = preparation(manifest, data, signer, instance, architecture, saved.get("schema_current_asserted"), artifact, files)
    require(saved == expected, "Preparation metadata differs from authenticated derivation")
    require(manifest["schema"]["floor"] <= schema <= manifest["schema"]["head"],
            "Current schema incompatible with release; rollback cannot downgrade a database")
    verify_tree(root / "release", files)
    return expected


def directory_identity(parent_fd, name):
    metadata = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    require(release.is_private_directory(metadata), "Expected caller-owned concrete private release directory")
    return release.inode_identity(metadata)


def absent(parent_fd, name):
    try:
        os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except FileNotFoundError:
        return True
    return False


def rename_noreplace(parent_fd, source, destination):
    # Linux is the only supported release platform. Refuse if libc lacks the
    # atomic no-replacement operation; a check followed by rename is unsafe.
    libc = ctypes.CDLL(None, use_errno=True)
    operation = getattr(libc, "renameat2", None)
    require(operation is not None, "Atomic renameat2 is required; no unsafe rename fallback")
    operation.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    operation.restype = ctypes.c_int
    if operation(parent_fd, os.fsencode(source), parent_fd, os.fsencode(destination), 1) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))


def exchange(parent_fd, active, incoming, retained, active_identity, incoming_identity, after_retire=None):
    """Filesystem-only, stopped-instance simulation. Never overwrite a target."""
    require(directory_identity(parent_fd, active) == active_identity
            and directory_identity(parent_fd, incoming) == incoming_identity and absent(parent_fd, retained),
            "Release binding changed before exchange")
    rename_noreplace(parent_fd, active, retained)
    os.fsync(parent_fd)
    try:
        if after_retire:
            after_retire()
        require(absent(parent_fd, active) and directory_identity(parent_fd, incoming) == incoming_identity,
                "Release destination or incoming binding changed")
        rename_noreplace(parent_fd, incoming, active)
        os.fsync(parent_fd)
    except BaseException:
        if absent(parent_fd, active) and directory_identity(parent_fd, retained) == active_identity:
            rename_noreplace(parent_fd, retained, active)
            os.fsync(parent_fd)
        raise


def rehearse(arguments):
    instance = instance_name(arguments.instance)
    require(release.natural(arguments.schema_current), "Invalid observed schema")
    prior = verify_prepared(arguments.previous, arguments.previous_trusted_signing_fingerprint,
                            instance, arguments.schema_current)
    candidate = verify_prepared(arguments.prepared, arguments.trusted_signing_fingerprint,
                                instance, arguments.schema_current)
    require(candidate["architecture"] == prior["architecture"], "Architecture differs between releases")
    require(release.version_precedence(candidate["release_version"]) > release.version_precedence(prior["release_version"]),
            "Forward rehearsal requires a strictly newer release, not an equal version or downgrade")
    require(all(not item["upgrade"]["migrations"] and item["schema_compatibility"] ==
                {"floor": arguments.schema_current, "head": arguments.schema_current}
                for item in (prior, candidate)),
            "Filesystem rollback rehearsal requires two unchanged-schema releases; migration rollback is not implemented")
    parent = Path(arguments.output_parent)
    parent_fd = private_directory(parent)
    name = "codifix-instance-rehearsal-" + secrets.token_hex(16)
    root_fd = new_directory(parent_fd, name)
    root = parent / name
    try:
        # Write only new private children of the explicit output parent. Live
        # /opt release and /etc credential paths in the plan are never targeted.
        release.write_new(root_fd, "scope.json", release.canonical({"instance": instance,
            "format": "codifix-filesystem-rehearsal-v1", "installation_authorized": False}))
        instance_fd = new_directory(root_fd, instance)
        try:
            for child, source, report in (("release", arguments.previous, prior),
                                          ("incoming", arguments.prepared, candidate)):
                tree_fd = new_directory(instance_fd, child)
                try:
                    files = unpack(Path(source) / report["archive"]["name"], report["archive"], tree_fd)
                    require(files == report["files"], "Archive changed before rehearsal copy")
                    os.fsync(tree_fd)
                finally:
                    os.close(tree_fd)
            previous_identity = directory_identity(instance_fd, "release")
            candidate_identity = directory_identity(instance_fd, "incoming")
            exchange(instance_fd, "release", "incoming", "previous", previous_identity, candidate_identity)
            verify_tree(root / instance / "release", candidate["files"])
            verify_tree(root / instance / "previous", prior["files"])
            exchange(instance_fd, "release", "previous", "retained-candidate", candidate_identity, previous_identity)
            verify_tree(root / instance / "release", prior["files"])
            verify_tree(root / instance / "retained-candidate", candidate["files"])
        finally:
            os.close(instance_fd)
        result = {"format": "codifix-filesystem-rehearsal-v1", "status": "filesystem_switch_and_rollback_verified",
                  "instance": instance, "schema_current_asserted": arguments.schema_current,
                  "database_schema_verified": False,
                  "previous_manifest_sha256": prior["manifest_sha256"],
                  "candidate_manifest_sha256": candidate["manifest_sha256"],
                  "prepared_sources_unchanged": True, "installation_authorized": False,
                  "service_or_binary_executed": False, "database_contacted": False,
                  "credential_switch_tested": False, "host_sandbox_tested": False}
        verify_tree(Path(arguments.previous) / "release", prior["files"])
        verify_tree(Path(arguments.prepared) / "release", candidate["files"])
        release.write_new(root_fd, "result.json", release.canonical(result))
        os.fsync(root_fd)
        require(release.stage_matches(parent_fd, name, root_fd, release.inode_identity(os.fstat(root_fd))),
                "Rehearsal directory binding changed")
    finally:
        os.close(root_fd)
        os.close(parent_fd)
    return {**result, "rehearsal_directory": str(root)}


def parser():
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="Authenticate and prepare one inactive concrete release")
    for flag in ("manifest", "signature", "public-key", "trusted-signing-fingerprint", "artifacts-dir",
                 "staging-parent", "instance", "release-archive"):
        prep.add_argument("--" + flag, required=True)
    prep.add_argument("--architecture", choices=sorted(release.ARCHITECTURES), required=True)
    prep.add_argument("--schema-current", type=int, required=True)
    rehearsal = commands.add_parser("rehearse", help="Switch and roll back copies inside a fresh disposable root only")
    for flag in ("previous", "prepared", "previous-trusted-signing-fingerprint", "trusted-signing-fingerprint",
                 "instance", "output-parent"):
        rehearsal.add_argument("--" + flag, required=True)
    rehearsal.add_argument("--schema-current", type=int, required=True)
    return root


def main():
    arguments = parser().parse_args()
    try:
        result = prepare(arguments) if arguments.command == "prepare" else rehearse(arguments)
    except (release.ReleaseError, OSError, UnicodeError) as error:
        print(json.dumps({"status": "rejected", "installation_authorized": False,
                          "reason": str(error) if isinstance(error, release.ReleaseError)
                          else "Local preparation or rehearsal operation failed"}), file=sys.stderr)
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
