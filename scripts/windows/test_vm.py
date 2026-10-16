# SPDX-License-Identifier: AGPL-3.0-or-later
import json
import unittest
import xml.etree.ElementTree as ET
import tempfile
from pathlib import Path
from unittest.mock import patch
import vm


class SeedTests(unittest.TestCase):
    def test_secure_boot_preserves_prior_disk_and_firmware(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'snapshots').mkdir()
            (root / 'tpm').mkdir()
            (root / 'disk.qcow2').write_bytes(b'original-disk')
            (root / 'nvram.fd').write_bytes(b'original-nvram')
            data = {'format': vm.FORMAT, 'root': str(root), 'firmware': 'old-code'}
            vm.write_json(root / 'lab.json', data)
            code = root / 'secure-code'
            keys = root / 'ms-keys'
            code.write_bytes(b'packaged-secure-code')
            keys.write_bytes(b'packaged-public-keys')
            original_path = vm.Path
            def firmware_path(value):
                return {'/usr/share/OVMF/OVMF_CODE_4M.secboot.fd': code,
                        '/usr/share/OVMF/OVMF_VARS_4M.ms.fd': keys}.get(value, original_path(value))
            with patch.object(vm, 'require_stopped'), patch.object(vm, 'Path', side_effect=firmware_path):
                vm.enable_secure_boot(root, data)
            changed = json.loads((root / 'lab.json').read_text())
            self.assertTrue(changed['secure_boot'])
            self.assertEqual((root / 'disk.qcow2').read_bytes(), b'original-disk')
            self.assertEqual((root / 'nvram.fd').read_bytes(), b'packaged-public-keys')
            previous = root / 'snapshots' / changed['firmware_backup']
            self.assertEqual((previous / 'nvram.fd').read_bytes(), b'original-nvram')
            vm.restore(root, previous.name)
            self.assertEqual(json.loads((root / 'lab.json').read_text()), data)
            self.assertEqual((root / 'nvram.fd').read_bytes(), b'original-nvram')

    def test_unattend_is_well_formed_and_wipes_only_lab_disk(self):
        ns = {'u': 'urn:schemas-microsoft-com:unattend'}
        document = ET.fromstring(vm.unattended('synthetic<&password'))
        self.assertEqual(document.find('.//u:DiskID', ns).text, '0')
        self.assertEqual(document.find('.//u:WillWipeDisk', ns).text, 'true')
        self.assertEqual(document.find('.//u:Password/u:Value', ns).text, 'synthetic<&password')
        self.assertEqual(document.find('.//u:MetaData/u:Value', ns).text,
                         'Windows 11 Enterprise Evaluation')
        self.assertEqual(document.find('.//u:AutoLogon/u:LogonCount', ns).text, '1')

    def test_production_and_relative_roots_rejected(self):
        for root in ['/data', '/', '/home/olivier', 'lab']:
            with self.assertRaises(ValueError):
                vm.private_root(root)

    def test_cold_snapshot_and_interrupted_restore_preserve_all_previous_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'snapshots').mkdir()
            (root / 'tpm').mkdir()
            for name in ['disk.qcow2', 'nvram.fd', 'tpm/state']:
                (root / name).write_bytes(b'snapshot')
            vm.snapshot(root, 'clean')
            for name in ['disk.qcow2', 'nvram.fd', 'tpm/state']:
                (root / name).write_bytes(b'new-work')
            rename = Path.rename
            interrupted = False

            def interrupt_once(path, target):
                nonlocal interrupted
                result = rename(path, target)
                if path == root / 'disk.qcow2' and not interrupted:
                    interrupted = True
                    raise RuntimeError('synthetic interruption after retaining prior disk')
                return result

            with patch.object(Path, 'rename', interrupt_once):
                with self.assertRaises(RuntimeError):
                    vm.restore(root, 'clean')
            operation = next(root.glob('restore-*'))
            self.assertFalse((operation / 'complete.json').exists())
            vm.recover_restore(root, operation.name)
            vm.recover_restore(root, operation.name)  # Exact replay is harmless.
            for name in ['disk.qcow2', 'nvram.fd', 'tpm/state']:
                self.assertEqual((root / name).read_bytes(), b'snapshot')
            self.assertEqual((operation / 'previous-disk.qcow2').read_bytes(), b'new-work')
            self.assertEqual((operation / 'previous-tpm/state').read_bytes(), b'new-work')


if __name__ == '__main__':
    unittest.main()
