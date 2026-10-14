#!/usr/bin/env python3
"""Synthetic, offline runtime staging tests. No supplied executable runs."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
import runtime_bundle as bundle
from onboarding_files import Refused


class RuntimeBundle(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="memory-runtime-fixture-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "runtime"
        self.source.mkdir(mode=0o755)
        self.payload = self.root / "payload"
        self.payload.mkdir(mode=0o755)
        self.manifest = self.root / "manifest.json"
        paths = ["python/bin/python3", *[f"postgres/bin/{n}" for n in sorted(bundle.TOOLS)],
                 "postgres/lib/vector.so", "postgres/share/extension/vector.control",
                 "postgres/share/extension/vector--0.8.1.sql", "notices/LICENSE-python",
                 "notices/LICENSE-postgresql", "notices/LICENSE-pgvector", "python/lib/libpython.dylib",
                 "python/lib/python3.14/__future__.py", "python/lib/python3.14/_collections_abc.py",
                 "python/lib/python3.14/lib-dynload/_ctypes.cpython-314-darwin.so",
                 "python/lib/python3.14/encodings/__init__.py"]
        entries = {}
        for name in paths:
            file = self.source / name
            file.parent.mkdir(parents=True, exist_ok=True)
            # Intentionally not real executables. Never execute supplied content.
            content = f"offline synthetic runtime file: {name}\n".encode()
            file.write_bytes(content)
            mode = "0555" if "/bin/" in name else "0444"
            file.chmod(int(mode, 8))
            entries[name] = {"sha256": hashlib.sha256(content).hexdigest(), "mode": mode}
        for directory in [self.source, *[p for p in self.source.rglob("*") if p.is_dir()]]:
            directory.chmod(0o755)
        self.value = {"schema_version": 1, "bundle_id": "fixture-py314-pg18-x86_64",
                      "arch": "x86_64", "install_prefix": bundle.BASE + "/fixture-py314-pg18-x86_64",
                      "relocatable_for_prefix": True, "python": {"path": "python/bin/python3", "version": "3.14.0"},
                      "postgres": {"bin_dir": "postgres/bin", "version": "18.0", "vector_library": "postgres/lib/vector.so",
                                   "vector_control": "postgres/share/extension/vector.control", "vector_version": "0.8.1",
                                   "vector_sql": ["postgres/share/extension/vector--0.8.1.sql"]},
                      "files": entries, "notices": [n for n in paths if n.startswith("notices/")]}
        self.save()

    def save(self):
        self.manifest.write_text(json.dumps(self.value))
        self.manifest.chmod(0o644)

    def validate(self):
        return bundle.validate_bundle(self.source, self.manifest, "x86_64")

    def test_native_dylib_extension_is_hash_bound(self):
        old, new = "postgres/lib/vector.so", "postgres/lib/vector.dylib"
        (self.source / old).rename(self.source / new)
        self.value["files"][new] = self.value["files"].pop(old)
        self.value["postgres"]["vector_library"] = new
        self.save()
        self.validate()
        self.value["postgres"]["vector_library"] = "postgres/lib/other.dylib"
        self.save()
        with self.assertRaisesRegex(Refused, "layout unsupported"):
            self.validate()

    def test_stage_exact_immutable_tree_and_prerequisite_descriptor(self):
        result = bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)
        expected = self.payload / self.value["install_prefix"].lstrip("/")
        self.assertEqual(result, expected / bundle.DESCRIPTOR)
        for name, entry in self.value["files"].items():
            self.assertEqual((expected / name).read_bytes(), (self.source / name).read_bytes())
            self.assertEqual(stat.S_IMODE((expected / name).stat().st_mode), int(entry["mode"], 8))
        self.assertEqual(stat.S_IMODE(expected.stat().st_mode), 0o555)
        self.assertEqual(stat.S_IMODE(result.stat().st_mode), 0o444)
        descriptor = json.loads(result.read_text())
        self.assertEqual(descriptor["manifest_sha256"], hashlib.sha256(self.manifest.read_bytes()).hexdigest())
        self.assertEqual(descriptor["postgres"]["major"], 18)
        self.assertEqual(set(descriptor["postgres"]["tool_sha256"]), bundle.TOOLS)
        self.assertEqual(descriptor["python"]["sha256"], self.value["files"]["python/bin/python3"]["sha256"])
        self.assertEqual(len(descriptor["files"]), len(self.value["files"]))
        self.assertEqual(descriptor["acceptance_pending"], ["native_macos_architecture", "loader_dependencies", "exact_prefix_relocatability"])
        self.assertEqual((expected / bundle.MANIFEST).read_bytes(), self.manifest.read_bytes())
        with self.assertRaises((Refused, FileExistsError)):
            bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)

    def test_isolated_cli_import_and_validate(self):
        result = subprocess.run([sys.executable, "-I", "-S", "-B", str(MACOS / "runtime_bundle.py"), "validate",
                                 "--bundle", str(self.source), "--manifest", str(self.manifest), "--arch", "x86_64"],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("acceptance pending", result.stdout)

    def test_manifest_change_between_preflight_and_stage_is_refused(self):
        reviewed = hashlib.sha256(self.manifest.read_bytes()).hexdigest()
        self.manifest.write_bytes(self.manifest.read_bytes() + b"\n")
        with self.assertRaisesRegex(Refused, "changed since package preflight"):
            bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload, manifest_sha256=reviewed)
        self.assertEqual(list(self.payload.iterdir()), [])

    def test_strict_schema_versions_arch_and_relocation(self):
        changes = [{"schema_version": True}, {"schema_version": 2}, {"unknown": 1}, {"arch": "aarch64"},
                   {"bundle_id": "Fixture"}, {"install_prefix": "/usr/local/elsewhere"}, {"relocatable_for_prefix": False},
                   {"python": {"path": "python/bin/python3", "version": "3.15.0"}},
                   {"postgres": dict(self.value["postgres"], version="15.9")}]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(Refused):
                bundle.validate_manifest(dict(self.value, **change), "x86_64")

    def test_duplicate_json_key_and_oversize_manifest(self):
        self.manifest.write_text('{"schema_version":1,"schema_version":1}')
        with self.assertRaisesRegex(Refused, "duplicate"):
            self.validate()
        self.manifest.write_bytes(b" " * (8 * 1024 * 1024 + 1))
        with self.assertRaisesRegex(Refused, "bounds"):
            self.validate()

    def test_traversal_case_unicode_and_reserved_aliases(self):
        for name in ["../escape", "/absolute", "python//oops", "python/thing.", "python/naïve", "PYTHON/bin/python3", bundle.DESCRIPTOR, "PYTHON",
                     "python/.hidden", "python/./file", "python/../file", "python/lib/python3.14/__FUTURE__.py"]:
            value = copy.deepcopy(self.value)
            value["files"][name] = {"sha256": "a" * 64, "mode": "0444"}
            with self.subTest(name=name), self.assertRaises(Refused):
                bundle.validate_manifest(value, "x86_64")

    def test_safe_leading_underscore_stdlib_and_extension_names_are_staged(self):
        self.validate()
        staged = bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload).parent
        for name in ["python/lib/python3.14/__future__.py", "python/lib/python3.14/_collections_abc.py",
                     "python/lib/python3.14/lib-dynload/_ctypes.cpython-314-darwin.so",
                     "python/lib/python3.14/encodings/__init__.py"]:
            with self.subTest(name=name):
                self.assertEqual(bundle.relative(name), name)
                self.assertEqual((staged / name).read_bytes(), (self.source / name).read_bytes())

    def test_missing_executable_notice_and_vector_sql(self):
        for name in ["python/bin/python3", "postgres/bin/psql", self.value["notices"][0], self.value["postgres"]["vector_sql"][0]]:
            value = copy.deepcopy(self.value)
            del value["files"][name]
            with self.subTest(name=name), self.assertRaises(Refused):
                bundle.validate_manifest(value, "x86_64")
        value = copy.deepcopy(self.value)
        value["notices"] = []
        with self.assertRaises(Refused):
            bundle.validate_manifest(value, "x86_64")

    def test_hash_permissions_and_extra_file_fail_without_staging(self):
        file = self.source / "python/bin/python3"
        def change_content():
            file.chmod(0o644)
            file.write_bytes(b"changed")
            file.chmod(0o555)
        for action in [lambda: file.chmod(0o755), change_content, lambda: (self.source / "extra").write_text("extra")]:
            with self.subTest(action=action):
                action()
                with self.assertRaises(Refused):
                    bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)
                self.assertEqual(list(self.payload.iterdir()), [])
                file.chmod(0o644)
                file.write_bytes(b"offline synthetic runtime file: python/bin/python3\n")
                file.chmod(0o555)
        (self.source / "extra").unlink()
        (self.source / "empty-extra").mkdir()
        with self.assertRaisesRegex(Refused, "unmanifested directory"):
            self.validate()

    def test_symlink_hardlink_special_and_untrusted_directory(self):
        file = self.source / "python/lib/libpython.dylib"
        original = file.read_bytes()
        file.unlink()
        file.symlink_to(self.source / "postgres/lib/vector.so")
        with self.assertRaisesRegex(Refused, "symlinks"):
            self.validate()
        file.unlink()
        os.link(self.source / "postgres/lib/vector.so", file)
        with self.assertRaisesRegex(Refused, "aliases"):
            self.validate()
        file.unlink()
        os.mkfifo(file)
        with self.assertRaisesRegex(Refused, "special"):
            self.validate()
        file.unlink()
        file.write_bytes(original)
        file.chmod(0o444)
        file.parent.chmod(0o777)
        with self.assertRaisesRegex(Refused, "directory"):
            self.validate()

    def test_symlinked_source_and_manifest_and_stage_ancestors(self):
        alias = self.root / "alias"
        alias.symlink_to(self.source, target_is_directory=True)
        with self.assertRaises(Refused):
            bundle.validate_bundle(alias, self.manifest, "x86_64")
        meta_alias = self.root / "meta-alias"
        meta_alias.symlink_to(self.manifest)
        with self.assertRaises(Refused):
            bundle.validate_bundle(self.source, meta_alias, "x86_64")
        (self.payload / "usr").symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)
        self.assertFalse((self.root / "local").exists())

    def test_reject_current_host_or_nonprivate_stage(self):
        with self.assertRaises(Refused):
            bundle.stage_bundle(self.source, self.manifest, "x86_64", Path("/"))
        self.root.chmod(0o755)
        with self.assertRaises(Refused):
            bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)

    def installed_fixture(self):
        # Patch only the constant in the test process; production has no prefix
        # bypass. Never write /usr/local or execute bundled fixture content.
        base = self.root / "installed-runtime"
        base.mkdir(mode=0o755)
        self.patch = mock.patch.object(bundle, "BASE", str(base))
        self.patch.start()
        self.addCleanup(self.patch.stop)
        self.value["install_prefix"] = str(base / self.value["bundle_id"])
        self.save()
        staged = bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)
        installed = base / self.value["bundle_id"]
        shutil.copytree(staged.parent, installed)
        return installed / bundle.DESCRIPTOR

    def test_installed_verifier_binds_manifest_descriptor_and_all_files(self):
        descriptor = self.installed_fixture()
        self.assertEqual(bundle.verify_installed(descriptor, "x86_64"), json.loads(descriptor.read_text()))
        with self.assertRaisesRegex(Refused, "architecture"):
            bundle.verify_installed(descriptor, "aarch64")
        extra = descriptor.parent / "extra"
        descriptor.parent.chmod(0o755)
        extra.write_text("unmanifested")
        descriptor.parent.chmod(0o555)
        with self.assertRaisesRegex(Refused, "unmanifested"):
            bundle.verify_installed(descriptor, "x86_64")
        descriptor.parent.chmod(0o755)
        extra.unlink()
        descriptor.parent.chmod(0o555)
        file = descriptor.parent / "python/lib/libpython.dylib"
        file.chmod(0o644)
        file.write_text("changed dependency")
        file.chmod(0o444)
        with self.assertRaisesRegex(Refused, "SHA-256"):
            bundle.verify_installed(descriptor, "x86_64")

    def test_installed_verifier_rejects_edited_metadata_and_mutable_directories(self):
        descriptor = self.installed_fixture()
        original = descriptor.read_bytes()
        descriptor.chmod(0o644)
        selected = json.loads(original)
        selected["files"] = {}
        descriptor.write_text(json.dumps(selected))
        descriptor.chmod(0o444)
        with self.assertRaisesRegex(Refused, "descriptor differs"):
            bundle.verify_installed(descriptor, "x86_64")
        descriptor.chmod(0o644)
        descriptor.write_bytes(original)
        descriptor.chmod(0o444)
        (descriptor.parent / "python").chmod(0o755)
        with self.assertRaisesRegex(Refused, "not immutable"):
            bundle.verify_installed(descriptor, "x86_64")
        (descriptor.parent / "python").chmod(0o555)
        manifest = descriptor.parent / bundle.MANIFEST
        manifest.chmod(0o644)
        manifest.write_bytes(manifest.read_bytes() + b"\n")
        manifest.chmod(0o444)
        with self.assertRaisesRegex(Refused, "descriptor differs"):
            bundle.verify_installed(descriptor, "x86_64")

    def test_preinstall_guard_is_create_only_read_only_and_volume_bound(self):
        scripts = self.root / "scripts"
        scripts.mkdir(mode=0o755)
        bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload, scripts=scripts)
        guard = scripts / "preinstall"
        checked = subprocess.run(["/bin/bash", "-n", str(guard)], capture_output=True, timeout=5)
        self.assertEqual(checked.returncode, 0)
        text = guard.read_text()
        self.assertIn("${3:-} != /", text)
        self.assertIn("-e $runtime_prefix || -L $runtime_prefix", text)
        self.assertIn("-L $ancestor", text)
        for mutation in ["rm ", "chmod", "chown", "launchctl", "exec "]:
            self.assertNotIn(mutation, text)

    def test_runtime_launcher_uses_fixed_python_isolation_and_descriptor(self):
        scripts = self.root / "scripts"
        scripts.mkdir(mode=0o755)
        bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload, scripts=scripts)
        launcher = self.payload / "usr/local/bin/memory-onboard"
        self.assertEqual(stat.S_IMODE(launcher.stat().st_mode), 0o755)
        text = launcher.read_text()
        self.assertIn(f"runtime_python='{self.value['install_prefix']}/python/bin/python3'", text)
        self.assertIn(f"runtime_descriptor='{self.value['install_prefix']}/{bundle.DESCRIPTOR}'", text)
        self.assertIn("operator='/usr/local/share/memory-layer/onboarding/operator/onboard.py'", text)
        self.assertIn('if [[ ${1:-} == first-run ]]; then\n  set -- "$@" --runtime "$runtime_descriptor"\nfi', text)
        self.assertIn('"$runtime_python" -I -S -B "$operator" "$@"', text)
        self.assertIn('exec /usr/bin/env -i HOME="${HOME:?HOME is required}"', text)
        checked = subprocess.run(["/bin/bash", "-n", str(launcher)], capture_output=True, timeout=5)
        self.assertEqual(checked.returncode, 0)
        # Syntax only: these are synthetic data files, never executed runtimes.
        for fallback in ["/usr/bin/python", "command -v", "sudo", "launchctl", "source ", "eval "]:
            self.assertNotIn(fallback, text)

    def test_runtime_launcher_is_create_only_and_rejects_symlink_bin(self):
        # Direct helper exercises publication without staging another bundle.
        (self.payload / "usr/local/bin").mkdir(parents=True, mode=0o755)
        for directory in [self.payload / "usr", self.payload / "usr/local", self.payload / "usr/local/bin"]:
            directory.chmod(0o755)
        existing = self.payload / "usr/local/bin/memory-onboard"
        existing.write_text("existing launcher")
        with self.assertRaises(FileExistsError):
            bundle.stage_launcher(self.payload, self.value)
        self.assertEqual(existing.read_text(), "existing launcher")
        existing.unlink()
        existing.parent.rmdir()
        existing.parent.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(OSError):
            bundle.stage_launcher(self.payload, self.value)
        self.assertFalse((self.root / "memory-onboard").exists())

    def test_lightweight_staging_does_not_add_runtime_launcher(self):
        bundle.stage_bundle(self.source, self.manifest, "x86_64", self.payload)
        self.assertFalse((self.payload / "usr/local/bin/memory-onboard").exists())


if __name__ == "__main__":
    unittest.main()
