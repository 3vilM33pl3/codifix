# SPDX-License-Identifier: AGPL-3.0-or-later
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('windows_pi_smoke',Path(__file__).with_name('smoke.py'))
smoke=importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)


class NativeSmokeTests(unittest.TestCase):
    def test_candidate_checks_actual_pe_machine_and_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'pi.exe'
            data=bytearray(128); data[:2]=b'MZ'; data[60:64]=(64).to_bytes(4,'little'); data[64:70]=b'PE\0\0\x64\x86'
            path.write_bytes(data)
            smoke.candidate_check(path,hashlib.sha256(data).hexdigest())
            for broken in (b'linux ELF',data[:64]+b'PE\0\0\x4c\x01'+data[70:]):
                path.write_bytes(broken)
                with self.assertRaises(ValueError):
                    smoke.candidate_check(path,hashlib.sha256(broken).hexdigest())
            path.write_bytes(data)
            with self.assertRaises(ValueError): smoke.candidate_check(path,'0'*64)

    def test_reuses_strict_shared_validator_without_claiming_sandbox(self):
        root=Path(__file__).resolve().parents[2]
        library=root/'packaging/ca-bookworm/pi_smoke.py'
        shared=smoke.protocol_library(library,hashlib.sha256(library.read_bytes()).hexdigest())
        with self.assertRaises(shared.CheckFailed):
            shared.validate_request({'model':shared.MODEL,'tools':[{'name':'forbidden'}]})
        source=Path(smoke.__file__).read_text()
        for text in ('"native_sandbox_verified": False','"inherited_broker_verified": False',
                     'subprocess.CREATE_NO_WINDOW','provider.calls == provider.completed == 1'):
            self.assertIn(text,source)
        self.assertNotIn('os.environ.copy()',source)

    def test_stage_binds_exact_existing_builder_and_protocol_inputs(self):
        root=Path(__file__).resolve().parents[2]
        manifest=json.loads((root/'scripts/windows/native-pi.json').read_text())
        self.assertEqual(manifest['source_revision'],'7c018475d54d4e09c907691e47a520f1f09e3288')
        self.assertEqual(manifest['target'],'x86_64-pc-windows-msvc')
        paths=[item['path'] for item in manifest['inputs']]
        self.assertEqual(len(paths),len(set(paths)))
        self.assertEqual(paths[0],'crates/codifix-supervisor-pi/scripts/build-windows.ps1')
        for item in manifest['inputs']:
            self.assertEqual(hashlib.sha256((root/item['path']).read_bytes()).hexdigest(),item['sha256'])
        stage=(root/'scripts/windows/guest-native-pi.ps1').read_text()
        for guard in ('Assert-NoReparse','Pi proof input order differs','GIT_CONFIG_GLOBAL=\'NUL\'',
                      'No caller/provider credentials','incomplete native build retained'.capitalize(),
                      "native_execution_verified=$false;isolation_verified=$false"):
            self.assertIn(guard,stage)


if __name__=='__main__': unittest.main()
