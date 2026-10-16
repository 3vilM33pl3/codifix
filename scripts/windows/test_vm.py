# SPDX-License-Identifier: AGPL-3.0-or-later
import json
import os
import socket
import unittest
import xml.etree.ElementTree as ET
import tempfile
from types import SimpleNamespace
from pathlib import Path
from unittest.mock import patch
import vm


class SeedTests(unittest.TestCase):
    def test_relocation_rejects_pending_restore_or_firmware_before_any_change(self):
        for pending in ('restore-123','secure-boot-nvram.partial'):
            with self.subTest(pending=pending), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary) / 'lab'; root.mkdir(mode=0o700)
                original=root / 'disk.qcow2'; original.write_bytes(b'original')
                marker=root / pending
                if pending.startswith('restore-'):
                    marker.mkdir(mode=0o700)
                    vm.write_json(marker / 'intent.json',{'retained':'do not change'})
                else:
                    marker.write_bytes(b'pending firmware')
                before={str(path.relative_to(root)):path.read_bytes()
                        for path in root.rglob('*') if path.is_file()}
                base=Path(temporary) / 'nvme'
                with patch.object(vm,'ACTIVE_DISK_BASE',base),patch.object(vm,'require_stopped'), \
                        patch.object(vm,'disk_information') as information:
                    with self.assertRaisesRegex(ValueError,'interrupted'):
                        vm.relocate_disk(root,base / 'active')
                    information.assert_not_called()
                self.assertFalse(base.exists())
                after={str(path.relative_to(root)):path.read_bytes()
                       for path in root.rglob('*') if path.is_file()}
                self.assertEqual(before,after)

    def test_active_descriptor_never_falls_back_to_legacy_disk(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'lab'
            root.mkdir(mode=0o700)
            base = Path(temporary) / 'nvme'
            base.mkdir(mode=0o700)
            selected = base / 'active'
            selected.mkdir(mode=0o700)
            disk = selected / 'disk.qcow2'
            (root / 'disk.qcow2').write_bytes(b'legacy')
            with patch.object(vm, 'ACTIVE_DISK_BASE', base):
                self.assertEqual(vm.active_disk(root), root / 'disk.qcow2')
                vm.write_json(root / 'active-disk.json', {'format':vm.ACTIVE_DISK_FORMAT,
                    'root':str(root),'path':str(disk),'virtual_size':200*1024**3})
                with self.assertRaisesRegex(ValueError, 'missing'):
                    vm.active_disk(root)
                outside = Path(temporary) / 'outside'
                outside.write_bytes(b'outside')
                disk.symlink_to(outside)
                with self.assertRaisesRegex(ValueError, 'symbolic'):
                    vm.active_disk(root)
                disk.unlink()
                disk.write_bytes(b'new')
                disk.chmod(0o600)
                with patch.object(vm, 'disk_information', return_value={'format':'qcow2','virtual_size':200*1024**3}):
                    self.assertEqual(vm.active_disk(root),disk)
                os.link(disk, selected / 'hardlink')
                with self.assertRaisesRegex(ValueError, 'single-linked'):
                    vm.active_disk(root)
                with self.assertRaises(ValueError):
                    vm.private_disk_path(base / '..' / 'outside')

    def test_relocation_binds_exact_copy_and_retains_hdd_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'lab'
            root.mkdir(mode=0o700)
            (root / 'tpm').mkdir(mode=0o700)
            for entry in ('disk.qcow2','nvram.fd','tpm/state'):
                (root / entry).write_bytes(b'synthetic-' + entry.encode())
            data={'format':vm.FORMAT,'root':str(root)}
            vm.write_json(root / 'lab.json',data)
            base=Path(temporary) / 'nvme'
            with patch.object(vm,'ACTIVE_DISK_BASE',base), patch.object(vm,'require_stopped') as stopped, \
                    patch.object(vm,'disk_information',return_value={'format':'qcow2','virtual_size':200*1024**3}):
                vm.relocate_disk(root,base / 'active')
                stopped.assert_called_once_with(root)
                selected=vm.active_disk(root)
                self.assertEqual(selected.read_bytes(),(root / 'disk.qcow2').read_bytes())
                self.assertEqual(json.loads((root / 'lab.json').read_text()),data)
                descriptor=json.loads((root / 'active-disk.json').read_text())
                self.assertEqual(descriptor['initial_sha256'],vm.digest(selected))
                recovery=Path(descriptor['recovery_directory'])
                self.assertEqual((recovery / 'nvram.fd').read_bytes(),(root / 'nvram.fd').read_bytes())
                self.assertEqual((recovery / 'tpm/state').read_bytes(),(root / 'tpm/state').read_bytes())
                self.assertTrue((recovery / 'complete.json').is_file())
                selected.write_bytes(b'legitimate guest writes')
                self.assertEqual(vm.active_disk(root),selected)  # Copy hash is historical.
                with self.assertRaises(ValueError):
                    vm.relocate_disk(root,base / 'another')

    def test_relocation_failed_copy_retains_original_selection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary) / 'lab'; root.mkdir(mode=0o700)
            (root / 'tpm').mkdir(mode=0o700)
            for entry in ('disk.qcow2','nvram.fd','tpm/state'):
                (root / entry).write_bytes(b'synthetic')
            vm.write_json(root / 'lab.json',{'format':vm.FORMAT,'root':str(root)})
            base=Path(temporary) / 'nvme'; original_run=vm.run
            def corrupt_copy(*args,**kwargs):
                result=original_run(*args,**kwargs)
                if args[0]=='cp' and args[-1]==base / 'active/disk.qcow2':
                    Path(args[-1]).write_bytes(b'corrupt')
                return result
            with patch.object(vm,'ACTIVE_DISK_BASE',base),patch.object(vm,'require_stopped'), \
                    patch.object(vm,'disk_information',return_value={'format':'qcow2','virtual_size':200*1024**3}), \
                    patch.object(vm,'run',side_effect=corrupt_copy):
                with self.assertRaisesRegex(ValueError,'differs'):
                    vm.relocate_disk(root,base / 'active')
                self.assertEqual(vm.active_disk(root),root / 'disk.qcow2')
                self.assertEqual((root / 'disk.qcow2').read_bytes(),b'synthetic')
                self.assertTrue((base / 'active/disk.qcow2').exists())

    def test_relocated_restore_retains_disks_beside_active_disk_and_recovers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary) / 'lab'; root.mkdir(mode=0o700)
            (root / 'snapshots').mkdir(); (root / 'tpm').mkdir(mode=0o700)
            for entry in ('disk.qcow2','nvram.fd','tpm/state'):
                (root / entry).write_bytes(b'clean')
            vm.write_json(root / 'lab.json',{'format':vm.FORMAT,'root':str(root),'old_firmware':True})
            base=Path(temporary) / 'nvme'
            with patch.object(vm,'ACTIVE_DISK_BASE',base),patch.object(vm,'require_stopped'), \
                    patch.object(vm,'disk_information',return_value={'format':'qcow2','virtual_size':200*1024**3}):
                vm.snapshot(root,'old-hdd-snapshot')
                vm.relocate_disk(root,base / 'active')
                disk=vm.active_disk(root); disk.write_bytes(b'new work')
                (root / 'disk.qcow2').write_bytes(b'inactive preserved')
                rename=Path.rename
                def interrupt(path,target):
                    result=rename(path,target)
                    if path==disk:
                        raise RuntimeError('synthetic interruption after active disk retention')
                    return result
                with patch.object(Path,'rename',interrupt):
                    with self.assertRaises(RuntimeError):
                        vm.restore(root,'old-hdd-snapshot')
                operation=next(root.glob('restore-*'))
                self.assertFalse((operation / 'complete.json').exists())
                vm.recover_restore(root,operation.name)
                vm.recover_restore(root,operation.name)
                self.assertEqual(vm.active_disk(root),disk)
                self.assertEqual(disk.read_bytes(),b'clean')
                self.assertEqual((disk.parent / operation.name / 'previous-disk.qcow2').read_bytes(),b'new work')
                self.assertFalse((operation / 'previous-disk.qcow2').exists())
                self.assertEqual((root / 'disk.qcow2').read_bytes(),b'inactive preserved')

    def test_active_disk_qcow_geometry_and_backing_are_checked(self):
        for info in ({'format':'raw','virtual-size':200*1024**3},
                     {'format':'qcow2','virtual-size':1024},
                     {'format':'qcow2','virtual-size':200*1024**3,'backing-filename':'parent'}):
            with patch.object(vm,'run',return_value=SimpleNamespace(stdout=json.dumps(info))):
                with self.assertRaises(ValueError):
                    vm.disk_information(Path('/synthetic'))

    def test_start_protects_qga_socket_even_if_qemu_resets_umask(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            data = {'cpus': 8, 'memory_mib': 16384, 'firmware': 'synthetic',
                    'ssh_port': 22290, 'virtio_iso': None}
            listener = socket.socket(socket.AF_UNIX)
            try:
                def fake_run(*args, **kwargs):
                    if args[0] == 'qemu-system-x86_64':
                        self.assertEqual(kwargs['umask'], 0o077)
                        listener.bind(str(root / 'guest-agent.sock'))
                        os.chmod(root / 'guest-agent.sock', 0o755)
                with patch.object(vm, 'require_stopped'), patch.object(vm, 'kvm'), \
                        patch.object(vm, 'run', side_effect=fake_run), patch.object(vm, 'running', return_value=True):
                    vm.start(root, data, False)
                self.assertEqual((root / 'guest-agent.sock').stat().st_mode & 0o777, 0o600)
            finally:
                listener.close()

    def test_seed_refresh_preserves_account_key_and_disk(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            seed = root / 'seed'
            seed.mkdir()
            original = vm.unattended('synthetic<&password')
            (seed / 'Autounattend.xml').write_bytes(original)
            (seed / 'authorized_keys').write_bytes(b'synthetic-public-key')
            (seed / 'bootstrap.ps1').write_bytes(b'original-bootstrap')
            (root / 'seed.iso').write_bytes(b'original-iso')
            (root / 'disk.qcow2').write_bytes(b'original-disk')

            def generate_iso(*args, **kwargs):
                Path(args[args.index('-o') + 1]).write_bytes(b'new-iso')

            with patch.object(vm, 'require_stopped'), patch.object(vm, 'run', side_effect=generate_iso):
                vm.refresh_seed(root)
            retained = next(root.glob('seed-refresh-*'))
            self.assertEqual((retained / 'previous-seed/Autounattend.xml').read_bytes(), original)
            self.assertEqual((retained / 'previous-seed.iso').read_bytes(), b'original-iso')
            self.assertEqual((seed / 'Autounattend.xml').read_bytes(), original)
            self.assertEqual((seed / 'authorized_keys').read_bytes(), b'synthetic-public-key')
            self.assertEqual((root / 'disk.qcow2').read_bytes(), b'original-disk')
            self.assertEqual((root / 'seed.iso').read_bytes(), b'new-iso')

    def test_copy_input_names_cannot_be_commands_or_remote_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / 'source.zip'
            source.write_bytes(b'synthetic')
            for name in ['../source', 'a/b', 'x:y', 'a;whoami', '-option', '.hidden', 'a b', '',
                         'NUL', 'con.txt', 'COM1.zip', 'lpt9', 'trailing.']:
                with self.assertRaises(ValueError):
                    vm.copy_in(Path(temporary), {}, source, name)
            with patch.object(vm, 'ssh'), patch.object(vm, 'run') as run:
                vm.copy_in(Path(temporary), {'ssh_port': 22290}, source, 'source.zip')
                self.assertEqual(run.call_args.args[-1],
                    'memorylab@127.0.0.1:C:/MemoryLab/inputs/source.zip')

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
        modifications = document.findall('.//u:ModifyPartition', ns)
        self.assertEqual([p.find('u:Order', ns).text for p in modifications], ['1', '2'])
        self.assertEqual([p.find('u:PartitionID', ns).text for p in modifications], ['1', '3'])
        oobe_locale = document.find("u:settings[@pass='oobeSystem']/u:component[@name='Microsoft-Windows-International-Core']", ns)
        self.assertIsNotNone(oobe_locale)
        self.assertEqual(oobe_locale.find('u:InputLocale', ns).text, '0809:00000809')
        self.assertEqual(oobe_locale.find('u:UserLocale', ns).text, 'en-GB')

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
