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
import urllib.request

import runtime_bundle as bundle


def sources(file):
    value = json.loads(Path(file).read_text())
    if set(value) != {"schema_version", "postgres", "vector", "python"} or value["schema_version"] != 1:
        raise ValueError("unsupported source manifest")
    for key in ("postgres", "vector", "python"):
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
        output = subprocess.check_output(["otool", "-L", str(file)], text=True, env=env)
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
    descriptor = {"sources": pinned, "arch": arch, "prefix": prefix}
    journal = work / "build-inputs.json"
    if journal.exists():
        if json.loads(journal.read_text()) != descriptor:
            raise ValueError("resume inputs changed; choose a fresh work directory")
    else:
        if any(work.iterdir()):
            raise ValueError("new build requires empty work directory")
        journal.write_text(json.dumps(descriptor, indent=2) + "\n")
    env = dict(os.environ)
    env.update({"LC_ALL": "C", "MACOSX_DEPLOYMENT_TARGET": "12.0", "PYTHONDONTWRITEBYTECODE": "1",
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
        print(json.dumps({"runtime_bundle": str(root), "runtime_manifest": str(manifest), "arch": arch}))
        return
    for key in ("postgres", "vector", "python"):
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
        if key == "postgres":
            # PostgreSQL defaults to no TLS; --without-ssl is not a valid way
            # to select that default (its configure requires an SSL provider).
            run(["./configure", f"--prefix={prefix}/postgres", "--without-readline", "--without-zlib", "--without-icu", "--without-lz4", "--without-zstd"], source, env)
            run(["make", f"-j{args.jobs}"], source, env)
            run(["make", "install", f"DESTDIR={stage}"], source, env)
        elif key == "vector":
            pg = root / "postgres"
            for command in vector_commands(pg, prefix, stage, args.jobs):
                run(command, source, env)
            if not (pg / "lib/vector.dylib").is_file() or not (pg / "share/extension/vector.control").is_file():
                raise ValueError("pgvector installation did not produce native files at the exact staged prefix")
        else:
            run(["./configure", f"--prefix={prefix}/python", "--without-ensurepip", "--disable-test-modules"], source, env)
            run(["make", f"-j{args.jobs}"], source, env)
            run(["make", "install", f"DESTDIR={stage}"], source, env)
            python = root / "python/bin/python3.14"
            run([python, "-I", "-c", "import ctypes, sqlite3, hashlib, json, pathlib, subprocess, socket, plistlib"], source, env)
        done.write_text("completed\n")
    notices = root / "notices"
    notices.mkdir(exist_ok=True)
    for key in ("postgres", "vector", "python"):
        shutil.copyfile(src / pinned[key]["directory"] / pinned[key]["notice"], notices / ("LICENSE-" + key))
    flatten_links(root)
    audit_and_sign(root, prefix, env)
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
