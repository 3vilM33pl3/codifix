# SPDX-License-Identifier: AGPL-3.0-or-later
import unittest
import xml.etree.ElementTree as ET
import tempfile
from pathlib import Path
from unittest.mock import patch
import vm


class SeedTests(unittest.TestCase):
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
