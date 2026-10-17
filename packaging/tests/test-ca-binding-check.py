#!/usr/bin/env python3
"""Synthetic only: never probes a host service, credential or database."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[2] / 'deploy/areas/ca-binding-check.py'
spec = importlib.util.spec_from_file_location('binding', SOURCE)
binding = importlib.util.module_from_spec(spec)
spec.loader.exec_module(binding)
SECRET = 'never-emit-this-secret'


class BindingTests(unittest.TestCase):
    def good(self, **changes):
        defaults = dict(service=42, identity=(b'100', (7, 8, 9, 10)),
                        binary_hash=binding.EXPECTED, loader_inputs=None,
                        credential=b'[database]\nurl="postgres://role:private@127.0.0.1:5433/memory_trust"',
                        sockets={('127.0.0.1', 40000, 123)},
                        activity={('127.0.0.1', 40000, 'role')})
        defaults.update(changes)
        from contextlib import ExitStack
        stack = ExitStack()
        for name, value in defaults.items():
            if isinstance(value, Exception):
                stack.enter_context(patch.object(binding, name, side_effect=value))
            elif isinstance(value, list):
                stack.enter_context(patch.object(binding, name, side_effect=value))
            else:
                stack.enter_context(patch.object(binding, name, return_value=value))
        stack.enter_context(patch.object(binding.os, 'stat', return_value=type('Stat', (), {'st_ino': 7})()))
        with stack:
            report = binding.report()
        self.assertNotIn(SECRET, json.dumps(report))
        self.assertEqual(set(report), {'format', 'executable', 'configured_target', 'observed_connection',
                                      'process_stable', 'startup_configuration_proven', 'error_stage', 'failure_kind', 'permission_limited', 'recovery', 'installation_or_activation_authorized'})
        return report

    def test_good_correlated(self):
        self.assertEqual(self.good()['observed_connection'], 'pid_socket_and_postgres_metadata_correlated')

    def test_restart_invalidates_every_proof(self):
        report = self.good(service=[42, 43])
        self.assertFalse(report['process_stable'])
        self.assertEqual(report['configured_target'], 'unknown')
        self.assertEqual(report['executable'], 'unknown')

    def test_start_time_change_invalidates(self):
        report = self.good(identity=[(b'100', (7, 8, 9, 10)), (b'101', (7, 8, 9, 10))])
        self.assertEqual(report['observed_connection'], 'unknown')

    def test_other_namespace(self):
        self.assertFalse(self.good(identity=(b'100', (999, 8, 9, 10)))['process_stable'])

    def test_socket_churn(self):
        report = self.good(sockets=[{('127.0.0.1', 40000, 123)}, {('127.0.0.1', 40001, 124)}])
        self.assertEqual(report['observed_connection'], 'unknown')
        self.assertEqual(report['executable'], 'matches_pinned_rc4')

    def test_wrong_role(self):
        self.assertEqual(self.good(activity={('127.0.0.1', 40000, SECRET)})['observed_connection'], 'unknown')

    def test_no_connections_is_unknown(self):
        self.assertEqual(self.good(sockets=set())['observed_connection'], 'unknown')

    def test_permission_failure_redacted(self):
        report = self.good(credential=PermissionError(SECRET))
        self.assertEqual(report['configured_target'], 'unknown')
        self.assertTrue(report['permission_limited'])
        self.assertEqual(report['error_stage'], 'configuration')
        self.assertEqual(report['failure_kind'], 'permission_limited')

    def test_secret_malformed_toml(self):
        self.assertEqual(self.good(credential=('[database]\nurl="' + SECRET).encode())['configured_target'], 'unknown')

    def test_secret_parser_failure(self):
        self.assertFalse(self.good(service=ValueError(SECRET))['process_stable'])

    def test_wrong_binary(self):
        self.assertEqual(self.good(binary_hash='0' * 64)['executable'], 'unknown')

    def test_url_inputs(self):
        for url in ('postgres://role:p@other:5433/memory_trust',
                    'postgres://role:p@127.0.0.1:5432/memory_trust',
                    'postgres://role:p@127.0.0.1:5433/other',
                    'postgres://role:p@127.0.0.1:5433/memory_trust?host=other'):
            with self.subTest(url=url), self.assertRaises(Exception):
                binding.config_target(('[database]\nurl="' + url + '"').encode())
        self.assertEqual(binding.config_target(b'[database]\nurl="postgres://role:p@[::1]:5433/memory_trust"'), ('role', '::1'))

    def test_overrides_and_profile_rejected_privately(self):
        argv = b'\0'.join(v.encode() for v in binding.ARGV) + b'\0'
        for env in (b'MEMORY_LAYER_PROFILE=dev\0',
                    b'MEMORY_LAYER_PROFILE=prod\0MEMORY_LAYER__DATABASE__URL=' + SECRET.encode() + b'\0',
                    b'MEMORY_LAYER_PROFILE=prod\0memory_layer__database__url=x\0'):
            with patch.object(binding, 'read', side_effect=[argv, env]), self.assertRaises(binding.Unknown):
                binding.loader_inputs(42)

    def test_nonstandard_argv(self):
        with patch.object(binding, 'read', return_value=b'private\0'), self.assertRaises(binding.Unknown):
            binding.loader_inputs(42)

    def test_safe_open_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'target'
            target.write_text(SECRET)
            link = Path(directory) / 'link'
            link.symlink_to(target)
            with self.assertRaises(OSError):
                binding.read(str(link))

    def test_credential_walk_uses_nofollow_and_checks_sibling(self):
        opened = []
        fake = type('Stat', (), {'st_uid': 0, 'st_mode': 0o40755})()
        def open_file(path, flags, **kwargs):
            opened.append((path, flags, kwargs))
            return len(opened)
        with patch.object(binding.os, 'open', side_effect=open_file), \
             patch.object(binding.os, 'close'), patch.object(binding.os, 'fstat', return_value=fake), \
             patch.object(binding.os, 'stat', return_value=fake), self.assertRaises(binding.Unknown):
            binding.credential(42)
        self.assertEqual(opened[0][0], '/proc/42/root')
        self.assertTrue(all(flags & os.O_NOFOLLOW for _, flags, _ in opened[1:]))

    def test_run_output_cap_and_timeout(self):
        # Local synthetic subprocesses only; inherited settings cannot supply code.
        with self.assertRaises(binding.Unknown):
            binding.run(['/usr/bin/python3', '-I', '-S', '-c', 'print("x" * 140000)'])
        with patch.object(binding.time, 'monotonic', side_effect=[0, 7]), self.assertRaises(binding.Unknown):
            binding.run(['/usr/bin/python3', '-I', '-S', '-c', 'import time; time.sleep(10)'])

    def test_sql_uses_host_without_inet_mask(self):
        with patch.object(binding, 'run', return_value=b'[{"address":"127.0.0.1","port":40000,"role":"role"}]') as command:
            self.assertEqual(binding.activity(), {('127.0.0.1', 40000, 'role')})
        sql = command.call_args.args[0][-1]
        self.assertIn('pg_catalog.host(client_addr)', sql)
        self.assertNotIn('client_addr::text', sql)

    def test_missing_tomllib_redacted(self):
        with patch.object(binding, 'tomllib', None):
            self.assertEqual(binding.report()['configured_target'], 'unknown')

    def test_exact_credential_service_uid_allowed(self):
        info = type('Stat', (), {'st_uid': 123, 'st_mode': 0o100400})()
        binding.trusted(info, owners=(0, 123))
        with self.assertRaises(binding.Unknown):
            binding.trusted(info)

    def test_socket_parser_exact_target_and_inode(self):
        def row(address, inode=123, port='0FA0', remote='0100007F'):
            return f'0: {address}:{port} {remote}:1539 01 0 0 0 0 0 {inode} 0'
        tcp = ('header\n' + row('0100007F') + '\n' + row('0100007F', inode=999) + '\n' +
               row('0100007F', port='0FA1', remote='0200007F') + '\n').encode()
        entry = type('Entry', (), {'path': '/synthetic/fd/1'})()
        from contextlib import nullcontext
        with patch.object(binding.os, 'scandir', return_value=nullcontext([entry])), \
             patch.object(binding.os, 'readlink', return_value='socket:[123]'), \
             patch.object(binding, 'read', side_effect=[tcp, b'header\n']):
            self.assertEqual(binding.sockets(42, '127.0.0.1'), {('127.0.0.1', 4000, 123)})
        ipv6 = b'header\n0: 00000000000000000000000001000000:0FA0 00000000000000000000000001000000:1539 01 0 0 0 0 0 123 0\n'
        with patch.object(binding.os, 'scandir', return_value=nullcontext([entry])), \
             patch.object(binding.os, 'readlink', return_value='socket:[123]'), \
             patch.object(binding, 'read', side_effect=[b'header\n', ipv6]):
            self.assertEqual(binding.sockets(42, '::1'), {('::1', 4000, 123)})

    def test_bounded_file(self):
        with tempfile.TemporaryFile() as stream:
            stream.write(b'x' * 101)
            stream.seek(0)
            with self.assertRaises(binding.Unknown):
                binding.read_fd(stream.fileno(), 100)


if __name__ == '__main__':
    unittest.main()
