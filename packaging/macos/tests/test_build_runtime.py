"""Offline producer metadata/archive checks; does not prove native execution."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

MACOS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MACOS))
spec = importlib.util.spec_from_file_location("producer", MACOS / "build-runtime.py")
producer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(producer)


class RuntimeProducer(unittest.TestCase):
    def test_pinned_sources_and_supported_prefix(self):
        pins = producer.sources(MACOS / "runtime-sources.json")
        self.assertEqual(pins["postgres"]["version"], "17.11")
        self.assertEqual(pins["python"]["version"], "3.14.8")
        self.assertEqual(pins["vector"]["version"], "0.8.7")
        for item in [pins["postgres"], pins["python"], pins["vector"]]:
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
