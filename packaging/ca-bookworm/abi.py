#!/usr/bin/env python3
"""Static Debian 12 ARM64 ABI gate for trusted, quiescent local build outputs.

Never invokes the artifacts, their loaders, or ldd. The explicit sysroot must be
from the reviewed Debian 12 cross toolchain; this tool does not establish its
distribution provenance. JSON is written to stdout; failure returns status 1.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import subprocess
import time
from dataclasses import dataclass


INTERPRETER = "/lib/ld-linux-aarch64.so.1"
MAX_GLIBC = (2, 36)
MAX_FILE_BYTES = 2 * 1024**3
MAX_OUTPUT_BYTES = 32 * 1024**2
MAX_OBJECTS = 128
READ_TIMEOUT = 30
NAME = r"[A-Za-z0-9_+.$-]+"


class AuditError(ValueError):
    """Insufficient or incompatible ABI evidence."""


def require(condition, message):
    if not condition:
        raise AuditError(message)


def digest(path):
    require(path.is_file(), f"not a regular file: {path}")
    require(path.stat().st_size <= MAX_FILE_BYTES, f"file exceeds audit bound: {path}")
    value = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            value.update(block)
    return value.hexdigest()


def readelf(executable, option, path):
    """Bound time and combined output; a nonempty diagnostic fails closed."""
    command = [executable, "--wide", option, "--", str(path)]
    environment = {"PATH": "/usr/bin:/bin", "LC_ALL": "C"}
    with subprocess.Popen(
        command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, env=environment,
    ) as process:
        buffers = {process.stdout: bytearray(), process.stderr: bytearray()}
        with selectors.DefaultSelector() as selector:
            for stream in buffers:
                selector.register(stream, selectors.EVENT_READ)
            deadline = time.monotonic() + READ_TIMEOUT
            total = 0
            try:
                while selector.get_map():
                    remaining = deadline - time.monotonic()
                    require(remaining > 0, f"readelf timeout: {path}")
                    for key, _ in selector.select(min(remaining, 0.25)):
                        block = os.read(key.fileobj.fileno(), 65536)
                        if not block:
                            selector.unregister(key.fileobj)
                            continue
                        total += len(block)
                        require(total <= MAX_OUTPUT_BYTES, f"readelf output exceeds bound: {path}")
                        buffers[key.fileobj].extend(block)
                process.wait(timeout=max(0.01, deadline - time.monotonic()))
                require(process.returncode == 0, f"readelf failed: {path}")
                require(not buffers[process.stderr], f"readelf diagnostic: {path}")
                return buffers[process.stdout].decode("utf-8", errors="strict")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()


def parse_header(text):
    fields = {}
    for line in text.splitlines():
        if ":" in line:
            key, value = line.strip().split(":", 1)
            # GNU readelf repeats Version (identification and ELF version).
            if key in {"Class", "Data", "Machine", "Type"}:
                require(key not in fields, f"duplicate ELF field: {key}")
                fields[key] = value.strip()
    require(fields.get("Class") == "ELF64", "expected ELF64")
    require(fields.get("Data") == "2's complement, little endian", "expected little endian")
    require(fields.get("Machine") == "AArch64", "expected AArch64")
    require(fields.get("Type", "").split(" ")[0] in {"EXEC", "DYN"}, "expected executable/shared ELF")
    return fields


def parse_program_headers(text, artifact):
    require("Program Headers:" in text, "missing program headers")
    interpreters = re.findall(r"\[Requesting program interpreter: ([^\]\n]+)\]", text)
    interp_segments = len(re.findall(r"^\s*INTERP\s", text, re.MULTILINE))
    require(len(interpreters) == interp_segments, "unparsed interpreter metadata")
    require(len(interpreters) <= 1, "multiple interpreters")
    if artifact:
        require(interpreters == [INTERPRETER], "artifact requires the approved ARM64 loader")
    elif interpreters:
        require(interpreters == [INTERPRETER], "unexpected library interpreter")
    return interpreters[0] if interpreters else None


def parse_dynamic(text):
    heading = re.search(r"Dynamic section at .+ contains (\d+) entr(?:y|ies):", text)
    require(heading is not None, "missing dynamic metadata (static binaries are not supported)")
    rows = [line for line in text.splitlines() if re.match(r"\s*0x", line)]
    require(len(rows) == int(heading[1]), "truncated dynamic metadata")
    needed, sonames = [], []
    for row in rows:
        match = re.match(r"\s*0x[0-9a-fA-F]+\s+\(([^)]+)\)\s*(.*)$", row)
        require(match is not None, "unparsed dynamic entry")
        tag, value = match.groups()
        require(tag not in {"RPATH", "RUNPATH", "AUDIT", "DEPAUDIT", "FILTER", "AUXILIARY"},
                f"unsupported dynamic loader override: {tag}")
        require(not tag.startswith("0x") and "unknown" not in tag.lower(), "unknown dynamic tag")
        require(not (tag == "FLAGS_1" and "NODEFLIB" in value), "default library search disabled")
        if tag in {"NEEDED", "SONAME"}:
            name = re.fullmatch(r"(?:Shared library|Library soname): \[(" + NAME + r")\]", value)
            require(name is not None, f"invalid {tag} name")
            (needed if tag == "NEEDED" else sonames).append(name[1])
    require(len(set(needed)) == len(needed), "duplicate DT_NEEDED")
    require(len(sonames) <= 1, "multiple SONAME entries")
    return needed, sonames[0] if sonames else None


def parse_versions(text):
    """Parse/count verneed groups and verdef entries, not decorative symbol rows."""
    if text.strip() == "No version information found in this file.":
        return {}, set()
    require(text.strip(), "missing version metadata")
    needs, definitions = {}, set()
    mode, expected, seen, pending, pending_count = None, 0, 0, None, 0

    def finish_section():
        if mode in {"needs", "definition"}:
            require(seen == expected, "truncated version metadata")
        if pending is not None:
            require(len(needs[pending]) == pending_count, "truncated required versions")

    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        section = re.fullmatch(r"Version (symbols|needs|definition) section .+ contains (\d+) entr(?:y|ies):", line)
        if section:
            finish_section()
            mode, expected, seen = section[1], int(section[2]), 0
            pending, pending_count = None, 0
            continue
        if line.startswith("Addr:"):
            require(mode is not None, "version address without section")
            continue
        require(mode is not None, "unknown version metadata")
        if mode == "symbols":
            require(re.match(r"[0-9a-f]+:", line) is not None, "unparsed version symbol row")
            continue
        if mode == "needs":
            group = re.fullmatch(r"(?:0x)?[0-9a-f]+: Version: 1\s+File: (" + NAME + r")\s+Cnt: (\d+)", line)
            if group:
                if pending is not None:
                    require(len(needs[pending]) == pending_count, "truncated required versions")
                pending, pending_count = group[1], int(group[2])
                require(pending not in needs, "duplicate required library")
                needs[pending] = set()
                seen += 1
                continue
            entry = re.fullmatch(r"(?:0x)?[0-9a-f]+:\s+Name: (" + NAME + r")\s+Flags: (?:none|WEAK)\s+Version: \d+", line)
            require(entry is not None and pending is not None, "unparsed required version")
            require(entry[1] not in needs[pending], "duplicate required version")
            needs[pending].add(entry[1])
        else:
            entry = re.fullmatch(r"(?:0x)?[0-9a-f]+: Rev: 1\s+Flags: (?:none|BASE|WEAK)\s+Index: \d+\s+Cnt: \d+\s+Name: (" + NAME + r")", line)
            if entry:
                require(entry[1] not in definitions, "duplicate defined version")
                definitions.add(entry[1])
                seen += 1
            else:
                require(re.fullmatch(r"(?:0x)?[0-9a-f]+: Parent \d+: " + NAME, line) is not None,
                        "unparsed defined version")
    finish_section()
    require(mode is not None, "no version sections")
    return needs, definitions


def parse_symbols(text):
    heading = re.search(r"Symbol table '\.dynsym' contains (\d+) entr(?:y|ies):", text)
    require(heading is not None, "missing dynamic symbols")
    rows = [line for line in text.splitlines() if re.match(r"\s*\d+:", line)]
    require(len(rows) == int(heading[1]), "truncated dynamic symbols")
    imports, exports = set(), set()
    for row in rows:
        match = re.fullmatch(r"\s*\d+:\s+[0-9a-f]+\s+\S+\s+\S+\s+(LOCAL|GLOBAL|WEAK|UNIQUE)\s+(DEFAULT|PROTECTED|HIDDEN|INTERNAL)\s+(\S+)\s*(.*?)\s*", row)
        require(match is not None, "unparsed dynamic symbol")
        binding, visibility, index, name = match.groups()
        if not name or binding == "LOCAL":
            continue
        name = re.sub(r" \(\d+\)$", "", name)
        require(not re.search(r"\s", name), "invalid dynamic symbol name")
        parts = re.split(r"(@@?)", name, maxsplit=1)
        base, version = parts[0], parts[2] if len(parts) == 3 else None
        if index == "UND":
            if binding != "WEAK":
                imports.add((base, version))
        elif visibility in {"DEFAULT", "PROTECTED"}:
            exports.add((base, version))
            if len(parts) == 1 or parts[1] == "@@":
                exports.add((base, None))
    return imports, exports


@dataclass
class Object:
    path: Path
    sha256: str
    header: dict
    interpreter: str | None
    needed: list
    soname: str | None
    required_versions: dict
    defined_versions: set
    imports: set
    exports: set


def inspect(path, reader, artifact=False):
    before = digest(path)
    header = parse_header(reader("--file-header", path))
    interpreter = parse_program_headers(reader("--program-headers", path), artifact)
    needed, soname = parse_dynamic(reader("--dynamic", path))
    versions, definitions = parse_versions(reader("--version-info", path))
    imports, exports = parse_symbols(reader("--dyn-syms", path))
    require(set(versions).issubset(needed), "required version owner absent from DT_NEEDED")
    required_version_names = set().union(*versions.values()) if versions else set()
    require(all(version is None or version in required_version_names for _, version in imports),
            "imported symbol version absent from version needs")
    require(all(version is None or version in definitions for _, version in exports),
            "exported symbol version absent from version definitions")
    numeric_glibc = []
    for value in required_version_names:
        if value.startswith("GLIBC_"):
            if value == "GLIBC_PRIVATE":
                require(not artifact, "artifact uses private glibc ABI")
            else:
                match = re.fullmatch(r"GLIBC_(\d+(?:\.\d+)+)", value)
                require(match is not None, f"unknown glibc requirement: {value}")
                numeric_glibc.append(tuple(map(int, match[1].split("."))))
    require(not numeric_glibc or max(numeric_glibc) <= MAX_GLIBC, "requires glibc newer than 2.36")
    require(before == digest(path), f"file changed during inspection: {path}")
    return Object(path, before, header, interpreter, needed, soname, versions, definitions, imports, exports)


class Auditor:
    def __init__(self, sysroot, reader):
        self.root = Path(sysroot).resolve(strict=True)
        require(self.root.is_dir() and self.root != Path("/"), "explicit cross sysroot required")
        self.reader = reader
        self.dirs = [self.root / entry for entry in (
            "lib", "lib/aarch64-linux-gnu", "usr/lib", "usr/lib/aarch64-linux-gnu",
        )]
        self.objects = {}

    def resolve(self, name):
        require(re.fullmatch(NAME, name) is not None and name not in {".", ".."}, "invalid library name")
        matches = set()
        for directory in self.dirs:
            candidate = directory / name
            if candidate.exists() or candidate.is_symlink():
                target = candidate.resolve(strict=True)
                require(target.is_relative_to(self.root), f"library escapes sysroot: {name}")
                require(target.is_file(), f"library is not a regular file: {name}")
                matches.add(target)
        require(len(matches) == 1, f"missing or ambiguous sysroot library: {name}")
        return matches.pop()

    def library(self, name):
        path = self.resolve(name)
        if path not in self.objects:
            require(len(self.objects) < MAX_OBJECTS, "dependency closure exceeds bound")
            value = inspect(path, self.reader)
            require(value.soname == name, f"unexpected SONAME for {name}")
            # Insert before descent: libc and the loader may be cyclic.
            self.objects[path] = value
            for dependency in value.needed:
                self.library(dependency)
        require(self.objects[path].soname == name, f"unexpected SONAME for {name}")
        return self.objects[path]

    def audit(self, artifacts):
        require(1 <= len(artifacts) <= 16, "expected one to sixteen artifacts")
        roots = [inspect(Path(path).resolve(strict=True), self.reader, True) for path in artifacts]
        loader = self.library(Path(INTERPRETER).name)
        exact_loader = (self.root / INTERPRETER.lstrip("/")).resolve(strict=True)
        require(exact_loader == loader.path, "interpreter is not at the expected sysroot path")
        for root in roots:
            for dependency in root.needed:
                self.library(dependency)
        libc = self.library("libc.so.6")
        require("GLIBC_2.36" in libc.defined_versions, "sysroot lacks the Debian 12 glibc baseline")
        for version in libc.defined_versions:
            if re.fullmatch(r"GLIBC_\d+(?:\.\d+)+", version):
                require(tuple(map(int, version[6:].split("."))) <= MAX_GLIBC, "sysroot glibc is newer than 2.36")
        all_objects = roots + list(self.objects.values())
        for value in all_objects:
            for owner, requirements in value.required_versions.items():
                missing = requirements - self.library(owner).defined_versions
                require(not missing, f"{value.path.name}: {owner} lacks versions {sorted(missing)}")
        # Check each artifact separately: one executable cannot supply another's imports.
        for root in roots:
            reachable = {loader.path: loader}
            pending = list(root.needed)
            while pending:
                value = self.library(pending.pop())
                if value.path not in reachable:
                    reachable[value.path] = value
                    pending.extend(value.needed)
            exports = set().union(root.exports, *(value.exports for value in reachable.values()))
            for value in [root, *reachable.values()]:
                missing = value.imports - exports
                require(not missing, f"{value.path.name}: unavailable dynamic symbols {sorted(missing, key=str)[:8]}")
        # Final pass binds the report to stable build outputs and library bytes.
        for value in all_objects:
            require(digest(value.path) == value.sha256, f"file changed during audit: {value.path}")
        def record(value):
            glibc = [
                version[6:] for entries in value.required_versions.values() for version in entries
                if re.fullmatch(r"GLIBC_\d+(?:\.\d+)+", version)
            ]
            return {
                "path": str(value.path), "sha256": value.sha256,
                "elf": value.header, "interpreter": value.interpreter,
                "needed": value.needed, "soname": value.soname,
                "required_versions": {key: sorted(item) for key, item in sorted(value.required_versions.items())},
                "defined_versions": sorted(value.defined_versions),
                "required_strong_dynamic_symbols": len(value.imports),
                "maximum_required_glibc": max(glibc, key=lambda item: tuple(map(int, item.split(".")))) if glibc else None,
            }
        return {
            "schema_version": 1, "status": "static_abi_pass", "target": "Debian 12 ARM64 glibc 2.36",
            "sysroot": str(self.root), "artifacts": [record(value) for value in roots],
            "libraries": [record(value) for value in sorted(self.objects.values(), key=lambda item: str(item.path))],
            "limitations": [
                "Static inspection only; no artifact, loader or library was executed.",
                "Requires trusted, quiescent artifacts and an independently provenance-verified Debian 12 cross sysroot.",
                "No proof of native execution, CPU instructions, kernel features, sandbox enforcement or application behavior.",
                "Does not inventory dlopen plugins, NSS modules, external executables or files accessed at runtime.",
                "Sysroot library hashes do not establish the libraries currently installed on the CA.",
                "Passing does not authorize installation, service activation, migration or paid calls.",
            ],
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sysroot", required=True, type=Path)
    parser.add_argument("--readelf", default="readelf", help="trusted host readelf executable")
    parser.add_argument("artifacts", nargs="+", type=Path)
    args = parser.parse_args()
    try:
        executable = shutil.which(args.readelf)
        require(executable is not None, "readelf unavailable")
        audit = Auditor(args.sysroot, lambda option, path: readelf(executable, option, path))
        result = audit.audit(args.artifacts)
        result["readelf"] = {"path": executable, "sha256": digest(Path(executable))}
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    except (AuditError, OSError, UnicodeError, subprocess.SubprocessError) as error:
        print(json.dumps({"schema_version": 1, "status": "static_abi_failed", "error": str(error)}, sort_keys=True))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
