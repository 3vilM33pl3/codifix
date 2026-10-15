"""Offline producer metadata/archive checks; does not prove native execution."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
spec = importlib.util.spec_from_file_location("producer", MACOS / "build-runtime.py")
producer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(producer)


class RuntimeProducer(unittest.TestCase):
    def test_macho_bundle_dependencies_become_relative_before_signing_and_replay(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prefix = "/usr/local/share/memory-layer/runtime/fixture-v3"
            executable, library = root / "postgres/bin/initdb", root / "postgres/lib/libpq.5.dylib"
            for file in (executable, library):
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(b"\xcf\xfa\xed\xfe" + b"fixture")
            original = prefix + "/postgres/lib/libpq.5.dylib"
            dependencies = {str(executable): [original, "/usr/lib/libSystem.B.dylib"], str(library): [original]}
            identities = {str(executable): None, str(library): original}
            calls = []
            def output(argv, **kwargs):
                file = argv[-1]
                lines = dependencies[file] if argv[1] == "-L" else ([identities[file]] if identities[file] else [])
                return file + ":\n" + "\n".join("\t" + line for line in lines)
            def command(argv, cwd, env):
                argv = list(map(str, argv))
                calls.append(argv)
                if argv[0] == "install_name_tool":
                    if argv[1] == "-change":
                        dependencies[argv[-1]][0] = argv[3]
                    else:
                        identities[argv[-1]] = argv[2]
                        dependencies[argv[-1]][0] = argv[2]
            with patch.object(producer.subprocess, "check_output", side_effect=output), patch.object(producer, "run", side_effect=command):
                producer.audit_and_sign(root, prefix, {})
                self.assertEqual(calls[0], ["install_name_tool", "-change", original, "@loader_path/../lib/libpq.5.dylib", str(executable)])
                self.assertEqual(calls[1][:4], ["codesign", "--force", "--sign", "-"])
                self.assertEqual(calls[3], ["install_name_tool", "-id", "@loader_path/libpq.5.dylib", str(library)])
                calls.clear()
                producer.audit_and_sign(root, prefix, {})
                self.assertFalse(any(call[0] == "install_name_tool" for call in calls))

    def test_macho_external_missing_and_escaping_dependencies_are_never_rewritten(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "runtime"
            executable = root / "bin/program"
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"\xcf\xfa\xed\xfe" + b"fixture")
            prefix = "/usr/local/share/memory-layer/runtime/fixture-v3"
            outside = root.parent / "outside.dylib"
            outside.write_bytes(b"outside")
            for dependency in ["/opt/homebrew/lib/libcrypto.dylib", prefix + "/missing.dylib", prefix + "/../outside.dylib"]:
                def output(argv, **kwargs):
                    return str(executable) + ":\n" + ("\t" + dependency if argv[1] == "-L" else "")
                with self.subTest(dependency=dependency), patch.object(producer.subprocess, "check_output", side_effect=output), patch.object(producer, "run") as command:
                    with self.assertRaises(ValueError):
                        producer.audit_and_sign(root, prefix, {})
                    command.assert_not_called()

    def test_resume_rejects_missing_or_changed_openssl_options_before_build_steps(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary) / "work"
            work.mkdir()
            args = SimpleNamespace(work_dir=str(work), bundle_id="fixture-v3", resume=True, jobs=4)
            original = {"sources": producer.sources(MACOS / "runtime-sources.json"), "arch": "aarch64",
                        "prefix": f"{producer.bundle.BASE}/fixture-v3"}
            changed = dict(original, producer_revision=2, openssl_options=["no-shared"])
            for descriptor in (original, changed):
                (work / "build-inputs.json").write_text(json.dumps(descriptor))
                (work / "openssl.complete").write_text("old completion must not be reused")
                with patch.object(producer.platform, "system", return_value="Darwin"), patch.object(producer.platform, "machine", return_value="arm64"), patch.object(producer, "run") as command:
                    with self.assertRaisesRegex(ValueError, "resume inputs changed"):
                        producer.produce(args)
                    command.assert_not_called()
                self.assertFalse((work / "sources").exists())
                self.assertEqual((work / "openssl.complete").read_text(), "old completion must not be reused")

    def test_postgres_builds_required_contrib_with_explicit_vendored_static_openssl(self):
        dependency = Path("/private/build/openssl-static")
        commands = producer.openssl_commands(dependency, "aarch64", 4)
        self.assertIn("darwin64-arm64-cc", commands[0])
        self.assertTrue({"no-shared", "no-module", "no-apps", "no-tests"}.issubset(commands[0]))
        self.assertTrue({"no-atexit", "no-thread-pool", "no-quic"}.issubset(commands[0]))
        self.assertNotIn("no-threads", commands[0])
        self.assertNotIn("no-async", commands[0])
        self.assertEqual(commands[-1], ["make", "install_dev"])
        prefix = "/usr/local/share/memory-layer/runtime/fixture-v2"
        stage = Path("/private/build/stage")
        commands = producer.postgres_commands(prefix, stage, dependency, 4)
        self.assertIn("--with-ssl=openssl", commands[0])
        self.assertIn(f"--with-includes={dependency}/include", commands[0])
        self.assertIn(f"--with-libraries={dependency}/lib", commands[0])
        for extension in ("pgcrypto", "pg_trgm"):
            self.assertIn(["make", "-C", f"contrib/{extension}", "-j4"], commands)
            self.assertIn(["make", "-C", f"contrib/{extension}", "install", f"DESTDIR={stage}"], commands)

    def extension_fixture(self, root):
        pg = root / "postgres"
        (pg / "lib").mkdir(parents=True)
        (pg / "share/extension").mkdir(parents=True)
        for name in ("pgcrypto", "pg_trgm", "vector"):
            for file in [pg / f"lib/{name}.dylib", pg / f"share/extension/{name}.control", pg / f"share/extension/{name}--1.0.sql"]:
                file.write_text("test fixture")
        return pg

    def test_missing_required_extension_blocks_runtime_validation(self):
        with tempfile.TemporaryDirectory() as temporary:
            pg = self.extension_fixture(Path(temporary))
            producer.required_postgres_extensions(pg)
            for relative in ["lib/pgcrypto.dylib", "share/extension/pg_trgm.control", "share/extension/vector--1.0.sql"]:
                file = pg / relative
                file.unlink()
                with self.subTest(file=relative), self.assertRaisesRegex(ValueError, "required PostgreSQL extension missing"):
                    producer.required_postgres_extensions(pg)
                file.write_text("test fixture")

    def test_extension_smoke_uses_disposable_cluster_and_nonsuperuser_then_stops(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pg = self.extension_fixture(root)
            probe = root / "probe"
            probe.mkdir()
            for fail_query in (False, True):
                calls = []
                def command(argv, cwd, env):
                    calls.append((list(map(str, argv)), cwd, env))
                    if fail_query and "CREATE EXTENSION pgcrypto" in str(argv):
                        raise ValueError("synthetic extension failure")
                with patch.object(producer.tempfile, "TemporaryDirectory") as directory, patch.object(producer, "run", side_effect=command):
                    directory.return_value.__enter__.return_value = str(probe)
                    if fail_query:
                        with self.assertRaisesRegex(ValueError, "synthetic extension failure"):
                            producer.smoke_postgres_extensions(pg, "0.8.7", {"PATH": "/usr/bin:/bin", "PGHOST": "live-host", "PGOPTIONS": "live-options"})
                    else:
                        producer.smoke_postgres_extensions(pg, "0.8.7", {"PATH": "/usr/bin:/bin", "PGHOST": "live-host"})
                self.assertIn("listen_addresses=''", str(calls[1][0]))
                self.assertIn("NOSUPERUSER", str(calls[2][0]))
                self.assertIn("CREATE EXTENSION vector VERSION '0.8.7'", str(calls[4][0]))
                query = calls[5][0]
                self.assertEqual(query[query.index("-U") + 1], "memory_runtime_probe")
                self.assertIn("CREATE EXTENSION pg_trgm", query[-1])
                self.assertEqual(calls[-1][0][-4:], ["-m", "immediate", "-w", "stop"])
                self.assertTrue(all("PGHOST" not in env and "PGOPTIONS" not in env for _, _, env in calls))

    def test_vector_destdir_is_applied_once(self):
        prefix = "/usr/local/share/memory-layer/runtime/fixture"
        stage = Path("/private/build/stage")
        pg = stage / prefix.lstrip("/") / "postgres"
        build, install = producer.vector_commands(pg, prefix, stage, 4)
        self.assertIn(f"includedir_server={pg}/include/server", build)
        self.assertIn(f"DESTDIR={stage}", install)
        for key, suffix in [("pkglibdir", "lib"), ("datadir", "share"), ("includedir_server", "include/server")]:
            self.assertIn(f"{key}={prefix}/postgres/{suffix}", install)
            self.assertFalse(any(arg.startswith(f"{key}={stage}") for arg in install))

    def test_pinned_sources_and_supported_prefix(self):
        pins = producer.sources(MACOS / "runtime-sources.json")
        self.assertEqual(pins["postgres"]["version"], "17.11")
        self.assertEqual(pins["python"]["version"], "3.14.8")
        self.assertEqual(pins["vector"]["version"], "0.8.7")
        self.assertEqual(pins["openssl"]["version"], "3.5.9")
        self.assertEqual(pins["openssl"]["sha256"], "603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a")
        self.assertLess(producer.SOURCE_ORDER.index("openssl"), producer.SOURCE_ORDER.index("postgres"))
        for item in [pins[key] for key in producer.SOURCE_ORDER]:
            self.assertEqual(len(bytes.fromhex(item["sha256"])), 32)

    def archive(self, root, name, *, link=False):
        archive = root / "source.tar"
        with tarfile.open(archive, "w") as stream:
            info = tarfile.TarInfo(name)
            if link:
                info.type, info.linkname = tarfile.SYMTYPE, "/private/secret"
                stream.addfile(info)
            else:
                data = b"source"
                info.size = len(data)
                stream.addfile(info, io.BytesIO(data))
        return archive, {"directory": "source-1", "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}

    def test_checked_source_archive_rejects_traversal_links_and_wrong_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, source = self.archive(root, "source-1/file")
            producer.checked_archive(archive, source)
            source["sha256"] = "0" * 64
            with self.assertRaises(ValueError):
                producer.checked_archive(archive, source)
            for name, link in [("source-1/../../secret", False), ("another/file", False), ("source-1/alias", True)]:
                archive, source = self.archive(root, name, link=link)
                with self.assertRaises(ValueError):
                    producer.checked_archive(archive, source)

    def test_flatten_only_internal_file_aliases(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "original").write_bytes(b"program")
            (root / "alias").symlink_to("original")
            producer.flatten_links(root)
            self.assertFalse((root / "alias").is_symlink())
            self.assertEqual((root / "alias").read_bytes(), b"program")
            (root / "outside").symlink_to("/etc/passwd")
            with self.assertRaises(ValueError):
                producer.flatten_links(root)

    def test_flatten_installed_timezone_hardlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original, alias = root / "zone", root / "zone-alias"
            original.write_bytes(b"timezone")
            os.link(original, alias)
            producer.flatten_links(root)
            self.assertEqual(original.stat().st_nlink, 1)
            self.assertEqual(alias.stat().st_nlink, 1)
            self.assertEqual(alias.read_bytes(), original.read_bytes())

    def test_source_directory_alias_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            file = Path(temporary) / "pins.json"
            value = producer.sources(MACOS / "runtime-sources.json")
            value["python"]["directory"] = "../escape"
            file.write_text(json.dumps(value))
            with self.assertRaises(producer.bundle.Refused):
                producer.sources(file)


if __name__ == "__main__":
    unittest.main()
