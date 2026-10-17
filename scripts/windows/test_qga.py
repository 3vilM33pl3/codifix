# SPDX-License-Identifier: AGPL-3.0-or-later
"""Synthetic sockets/RPC only. Never connects to the Windows VM."""
import base64
import contextlib
import hashlib
import json
import os
import re
from pathlib import Path
import socket
import tempfile
import threading
import unittest
from unittest.mock import patch
import vm


@contextlib.contextmanager
def fake_agent(root, result):
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(root / 'guest-agent.sock'))
    os.chmod(root / 'guest-agent.sock', 0o600)
    listener.listen(1)
    seen = []
    errors = []
    def serve():
        try:
            with listener.accept()[0] as connection, connection.makefile('rwb') as stream:
                sync = json.loads(stream.readline().lstrip(b'\xff'))
                assert sync['execute'] == 'guest-sync-delimited'
                stream.write(b'{"return":"stale"}\n\xff' + json.dumps({'return': sync['arguments']['id']}).encode() + b'\n')
                stream.flush()
                seen.append(json.loads(stream.readline()))
                stream.write(json.dumps(result).encode() + b'\n')
                stream.flush()
        except Exception as error:
            errors.append(error)
    worker = threading.Thread(target=serve, daemon=True)
    worker.start()
    try:
        yield seen
    finally:
        worker.join(2)
        listener.close()
        if worker.is_alive() or errors:
            raise AssertionError(f'fake guest agent failed: {errors}')


class GuestAgentTests(unittest.TestCase):
    def test_stage_start_is_hash_bound_interactive_and_process_policy_only(self):
        operation='11111111-1111-4111-8111-111111111111'
        with patch.object(vm,'guest_exec',return_value={'pid':42}) as execute:
            result=vm.guest_stage_start(Path('/unused'),'NativeApi',operation)
        self.assertEqual(result['status'],'dispatch_requested')
        self.assertEqual(result['pid'],42)
        self.assertFalse(result['machine_execution_policy_changed'])
        self.assertEqual(execute.call_args.args[1],r'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe')
        arguments=execute.call_args.args[2]
        self.assertEqual(arguments[:4],['-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand'])
        outer=base64.b64decode(arguments[4]).decode('utf-16-le')
        inner_encoded=re.search(r"\$encoded='([A-Za-z0-9+/=]+)'",outer).group(1)
        inner=base64.b64decode(inner_encoded).decode('utf-16-le')
        for script in (outer,inner):
            self.assertIn(result['runner_sha256'],script)
            self.assertIn("$env:COMPUTERNAME -ne 'MEMORY-WINLAB'",script)
            self.assertNotIn('Set-ExecutionPolicy',script)
            self.assertNotIn('Password',script)
        self.assertIn("-LogonType Interactive -RunLevel Limited",outer)
        self.assertEqual(result['stage_execution_identity'],r'MEMORY-WINLAB\memorylab (Interactive, Limited)')
        self.assertIn("'MEMORY-WINLAB\\memorylab'",outer)
        self.assertIn("-NoProfile -ExecutionPolicy Bypass -EncodedCommand",outer)
        self.assertIn('CreateNew',outer)
        self.assertIn('SetAccessRuleProtection($true,$false)',outer)
        self.assertIn('Launch task exists; inspect status',outer)
        self.assertNotIn(' -Force',outer.replace('Get-Item -LiteralPath $path -Force',''))
        self.assertLess(inner.index('Get-FileHash'),inner.index('& $runner'))
        self.assertIn(f"-Action Start -Stage NativeApi -OperationId '{operation}'",inner)
        self.assertIn(f"C:\\MemoryLab\\stage-start-{operation}.log",inner)

    def test_installation_stages_keep_explicit_highest_identity(self):
        for stage in ['BuildTools','PostgresPrepare','PostgresStart','PdsPrepare']:
            with self.subTest(stage=stage),patch.object(vm,'guest_exec',return_value={'pid':42}) as execute:
                source=r'C:\MemoryLab\source' if stage=='PdsPrepare' else None
                result=vm.guest_stage_start(Path('/unused'),stage,'11111111-1111-4111-8111-111111111111',source)
                outer=base64.b64decode(execute.call_args.args[2][-1]).decode('utf-16-le')
                self.assertIn('-LogonType Interactive -RunLevel Highest',outer)
                self.assertNotIn('-RunLevel Limited',outer)
                self.assertEqual(result['stage_execution_identity'],r'MEMORY-WINLAB\memorylab (Interactive, Highest)')

    def test_source_build_stages_are_limited_and_reject_missing_source(self):
        operation='11111111-1111-4111-8111-111111111111'
        for stage in ['PiBuild','ProductCheck','ProductBuild','NativeEval']:
            with self.subTest(stage=stage),patch.object(vm,'guest_exec',return_value={'pid':42}) as execute:
                result=vm.guest_stage_start(Path('/unused'),stage,operation,r'C:\MemoryLab\source\reviewed-v1')
                outer=base64.b64decode(execute.call_args.args[2][-1]).decode('utf-16-le')
                inner=base64.b64decode(re.search(r"\$encoded='([A-Za-z0-9+/=]+)'",outer).group(1)).decode('utf-16-le')
                self.assertIn('-LogonType Interactive -RunLevel Limited',outer)
                self.assertIn("-SourceRoot 'C:\\MemoryLab\\source\\reviewed-v1'",inner)
                self.assertEqual(result['stage'],stage)
            with patch.object(vm,'guest_exec') as execute,self.assertRaises(ValueError):
                vm.guest_stage_start(Path('/unused'),stage,operation)
            execute.assert_not_called()

    def test_pds_stdio_proof_is_limited_and_does_not_accept_source_arguments(self):
        operation='11111111-1111-4111-8111-111111111111'
        with patch.object(vm,'guest_exec',return_value={'pid':42}) as execute:
            vm.guest_stage_start(Path('/unused'),'PdsVerify',operation)
            outer=base64.b64decode(execute.call_args.args[2][-1]).decode('utf-16-le')
            self.assertIn('-LogonType Interactive -RunLevel Limited',outer)
        with self.assertRaises(ValueError):
            vm.guest_stage_start(Path('/unused'),'PdsVerify',operation,r'C:\MemoryLab\source')

    def test_stage_start_rejects_unknown_stage_uuid_and_source_injection_before_rpc(self):
        operation='11111111-1111-4111-8111-111111111111'
        invalid=[('Unknown',operation,None),('NativeApi;whoami',operation,None),
                 ('NativeApi','00000000-0000-0000-0000-000000000000',None),
                 ('NativeApi',operation+';whoami',None),('NativeApi','not-uuid',None),
                 ('NativeApi',operation,r'C:\MemoryLab\source'),('PdsPrepare',operation,None)]
        invalid.extend(('PdsPrepare',operation,path) for path in
                       [r'C:\Windows',r'C:\MemoryLab\source-other',r'C:\MemoryLab\source\..\other',
                        r'C:\MemoryLab\source\CON',r'C:\MemoryLab\source\a:stream',
                        "C:\\MemoryLab\\source\\x';whoami",r'C:\MemoryLab\source\.',
                        r'C:/MemoryLab/source',r'\\host\share\source'])
        with patch.object(vm,'guest_exec') as execute:
            for stage,identity,source in invalid:
                with self.subTest(stage=stage,identity=identity,source=source),self.assertRaises(ValueError):
                    vm.guest_stage_start(Path('/unused'),stage,identity,source)
            execute.assert_not_called()
        with patch.object(vm,'guest_exec',return_value={'pid':42}) as execute:
            result=vm.guest_stage_start(Path('/unused'),'PdsPrepare',operation,r'C:\MemoryLab\source\reviewed-v1')
            self.assertEqual(result['stage'],'PdsPrepare')
            outer=base64.b64decode(execute.call_args.args[2][-1]).decode('utf-16-le')
            inner=base64.b64decode(re.search(r"\$encoded='([A-Za-z0-9+/=]+)'",outer).group(1)).decode('utf-16-le')
            self.assertIn("-SourceRoot 'C:\\MemoryLab\\source\\reviewed-v1'",inner)

    def test_transport_timeout_is_not_retried_or_reported_as_rollback(self):
        with patch.object(vm, '_qga_once', side_effect=TimeoutError('untrusted secret detail')) as once:
            with self.assertRaisesRegex(ValueError, 'may be uncertain') as caught:
                vm.qga(Path('/unused'), 'guest-exec', {})
        once.assert_called_once()
        self.assertNotIn('secret', str(caught.exception))

    def test_private_socket_resync_and_literal_async_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with fake_agent(root, {'return': {'pid': 42}}) as seen:
                result = vm.guest_exec(root, r'C:\MemoryLab\inputs\native.exe', ['space argument', '; not a shell'], capture=True)
            self.assertEqual(result['pid'], 42)
            self.assertEqual(seen, [{'execute': 'guest-exec', 'arguments': {
                'path': r'C:\MemoryLab\inputs\native.exe', 'arg': ['space argument', '; not a shell'], 'capture-output': True}}])

    def test_rpc_errors_do_not_print_untrusted_agent_descriptions(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with fake_agent(root, {'error': {'class': 'GenericError', 'desc': 'secret\x1b[31m'}}):
                with self.assertRaises(ValueError) as caught:
                    vm.qga(root, 'guest-info')
            self.assertNotIn('secret', str(caught.exception))

    def test_status_is_metadata_only_until_explicit_output_request_and_retains_receipt(self):
        data = b'private synthetic\x1b[31m\xff'
        result = {'exited': True, 'exitcode': 0, 'out-data': base64.b64encode(data).decode(), 'out-truncated': False}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(vm, 'qga', return_value=result):
                summary = vm.guest_status(root, 42)
            self.assertNotIn('text', summary['stdout'])
            self.assertNotIn('private synthetic', json.dumps(summary))
            receipt = summary['saved_status']
            self.assertEqual((root / receipt).stat().st_mode & 0o077, 0)
            with patch.object(vm, 'qga', side_effect=AssertionError('saved result must not contact VM')):
                shown = vm.guest_status(root, 42, True, receipt)
            encoded = json.dumps(shown)
            self.assertIn('private synthetic', encoded)
            self.assertNotIn('\x1b', encoded)
            self.assertEqual(shown['stdout']['sha256'], hashlib.sha256(data).hexdigest())
            with self.assertRaises(ValueError):
                vm.guest_status(root, 42, True, '../other.json')

    def test_execution_and_copy_paths_reject_implicit_lookup_devices_and_escape(self):
        with patch.object(vm, 'qga') as rpc:
            for executable in ['powershell.exe', 'script.ps1', r'\\host\share\x.exe', 'C:x.exe']:
                with self.assertRaises(ValueError):
                    vm.guest_exec(Path('/unused'), executable, [])
            for target in [r'C:\Windows\x.exe', r'C:\MemoryLab\inputs\..\other', r'C:\MemoryLab\inputs\CON.txt',
                           r'C:\MemoryLab\inputs\x:stream', r'C:\MemoryLab\inputs\x.', r'C:\MemoryLab\inputs\a;whoami',
                           r'\\host\share\x', 'C:/MemoryLab/inputs/a']:
                with self.assertRaises(ValueError):
                    vm.guest_input_target(target)
            rpc.assert_not_called()

    def test_copy_uses_create_new_bounded_chunks_flush_close_and_readback(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source.zip'
            data = b'synthetic' * 5
            source.write_bytes(data)
            uploaded = bytearray()
            calls = []
            def rpc(_root, command, args):
                calls.append((command, args))
                if command == 'guest-exec':
                    self.assertIn('CreateNew', args['arg'][-1])
                    self.assertNotIn('synthetic', args['arg'][-1])
                    return {'pid': 12}
                if command == 'guest-exec-status': return {'exited': True, 'exitcode': 0}
                if command == 'guest-file-open': return 55
                if command == 'guest-file-write':
                    part = base64.b64decode(args['buf-b64'], validate=True)
                    self.assertLessEqual(len(part), 8)
                    uploaded.extend(part)
                    return {'count': len(part), 'eof': False}
                if command == 'guest-file-read':
                    part = bytes(uploaded[:args['count']])
                    del uploaded[:len(part)]
                    return {'count': len(part), 'buf-b64': base64.b64encode(part).decode(), 'eof': not uploaded}
                return {}
            with patch.object(vm, 'qga', side_effect=rpc), patch.object(vm, 'QGA_CHUNK', 8):
                result = vm.guest_copy_in(root, source, r'C:\MemoryLab\inputs\source.zip')
            self.assertEqual(result['sha256'], hashlib.sha256(data).hexdigest())
            self.assertTrue(result['read_back_verified'])
            self.assertFalse(result['overwrite'])
            self.assertEqual([args['mode'] for cmd, args in calls if cmd == 'guest-file-open'], ['r+b', 'rb'])
            self.assertEqual(len([cmd for cmd, _ in calls if cmd == 'guest-file-close']), 2)

    def test_failed_existing_destination_never_opens_or_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / 'source'
            source.write_bytes(b'synthetic')
            with patch.object(vm, 'qga', side_effect=[{'pid': 12}, {'exited': True, 'exitcode': 1}]) as rpc:
                with self.assertRaisesRegex(ValueError, 'never overwritten'):
                    vm.guest_copy_in(Path(temporary), source, r'C:\MemoryLab\inputs\existing.txt')
            self.assertEqual(rpc.call_count, 2)

    def test_short_write_closes_handle_without_retry_or_claiming_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / 'source'
            source.write_bytes(b'synthetic')
            with patch.object(vm, 'qga', side_effect=[{'pid': 12}, {'exited': True, 'exitcode': 0}, 55, {'count': 1}, {}]) as rpc:
                with self.assertRaisesRegex(ValueError, 'partial input retained'):
                    vm.guest_copy_in(Path(temporary), source, r'C:\MemoryLab\inputs\new.txt')
            self.assertEqual(rpc.call_args.args[1], 'guest-file-close')
            self.assertEqual(rpc.call_count, 5)

    def test_changed_readback_and_oversized_source_are_not_verified(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source'
            source.write_bytes(b'synthetic')
            with patch.object(vm, 'qga') as rpc, patch.object(vm, 'QGA_COPY_LIMIT', 4):
                with self.assertRaises(ValueError):
                    vm.guest_copy_in(root, source, r'C:\MemoryLab\inputs\new.txt')
                rpc.assert_not_called()
            replies = [{'pid': 12}, {'exited': True, 'exitcode': 0}, 55, {'count': 9}, {}, {}, 56,
                       {'count': 9, 'buf-b64': base64.b64encode(b'different').decode(), 'eof': True}, {}]
            with patch.object(vm, 'qga', side_effect=replies):
                with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                    vm.guest_copy_in(root, source, r'C:\MemoryLab\inputs\new.txt')

    def test_non_socket_and_unprivate_socket_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / 'guest-agent.sock'
            target.write_bytes(b'not a socket')
            with self.assertRaises(ValueError):
                vm.qga(root, 'guest-info')


if __name__ == '__main__':
    unittest.main()
