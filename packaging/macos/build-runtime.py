#!/usr/bin/env python3
"""Native, resumable source producer for the exact-prefix runtime bundle.

Runs only on Darwin; build products remain in the explicit work directory.
Never installs into /usr/local or changes a running installation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

import runtime_bundle as bundle

SOURCE_ORDER = ("openssl", "postgres", "vector", "python")
POSTGRES_CONTRIB = ("pgcrypto", "pg_trgm")
OPENSSL_OPTIONS = ("no-shared", "no-module", "no-tests", "no-apps", "no-docs",
                   "no-atexit", "no-thread-pool", "no-quic")


def sources(file):
    value = json.loads(Path(file).read_text())
    if set(value) != {"schema_version", *SOURCE_ORDER} or value["schema_version"] != 1:
        raise ValueError("unsupported source manifest")
    for key in SOURCE_ORDER:
        item = value[key]
        if set(item) != {"version", "url", "sha256", "directory", "notice"}:
            raise ValueError("unsupported source fields")
        bundle.relative(item["directory"])
        bundle.relative(item["notice"])
        if not item["url"].startswith("https://") or len(item["sha256"]) != 64:
            raise ValueError("HTTPS and SHA-256 required")
        int(item["sha256"], 16)
    return value


def checked_archive(archive, source):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != source["sha256"]:
        raise ValueError("download differs from pinned SHA-256")
    with tarfile.open(archive) as stream:
        for member in stream.getmembers():
            name = Path(member.name)
            if name.is_absolute() or ".." in name.parts or not name.parts or name.parts[0] != source["directory"]:
                raise ValueError("source archive escapes its reviewed directory")
            if member.issym() or member.islnk() or not (member.isdir() or member.isfile()):
                raise ValueError("source archive contains unsupported link or special file")


def run(argv, cwd, env):
    print("Running: " + " ".join(map(str, argv)), flush=True)
    subprocess.run(list(map(str, argv)), cwd=cwd, env=env, check=True)


def vector_commands(pg, prefix, stage, jobs):
    # pg_config relocates its answers to the staged executable. Build against
    # that tree, but give installation the final directories before DESTDIR.
    flags = [f"PG_CONFIG={pg}/bin/pg_config", f"PGXS={pg}/lib/pgxs/src/makefiles/pgxs.mk"]
    build = ["make", f"-j{jobs}", "OPTFLAGS=", *flags,
             f"includedir={pg}/include", f"includedir_server={pg}/include/server"]
    installed = f"{prefix}/postgres"
    install = ["make", "install", f"DESTDIR={stage}", *flags,
               f"pkglibdir={installed}/lib", f"datadir={installed}/share",
               f"docdir={installed}/share/doc", f"includedir={installed}/include",
               f"includedir_server={installed}/include/server"]
    return build, install


def openssl_commands(dependency, arch, jobs):
    target = {"aarch64": "darwin64-arm64-cc", "x86_64": "darwin64-x86_64-cc"}[arch]
    return [
        # libpq rejects any exit-symbol reference, including references pulled
        # in from static dependencies. These supported OpenSSL options remove
        # process-exit cleanup and optional worker pools/QUIC, not thread safety.
        ["./Configure", target, *OPENSSL_OPTIONS,
         f"--prefix={dependency}", "--libdir=lib"],
        ["make", f"-j{jobs}"],
        ["make", "install_dev"],
    ]


def postgres_commands(prefix, stage, dependency, jobs):
    commands = [
        ["./configure", f"--prefix={prefix}/postgres", "--with-ssl=openssl",
         f"--with-includes={dependency}/include", f"--with-libraries={dependency}/lib",
         "--without-readline", "--without-zlib", "--without-icu", "--without-lz4", "--without-zstd"],
        ["make", f"-j{jobs}"],
        ["make", "install", f"DESTDIR={stage}"],
    ]
    for extension in POSTGRES_CONTRIB:
        commands.extend([
            ["make", "-C", f"contrib/{extension}", f"-j{jobs}"],
            ["make", "-C", f"contrib/{extension}", "install", f"DESTDIR={stage}"],
        ])
    return commands


def required_postgres_extensions(pg):
    for extension in (*POSTGRES_CONTRIB, "vector"):
        if not (pg / f"lib/{extension}.dylib").is_file() or not (pg / f"share/extension/{extension}.control").is_file() or not any((pg / "share/extension").glob(f"{extension}--*.sql")):
            raise ValueError(f"required PostgreSQL extension missing from runtime: {extension}")


def smoke_postgres_extensions(pg, vector_version, env):
    """Use a private disposable socket/cluster; never contact installed services."""
    required_postgres_extensions(pg)
    env = {key: value for key, value in env.items() if not key.startswith("PG")}
    # A short /private/tmp socket path avoids Darwin's Unix socket path limit.
    with tempfile.TemporaryDirectory(prefix="memory-pg-probe-", dir="/private/tmp") as temporary:
        probe = Path(temporary)
        data = probe / "data"
        run([pg / "bin/initdb", "-D", data, "-U", "memory_runtime_probe_admin", "--auth-local=trust", "--auth-host=reject", "--no-locale"], probe, env)
        started = False
        try:
            run([pg / "bin/pg_ctl", "-D", data, "-l", probe / "postgres.log", "-o",
                 f"-p 5432 -c listen_addresses='' -c unix_socket_directories='{probe}' -c unix_socket_permissions=0700", "-w", "start"], probe, env)
            started = True
            psql = [pg / "bin/psql", "-X", "-w", "-h", probe, "-p", "5432", "-U", "memory_runtime_probe_admin", "-d", "postgres", "-v", "ON_ERROR_STOP=1"]
            run([*psql, "-c", "CREATE ROLE memory_runtime_probe LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION"], probe, env)
            run([*psql, "-c", "CREATE DATABASE memory_runtime_probe OWNER memory_runtime_probe"], probe, env)
            run([*psql, "-d", "memory_runtime_probe", "-c", f"CREATE EXTENSION vector VERSION '{vector_version}'"], probe, env)
            run([pg / "bin/psql", "-X", "-w", "-h", probe, "-p", "5432", "-U", "memory_runtime_probe", "-d", "memory_runtime_probe", "-v", "ON_ERROR_STOP=1", "-c",
                 "CREATE EXTENSION pgcrypto; CREATE EXTENSION pg_trgm; SELECT encode(digest('runtime probe', 'sha256'), 'hex'), similarity('runtime', 'runtime'), '[1,2,3]'::vector;"], probe, env)
        finally:
            if started:
                run([pg / "bin/pg_ctl", "-D", data, "-m", "immediate", "-w", "stop"], probe, env)


def flatten_links(root):
    # Upstream executable aliases become independent hash-bound regular files.
    for file in sorted(root.rglob("*")):
        if file.is_symlink():
            target = file.resolve(strict=True)
            if root.resolve() not in target.parents or not target.is_file():
                raise ValueError("installed source creates external/directory alias")
            data, mode = target.read_bytes(), target.stat().st_mode & 0o777
            file.unlink()
            file.write_bytes(data)
            file.chmod(mode)
        elif file.is_file() and file.stat().st_nlink != 1:
            # PostgreSQL's timezone installer uses hard links for equivalent
            # zones. The package contract hashes independent regular files.
            data, mode = file.read_bytes(), file.stat().st_mode & 0o777
            file.unlink()
            file.write_bytes(data)
            file.chmod(mode)
    for directory in sorted((p for p in root.rglob("*") if p.is_dir()), reverse=True):
        if not any(directory.iterdir()):
            directory.rmdir()


def audit_and_sign(root, prefix, env):
    magics = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}
    for file in sorted(root.rglob("*")):
        if not file.is_file():
            continue
        with file.open("rb") as stream:
            macho = stream.read(4) in magics
        if not macho:
            continue
        # A resumed producer may be repairing a previously read-only staging
        # tree. Only producer-owned output is signed; final modes follow below.
        file.chmod(file.stat().st_mode & 0o777 | 0o200)
        output = subprocess.check_output(["otool", "-L", str(file)], text=True, env=env)
        identity_output = subprocess.check_output(["otool", "-D", str(file)], text=True, env=env)
        identities = identity_output.splitlines()[1:]
        identity = identities[0].strip() if len(identities) == 1 else None
        changes = []
        for line in output.splitlines()[1:]:
            dep = line.strip().split(" (", 1)[0]
            if dep.startswith(("/usr/lib/", "/System/Library/")):
                continue
            if dep.startswith(prefix + "/"):
                target = root / dep[len(prefix) + 1:]
            elif dep.startswith("@loader_path/"):
                target = file.parent / dep[len("@loader_path/"):]
            elif dep.startswith("@executable_path/") and "/bin/" in str(file):
                target = file.parent / dep[len("@executable_path/"):]
            else:
                raise ValueError(f"unsupported native dependency in {file.name}: {dep}")
            if not target.is_file() or root.resolve() not in target.resolve().parents:
                raise ValueError(f"native dependency missing from runtime: {dep}")
            if dep.startswith(prefix + "/"):
                # Rewrite only a validated dependency inside this exact bundle.
                # Relative loader paths work both in staging and after install.
                relative = "@loader_path/" + os.path.relpath(target, file.parent)
                if dep == identity:
                    changes.extend(["-id", relative])
                else:
                    changes.extend(["-change", dep, relative])
        if changes:
            run(["install_name_tool", *changes, file], root, env)
        run(["codesign", "--force", "--sign", "-", file], root, env)
        run(["codesign", "--verify", "--strict", file], root, env)


def produce(args):
    if platform.system() != "Darwin":
        raise ValueError("native runtime production requires macOS")
    arch = {"arm64": "aarch64", "x86_64": "x86_64"}.get(platform.machine())
    if not arch:
        raise ValueError("unsupported native architecture")
    pinned = sources(Path(__file__).with_name("runtime-sources.json"))
    bundle_id = args.bundle_id
    if not bundle_id or not bundle.re.fullmatch(r"[a-z0-9][a-z0-9_\-]{0,79}", bundle_id):
        raise ValueError("invalid bundle ID")
    prefix = f"{bundle.BASE}/{bundle_id}"
    work = Path(args.work_dir).absolute()
    if work.is_symlink():
        raise ValueError("work directory cannot be a link")
    work.mkdir(mode=0o700, parents=False, exist_ok=args.resume)
    descriptor = {"sources": pinned, "arch": arch, "prefix": prefix,
                  "producer_revision": 2, "openssl_options": list(OPENSSL_OPTIONS)}
    journal = work / "build-inputs.json"
    if journal.exists():
        if json.loads(journal.read_text()) != descriptor:
            raise ValueError("resume inputs changed; choose a fresh work directory")
    else:
        if any(work.iterdir()):
            raise ValueError("new build requires empty work directory")
        journal.write_text(json.dumps(descriptor, indent=2) + "\n")
    env = dict(os.environ)
    env.update({"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C", "MACOSX_DEPLOYMENT_TARGET": "12.0", "PYTHONDONTWRITEBYTECODE": "1",
                "PKG_CONFIG": "/usr/bin/false"})
    for key in ("PYTHONPATH", "PYTHONHOME", "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "CPATH", "LIBRARY_PATH", "PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR", "CFLAGS", "CPPFLAGS", "LDFLAGS"):
        env.pop(key, None)
    src = work / "sources"
    src.mkdir(exist_ok=True)
    stage = work / "stage"
    stage.mkdir(exist_ok=True)
    root = stage / prefix.lstrip("/")
    manifest = work / "runtime-manifest.json"
    if manifest.exists():
        bundle.validate_bundle(root, manifest, arch)
        required_postgres_extensions(root / "postgres")
        print(json.dumps({"runtime_bundle": str(root), "runtime_manifest": str(manifest), "arch": arch}))
        return
    dependency = work / "openssl-static"
    for key in SOURCE_ORDER:
        item = pinned[key]
        archive = work / (key + ".archive")
        if not archive.exists():
            temporary = archive.with_suffix(".download")
            urllib.request.urlretrieve(item["url"], temporary)
            checked_archive(temporary, item)
            temporary.rename(archive)
        checked_archive(archive, item)
        source = src / item["directory"]
        if not source.exists():
            with tarfile.open(archive) as stream:
                stream.extractall(src)
        done = work / (key + ".complete")
        if done.exists():
            continue
        if key == "openssl":
            for command in openssl_commands(dependency, arch, args.jobs):
                run(command, source, env)
            if not all((dependency / f"lib/{name}.a").is_file() for name in ("libssl", "libcrypto")) or any((dependency / "lib").glob("*.dylib")):
                raise ValueError("OpenSSL must supply only pinned static libraries")
        elif key == "postgres":
            for command in postgres_commands(prefix, stage, dependency, args.jobs):
                run(command, source, env)
        elif key == "vector":
            pg = root / "postgres"
            for command in vector_commands(pg, prefix, stage, args.jobs):
                run(command, source, env)
            if not (pg / "lib/vector.dylib").is_file() or not (pg / "share/extension/vector.control").is_file():
                raise ValueError("pgvector installation did not produce native files at the exact staged prefix")
        else:
            # This is an operator runtime, not a network Python distribution.
            # Memory owns HTTPS. Do not autodiscover Homebrew OpenSSL or ship
            # extensions whose loader paths require a build-host installation.
            (source / "Modules/Setup.local").write_text("*disabled*\n_ssl\n_hashlib\n")
            run(["./configure", f"--prefix={prefix}/python", "--without-ensurepip", "--disable-test-modules"], source, env)
            run(["make", f"-j{args.jobs}"], source, env)
            run(["make", "install", f"DESTDIR={stage}"], source, env)
            python = root / "python/bin/python3.14"
            run([python, "-I", "-c", "import ctypes, sqlite3, hashlib, json, pathlib, subprocess, socket, plistlib"], source, env)
        done.write_text("completed\n")
    notices = root / "notices"
    notices.mkdir(exist_ok=True)
    for key in SOURCE_ORDER:
        source_notice = src / pinned[key]["directory"] / pinned[key]["notice"]
        notice = notices / ("LICENSE-" + key)
        if notice.exists():
            if notice.is_symlink() or notice.read_bytes() != source_notice.read_bytes():
                raise ValueError("resumed redistribution notice differs from pinned source")
        else:
            shutil.copyfile(source_notice, notice)
    flatten_links(root)
    audit_and_sign(root, prefix, env)
    smoke_postgres_extensions(root / "postgres", pinned["vector"]["version"], env)
    entries = {}
    for file in sorted(root.rglob("*")):
        if file.is_file():
            mode = "0555" if file.stat().st_mode & 0o111 else "0444"
            file.chmod(int(mode, 8))
            entries[file.relative_to(root).as_posix()] = {"sha256": hashlib.sha256(file.read_bytes()).hexdigest(), "mode": mode}
        elif file.is_dir():
            file.chmod(0o755)
    value = {"schema_version": 1, "bundle_id": bundle_id, "arch": arch, "install_prefix": prefix,
             "relocatable_for_prefix": True, "python": {"path": "python/bin/python3", "version": pinned["python"]["version"]},
             "postgres": {"bin_dir": "postgres/bin", "version": pinned["postgres"]["version"], "vector_library": "postgres/lib/vector.dylib", "vector_control": "postgres/share/extension/vector.control", "vector_version": pinned["vector"]["version"], "vector_sql": sorted(n for n in entries if n.startswith("postgres/share/extension/vector--") and n.endswith(".sql"))},
             "files": entries, "notices": sorted(n for n in entries if n.startswith("notices/"))}
    bundle.validate_manifest(value, arch)
    manifest.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    manifest.chmod(0o444)
    bundle.validate_bundle(root, manifest, arch)
    print(json.dumps({"runtime_bundle": str(root), "runtime_manifest": str(manifest), "arch": arch}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--bundle-id", required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.jobs <= 32:
        parser.error("jobs must be between 1 and 32")
    try:
        produce(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"runtime build failed: {error}\n")


if __name__ == "__main__":
    main()
