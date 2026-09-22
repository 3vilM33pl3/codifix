#!/usr/bin/env python3
"""Offline ABI parser/fail-closed tests; optional real cross-compiler fixture.

Run: python3 -m unittest discover -s packaging/ca-bookworm -p 'test_abi.py' -v
Set CA_ABI_TEST_SYSROOT=/usr/aarch64-linux-gnu in the reviewed builder to
also compile and statically inspect a synthetic ARM64 executable (never run it).
"""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import abi


HEADER = """ELF Header:
  Class: ELF64
  Data: 2's complement, little endian
  Type: DYN (Position-Independent Executable file)
  Machine: AArch64
"""


def program(artifact=False, architecture="arm64"):
    return "Program Headers:\n  LOAD 0x000 0x000\n" + (
        f"  INTERP 0x100 0x100\n [Requesting program interpreter: {abi.target(architecture).interpreter}]\n"
        if artifact else ""
    )


def dynamic(needed=(), soname=None, extra=()):
    rows = [f" 0x00000001 (NEEDED) Shared library: [{name}]" for name in needed]
    if soname:
        rows.append(f" 0x0000000e (SONAME) Library soname: [{soname}]")
    rows.extend(extra)
    rows.append(" 0x00000000 (NULL) 0x0")
    return f"Dynamic section at offset 0x100 contains {len(rows)} entries:\n" + "\n".join(rows)


def versions(required=None, defined=()):
    lines = []
    if defined:
        lines.append(f"Version definition section '.gnu.version_d' contains {len(defined)} entries:")
        lines.append(" Addr: 0x100 Offset: 0x100 Link: 1 (.dynstr)")
        for number, name in enumerate(defined):
            lines.append(f" 0x{number:04x}: Rev: 1 Flags: none Index: {number + 1} Cnt: 1 Name: {name}")
    if required:
        lines.append(f"Version needs section '.gnu.version_r' contains {len(required)} entries:")
        lines.append(" Addr: 0x100 Offset: 0x100 Link: 1 (.dynstr)")
        for name, required_versions in required.items():
            lines.append(f" 000000: Version: 1 File: {name} Cnt: {len(required_versions)}")
            for number, version in enumerate(required_versions):
                lines.append(f" 0x0010: Name: {version} Flags: none Version: {number + 2}")
    return "\n".join(lines) or "No version information found in this file."


def symbols(imports=(), exports=(), weak=()):
    rows = [" 0: 0000000000000000 0 NOTYPE LOCAL DEFAULT UND"]
    for name, binding, index in (
        *((name, "GLOBAL", "UND") for name in imports),
        *((name, "GLOBAL", "12") for name in exports),
        *((name, "WEAK", "UND") for name in weak),
    ):
        rows.append(f" {len(rows)}: 0000000000000000 0 FUNC {binding} DEFAULT {index} {name}")
    return f"Symbol table '.dynsym' contains {len(rows)} entries:\n" + "\n".join(rows)


def metadata(artifact=False, needed=(), soname=None, required=None, defined=(), imports=(), exports=(),
             architecture="arm64"):
    return {
        "--file-header": HEADER.replace("AArch64", abi.target(architecture).machine),
        "--program-headers": program(artifact, architecture),
        "--dynamic": dynamic(needed, soname),
        "--version-info": versions(required, defined),
        "--dyn-syms": symbols(imports, exports),
    }


class ParserTests(unittest.TestCase):
    def test_header_success(self):
        self.assertEqual(abi.parse_header(HEADER)["Machine"], "AArch64")

    def test_explicit_amd64_header_and_interpreter(self):
        header = HEADER.replace("AArch64", "Advanced Micro Devices X86-64")
        self.assertEqual(abi.parse_header(header, "amd64")["Machine"], "Advanced Micro Devices X86-64")
        self.assertEqual(abi.parse_program_headers(program(True, "amd64"), True, "amd64"),
                         "/lib64/ld-linux-x86-64.so.2")
        for text in (HEADER, header.replace("ELF64", "ELF32"), header.replace("little endian", "big endian")):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_header(text, "amd64")
        for text in (program(True), program(), program(True, "amd64").replace("/lib64/", "/tmp/")):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_program_headers(text, True, "amd64")

    def test_unknown_architecture_rejected_by_all_entrypoints(self):
        for architecture in ("AMD64", "x86_64", "riscv64", "", None):
            with self.subTest(architecture=architecture):
                for action in (
                    lambda: abi.parse_header(HEADER, architecture),
                    lambda: abi.parse_program_headers(program(True), True, architecture),
                    lambda: abi.inspect(Path(__file__), lambda *_: "", architecture=architecture),
                    lambda: abi.Auditor(Path("/"), lambda *_: "", architecture),
                ):
                    with self.assertRaisesRegex(abi.AuditError, "unsupported architecture"):
                        action()

    def test_arch_class_endianness_unknown_or_duplicate_fail(self):
        for text in (
            HEADER.replace("AArch64", "Advanced Micro Devices X86-64"),
            HEADER.replace("ELF64", "ELF32"),
            HEADER.replace("little endian", "big endian"),
            HEADER.replace("DYN", "REL"),
            HEADER.replace("Machine:", "Unknown:"),
            HEADER + " Class: ELF64\n",
        ):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_header(text)

    def test_interpreter_required(self):
        self.assertEqual(abi.parse_program_headers(program(True), True), abi.INTERPRETER)
        for text in (program(), program(True).replace("aarch64", "x86-64"), "", program(True) + " INTERP 0x100\n"):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_program_headers(text, True)

    def test_dynamic(self):
        self.assertEqual(abi.parse_dynamic(dynamic(["libc.so.6"], "example.so")), (["libc.so.6"], "example.so"))

    def test_paths_overrides_and_truncation_fail(self):
        for text in (
            dynamic(["/tmp/libc.so.6"]), dynamic(["../libc.so.6"]),
            dynamic(["libc.so.6", "libc.so.6"]),
            dynamic(extra=[" 0x001 (RUNPATH) Library runpath: [/tmp]"]),
            dynamic(extra=[" 0x001 (RPATH) Library rpath: [/tmp]"]),
            dynamic(extra=[" 0x001 (AUDIT) Library audit: [evil.so]"]),
            dynamic(extra=[" 0x001 (FILTER) Library filter: [evil.so]"]),
            dynamic(extra=[" 0x001 (0x1234) unknown"]),
            dynamic(extra=[" 0x001 (FLAGS_1) Flags: NODEFLIB"]),
            dynamic().replace("1 entries", "2 entries"), "",
        ):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_dynamic(text)

    def test_version_needs_separated_from_definitions(self):
        needs, definitions = abi.parse_versions(versions(
            {"libc.so.6": ["GLIBC_2.17", "GLIBC_2.36"], "libgcc_s.so.1": ["GCC_3.0"]},
            ["example.so", "EXAMPLE_1"],
        ))
        self.assertEqual(needs["libc.so.6"], {"GLIBC_2.17", "GLIBC_2.36"})
        self.assertEqual(definitions, {"example.so", "EXAMPLE_1"})

    def test_readelf_singular_entry_heading(self):
        needs, _ = abi.parse_versions(versions({"libc.so.6": ["GLIBC_2.36"]}).replace("1 entries:", "1 entry:"))
        self.assertEqual(needs, {"libc.so.6": {"GLIBC_2.36"}})

    def test_unknown_and_truncated_version_metadata_fail(self):
        good = versions({"libc.so.6": ["GLIBC_2.36"]})
        for text in (
            "", "unexpected", good.replace("Cnt: 1", "Cnt: 2"),
            good.replace("contains 1 entries", "contains 2 entries"),
            good.replace("Version: 1 File:", "Version: 9 File:"),
            versions(defined=["LIB_1"]).replace("Name:", "Unrecognized:"),
        ):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_versions(text)

    def test_symbols_default_versions_and_weak_imports(self):
        imports, exports = abi.parse_symbols(symbols(
            imports=["puts@GLIBC_2.17 (2)"],
            exports=["symbol@@VERSION_1", "legacy@VERSION_0"], weak=["optional"],
        ))
        self.assertEqual(imports, {("puts", "GLIBC_2.17")})
        self.assertIn(("symbol", None), exports)
        self.assertIn(("legacy", "VERSION_0"), exports)
        self.assertNotIn(("legacy", None), exports)

    def test_symbol_metadata_fails_closed(self):
        for text in ("", symbols().replace("1 entries", "2 entries"), symbols().replace("LOCAL", "UNKNOWN")):
            with self.subTest(text=text), self.assertRaises(abi.AuditError):
                abi.parse_symbols(text)


class ClosureTests(unittest.TestCase):
    architecture = "arm64"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        base = Path(self.temp.name)
        self.root = base / "sysroot"
        (self.root / "lib").mkdir(parents=True)
        self.artifact = base / "memory"
        self.artifact.write_bytes(b"trusted-test-artifact")
        self.data = {self.artifact: metadata(
            artifact=True, needed=["libc.so.6", "libstdc++.so.6"],
            required={"libc.so.6": ["GLIBC_2.34"], "libstdc++.so.6": ["GLIBCXX_3.4.30"]},
            imports=["start@GLIBC_2.34", "cpp@GLIBCXX_3.4.30"],
            architecture=self.architecture,
        )}
        self.loader_name = Path(abi.target(self.architecture).interpreter).name
        loader = self.lib(self.loader_name, defined=["GLIBC_PRIVATE"], exports=["loader@@GLIBC_PRIVATE"])
        interpreter = self.root / abi.target(self.architecture).interpreter.lstrip("/")
        if interpreter != loader:
            interpreter.parent.mkdir(parents=True)
            interpreter.symlink_to("../lib/" + self.loader_name)
        self.lib("libc.so.6", needed=[self.loader_name],
                 required={self.loader_name: ["GLIBC_PRIVATE"]},
                 defined=["GLIBC_2.34", "GLIBC_2.36"], imports=["loader@GLIBC_PRIVATE"],
                 exports=["start@@GLIBC_2.34"])
        self.lib("libstdc++.so.6", needed=["libgcc_s.so.1"],
                 required={"libgcc_s.so.1": ["GCC_3.0"]}, defined=["GLIBCXX_3.4.30"],
                 imports=["unwind@GCC_3.0"], exports=["cpp@@GLIBCXX_3.4.30"])
        self.lib("libgcc_s.so.1", defined=["GCC_3.0"], exports=["unwind@@GCC_3.0"])

    def lib(self, name, **kwargs):
        path = self.root / "lib" / name
        path.write_bytes(name.encode())
        self.data[path] = metadata(soname=name, architecture=self.architecture, **kwargs)
        return path

    def reader(self, option, path):
        return self.data[path][option]

    def audit(self):
        return abi.Auditor(self.root, self.reader, self.architecture).audit([self.artifact])

    def test_transitive_closure_hashes_and_limitations(self):
        result = self.audit()
        self.assertEqual(result["status"], "static_abi_pass")
        self.assertEqual(result["architecture"], self.architecture)
        self.assertEqual(result["target"], "Debian 12 " + self.architecture.upper() + " glibc 2.36")
        self.assertEqual(len(result["libraries"]), 4)
        self.assertEqual(result["artifacts"][0]["sha256"], abi.digest(self.artifact))
        self.assertEqual(result["artifacts"][0]["maximum_required_glibc"], "2.34")
        self.assertTrue(all(len(value["sha256"]) == 64 for value in result["libraries"]))
        self.assertIn("Static inspection only", result["limitations"][0])

    def test_missing_transitive_library_fails(self):
        (self.root / "lib/libgcc_s.so.1").unlink()
        with self.assertRaisesRegex(abi.AuditError, "missing or ambiguous"):
            self.audit()

    def test_transitive_missing_version_fails(self):
        self.data[self.root / "lib/libstdc++.so.6"]["--version-info"] = versions(
            {"libgcc_s.so.1": ["GCC_3.0", "GCC_4.0"]}, ["GLIBCXX_3.4.30"],
        )
        with self.assertRaisesRegex(abi.AuditError, "lacks versions"):
            self.audit()

    def test_declared_version_without_actual_symbol_fails(self):
        self.data[self.root / "lib/libstdc++.so.6"]["--dyn-syms"] = symbols(exports=["different@@GLIBCXX_3.4.30"])
        with self.assertRaisesRegex(abi.AuditError, "unavailable dynamic symbols"):
            self.audit()

    def test_transitive_wrong_architecture_fails(self):
        self.data[self.root / "lib/libgcc_s.so.1"]["--file-header"] = HEADER.replace("AArch64", "ARM")
        with self.assertRaisesRegex(abi.AuditError, abi.target(self.architecture).machine):
            self.audit()

    def test_new_glibc_and_private_application_requirements_fail(self):
        for version in ("GLIBC_2.37", "GLIBC_2.43", "GLIBC_PRIVATE", "GLIBC_FUTURE"):
            self.data[self.artifact]["--version-info"] = versions({"libc.so.6": [version]})
            self.data[self.artifact]["--dyn-syms"] = symbols(imports=[f"start@{version}"])
            with self.subTest(version=version), self.assertRaises(abi.AuditError):
                self.audit()

    def test_versions_must_belong_to_needed_libraries(self):
        self.data[self.artifact]["--version-info"] = versions({"libabsent.so": ["VERSION_1"]})
        with self.assertRaisesRegex(abi.AuditError, "absent from DT_NEEDED"):
            self.audit()

    def test_sysroot_must_be_the_236_baseline(self):
        for defined in (["GLIBC_2.34"], ["GLIBC_2.34", "GLIBC_2.36", "GLIBC_2.43"]):
            self.data[self.root / "lib/libc.so.6"]["--version-info"] = versions(defined=defined)
            self.data[self.root / "lib/libc.so.6"]["--dyn-syms"] = symbols(exports=["start@@GLIBC_2.34"])
            with self.subTest(defined=defined), self.assertRaisesRegex(abi.AuditError, "sysroot"):
                self.audit()

    def test_symbol_versions_cannot_bypass_requirement_metadata(self):
        self.data[self.artifact]["--version-info"] = versions()
        with self.assertRaisesRegex(abi.AuditError, "absent from version needs"):
            self.audit()

    def test_wrong_soname_fails(self):
        self.data[self.root / "lib/libgcc_s.so.1"]["--dynamic"] = dynamic(soname="libwrong.so")
        with self.assertRaisesRegex(abi.AuditError, "SONAME"):
            self.audit()

    def test_library_symlink_cannot_escape_sysroot(self):
        path = self.root / "lib/libgcc_s.so.1"
        path.unlink()
        path.symlink_to(self.artifact)
        with self.assertRaisesRegex(abi.AuditError, "escapes sysroot"):
            self.audit()

    def test_ambiguous_library_rejected(self):
        directory = self.root / "usr/lib"
        directory.mkdir(parents=True)
        (directory / "libgcc_s.so.1").write_bytes(b"other bytes")
        with self.assertRaisesRegex(abi.AuditError, "ambiguous"):
            self.audit()

    def test_object_cannot_be_satisfied_by_another_artifact(self):
        other = self.artifact.with_name("another")
        other.write_bytes(b"another")
        self.data[other] = metadata(artifact=True, exports=["missing"], architecture=self.architecture)
        self.data[self.artifact]["--dyn-syms"] = symbols(imports=["missing"])
        with self.assertRaisesRegex(abi.AuditError, "unavailable dynamic symbols"):
            abi.Auditor(self.root, self.reader, self.architecture).audit([self.artifact, other])

    def test_changed_artifact_fails(self):
        first = True
        def reader(option, path):
            nonlocal first
            if first:
                self.artifact.write_bytes(b"changed during inspection")
                first = False
            return self.reader(option, path)
        with self.assertRaisesRegex(abi.AuditError, "changed during inspection"):
            abi.Auditor(self.root, reader, self.architecture).audit([self.artifact])

    def test_bounds_and_root_rejected(self):
        with self.assertRaises(abi.AuditError):
            abi.Auditor(Path("/"), self.reader, self.architecture)
        with self.assertRaises(abi.AuditError):
            abi.Auditor(self.root, self.reader, self.architecture).audit([])
        with patch.object(abi, "MAX_OBJECTS", 1), self.assertRaisesRegex(abi.AuditError, "closure exceeds"):
            self.audit()


class Amd64ClosureTests(ClosureTests):
    """Repeat every transitive rejection on AMD64, not just its happy path."""
    architecture = "amd64"

    def test_amd64_requires_explicit_selection(self):
        with self.assertRaisesRegex(abi.AuditError, "expected AArch64"):
            abi.Auditor(self.root, self.reader).audit([self.artifact])

    def test_amd64_rejects_arm64_artifact_and_transitive_library(self):
        for path in (self.artifact, self.root / "lib/libgcc_s.so.1"):
            original = self.data[path]["--file-header"]
            self.data[path]["--file-header"] = HEADER
            try:
                with self.assertRaisesRegex(abi.AuditError, "expected Advanced Micro Devices X86-64"):
                    self.audit()
            finally:
                self.data[path]["--file-header"] = original

    def test_amd64_multiarch_sysroot_and_loader_symlink(self):
        directory = self.root / "lib/x86_64-linux-gnu"
        directory.mkdir()
        for path in list(self.data):
            if path.parent == self.root / "lib":
                new = directory / path.name
                path.rename(new)
                self.data[new] = self.data.pop(path)
        interpreter = self.root / "lib64" / self.loader_name
        interpreter.unlink()
        interpreter.symlink_to("../lib/x86_64-linux-gnu/" + self.loader_name)
        result = self.audit()
        self.assertEqual(result["status"], "static_abi_pass")
        self.assertTrue(all(Path(item["path"]).parent == directory for item in result["libraries"]))
        self.assertNotIn("ARM64", str(result))
        self.assertNotIn("AArch64", str(result))

    def test_amd64_does_not_search_arm64_multiarch_directory(self):
        old = self.root / "lib/libgcc_s.so.1"
        directory = self.root / "lib/aarch64-linux-gnu"
        directory.mkdir()
        new = directory / old.name
        old.rename(new)
        self.data[new] = self.data.pop(old)
        with self.assertRaisesRegex(abi.AuditError, "missing or ambiguous"):
            self.audit()

    def test_amd64_loader_must_be_at_exact_interpreter_path(self):
        (self.root / "lib64" / self.loader_name).unlink()
        with self.assertRaises(FileNotFoundError):
            self.audit()

    def test_amd64_wrong_or_overridden_interpreter_refused(self):
        for interpreter in (abi.INTERPRETER, "/lib/ld-linux-x86-64.so.2", "/tmp/ld-linux-x86-64.so.2"):
            self.data[self.artifact]["--program-headers"] = program(True, "amd64").replace(
                abi.target("amd64").interpreter, interpreter)
            with self.subTest(interpreter=interpreter), self.assertRaisesRegex(abi.AuditError, "approved AMD64 loader"):
                self.audit()

    def test_amd64_transitive_library_cannot_select_arm64_loader(self):
        self.data[self.root / "lib/libc.so.6"]["--program-headers"] = program(True)
        with self.assertRaisesRegex(abi.AuditError, "unexpected library interpreter"):
            self.audit()

    def test_amd64_sysroot_alias_to_host_root_refused(self):
        alias = Path(self.temp.name) / "host-root-alias"
        alias.symlink_to("/", target_is_directory=True)
        with self.assertRaisesRegex(abi.AuditError, "explicit isolated sysroot"):
            abi.Auditor(alias, self.reader, "amd64")


class SubprocessTests(unittest.TestCase):
    def test_cli_rejects_unknown_architecture(self):
        result = subprocess.run(
            [sys.executable, str(Path(abi.__file__)), "--architecture", "x86_64",
             "--sysroot", "/", str(Path(__file__))],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"invalid choice", result.stderr)
        self.assertEqual(result.stdout, b"")

    @unittest.skipUnless(shutil.which("readelf"), "readelf CLI prerequisite is unavailable")
    def test_cli_explicit_amd64_still_refuses_host_root(self):
        # Rejection precedes inspecting any artifact or library; only the
        # verifier is executed, with this Python source as an unused input.
        result = subprocess.run(
            [sys.executable, str(Path(abi.__file__)), "--architecture", "amd64",
             "--sysroot", "/", str(Path(__file__))],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(b"explicit isolated sysroot required", result.stdout)
        self.assertEqual(result.stderr, b"")

    def test_missing_readelf_and_wrong_format_fail(self):
        with self.assertRaises(OSError):
            abi.readelf("/nonexistent/readelf", "--file-header", Path(__file__))
        reader = shutil.which("readelf")
        if reader:
            with self.assertRaisesRegex(abi.AuditError, "readelf failed"):
                abi.readelf(reader, "--file-header", Path(__file__))

    def test_subprocess_output_bound(self):
        # Python supplies deterministic stdout; no artifact or generated tool runs.
        process = subprocess.Popen(
            [sys.executable, "-c", "print('x' * 65536)"],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        with patch.object(abi.subprocess, "Popen", return_value=process), patch.object(abi, "MAX_OUTPUT_BYTES", 64):
            with self.assertRaisesRegex(abi.AuditError, "output exceeds bound"):
                abi.readelf("fake-readelf", "--file-header", Path(__file__))
        self.assertIsNotNone(process.poll())

    def test_subprocess_timeout(self):
        process = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(5)"],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        with patch.object(abi.subprocess, "Popen", return_value=process), patch.object(abi, "READ_TIMEOUT", 0.01):
            with self.assertRaisesRegex(abi.AuditError, "timeout"):
                abi.readelf("fake-readelf", "--file-header", Path(__file__))
        self.assertIsNotNone(process.poll())


@unittest.skipUnless(os.environ.get("CA_ABI_TEST_SYSROOT"), "optional reviewed cross compiler fixture")
class RealFixtureTests(unittest.TestCase):
    def test_cross_compiled_fixture_without_execution(self):
        compiler = shutil.which("aarch64-linux-gnu-gcc")
        reader = shutil.which("readelf")
        self.assertIsNotNone(compiler)
        self.assertIsNotNone(reader)
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "synthetic"
            subprocess.run(
                [compiler, "-x", "c", "-", "-o", str(artifact)],
                input=b"#include <stdio.h>\nint main(void) { return puts(\"synthetic ABI fixture\") < 0; }\n",
                check=True, timeout=30, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            result = abi.Auditor(
                os.environ["CA_ABI_TEST_SYSROOT"],
                lambda option, path: abi.readelf(reader, option, path),
            ).audit([artifact])
            self.assertEqual(result["status"], "static_abi_pass")

    def test_cpp_transitive_fixture_without_execution(self):
        compiler = shutil.which("aarch64-linux-gnu-g++")
        reader = shutil.which("readelf")
        self.assertIsNotNone(compiler)
        self.assertIsNotNone(reader)
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "synthetic-cpp"
            subprocess.run(
                [compiler, "-x", "c++", "-", "-o", str(artifact)],
                input=b"#include <iostream>\n#include <string>\nint main(int argc, char**) { std::cout << std::string(argc, 'x'); }\n",
                check=True, timeout=30, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            result = abi.Auditor(
                os.environ["CA_ABI_TEST_SYSROOT"],
                lambda option, path: abi.readelf(reader, option, path),
            ).audit([artifact])
            self.assertEqual(result["status"], "static_abi_pass")
            self.assertIn("libstdc++.so.6", {value["soname"] for value in result["libraries"]})
            self.assertIn("libgcc_s.so.1", {value["soname"] for value in result["libraries"]})


if __name__ == "__main__":
    unittest.main()
