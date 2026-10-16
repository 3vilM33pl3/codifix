# SPDX-License-Identifier: AGPL-3.0-or-later
import json
import hashlib
from pathlib import Path
import unittest
from urllib.parse import urlsplit


class BuildToolsManifestTests(unittest.TestCase):
    def test_pds_stdio_proof_keeps_native_startup_separate_from_repository_acceptance(self):
        script = (Path(__file__).parent / 'guest-pds-stdio.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'",'Native PDS runtime inventory changed',
                         'Native PDS runtime bytes changed','EnvironmentVariables.Clear()',
                         "PDS_DEV_MODE='false'", "PDS_DISABLE_SSRF_PROTECTION='false'",
                         'WaitForExit(90000)','signed_repository_verified=$false','pipe_acl_verified=$false',
                         'tcp_listener_created=$false','probe-stdio.mjs']:
            self.assertIn(required,script)
        self.assertNotIn('PDS_PORT',script)
        self.assertNotIn('Invoke-WebRequest',script)
        self.assertNotIn('Write-Output $stderr',script)

    def test_pi_stage_does_not_reassign_powershell_read_only_home(self):
        script = (Path(__file__).parent / 'guest-native-pi.ps1').read_text()
        self.assertNotRegex(script, r'(?i)\$home\s*=')
        self.assertIn('USERPROFILE=$privateProfile; HOME=$privateProfile',script)
        self.assertIn("PSExecutionPolicyPreference='Bypass'",script)
        self.assertNotIn('Set-ExecutionPolicy',script)
        self.assertIn("CARGO_BUILD_JOBS='2'",script)

    def test_pi_source_alias_is_materialized_without_link_privilege_or_source_repin(self):
        root=Path(__file__).resolve().parents[2]
        script=(root/'crates/mem-supervisor-pi/scripts/build-windows.ps1').read_text()
        for required in ["--exclude=.beads/beads.jsonl",'120000 blob',"${Revision}:.beads/beads.jsonl",
                         "'issues.jsonl'",'source_link_materialization=',"policy='git-core-symlinks-false'"]:
            self.assertIn(required,script)
        self.assertNotIn('New-Item -ItemType SymbolicLink',script)
        self.assertIn("'--profile','minimal','--no-self-update'",script)

    def test_native_build_families_are_separate_but_each_is_serialized(self):
        script = (Path(__file__).parent / 'guest-stage.ps1').read_text()
        for required in ["'PiBuild' { 'Local\\MemoryLab-NativePiBuild' }",
                         "'ProductCheck' { 'Local\\MemoryLab-NativeProductBuild' }",
                         "default { 'Local\\MemoryLab-NativeBuildStage' }",'$mutex.WaitOne(0)']:
            self.assertIn(required,script)

    def test_product_compilation_is_bounded_native_and_not_workflow_evidence(self):
        script = (Path(__file__).parent / 'guest-native-product.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'",'x86_64-pc-windows-msvc',
                         'rev-parse HEAD','--untracked-files=no','WaitForExit(7200000)',
                         "'check','--workspace','--all-targets','--locked'",'workflow_verified=$false',
                         'RedirectStandardOutput','RedirectStandardError','Build changed the frozen Cargo.lock']:
            self.assertIn(required,script)
        self.assertNotIn('publish',script)
        self.assertNotIn('Invoke-Expression',script)

    def test_optional_lab_ssh_is_pinned_key_only_and_never_replaces_service(self):
        script = (Path(__file__).parent / 'guest-ssh.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", 'Get-FileHash',
                         'Get-AuthenticodeSignature', "Get-Service sshd", 'Get-Process dismhost',
                         'PasswordAuthentication no', 'KbdInteractiveAuthentication no',
                         'Existing SSH key differs', "-RemoteAddress '10.77.0.2'"]:
            self.assertIn(required, script)
        self.assertIn("-DisplayName 'Memory lab key-only SSH from private host'", script)
        self.assertNotIn('Invoke-WebRequest', script)
        self.assertNotIn('Add-WindowsCapability', script)

    def test_native_onboarding_compiles_the_settings_feature_it_tests(self):
        script = (Path(__file__).resolve().parents[2] / 'packaging/windows/check-onboarding.ps1').read_text()
        self.assertEqual(script.count('--features project-sync'), 2)

    def test_receipt_replacement_uses_native_replace_not_move_force(self):
        for name in ['guest-stage.ps1', 'guest-native-api.ps1', 'guest-build-tools.ps1']:
            script = (Path(__file__).parent / name).read_text()
            self.assertIn('[IO.File]::Replace(', script)
            self.assertIn('[IO.File]::Move(', script)
            self.assertIn('[NullString]::Value', script)

    def test_icacls_receives_explicit_numeric_sid_syntax(self):
        for name in ['guest-stage.ps1', 'guest-native-api.ps1',
                     'guest-native-postgres.ps1', 'guest-native-pds.ps1']:
            script = (Path(__file__).parent / name).read_text()
            self.assertIn('"*${sid}:(OI)(CI)F"', script)
            self.assertNotIn('"${sid}:(OI)(CI)F"', script)

    def test_native_api_stage_is_hash_bound_interactive_and_bounded(self):
        root = Path(__file__).parent
        stage = (root / 'guest-stage.ps1').read_text()
        self.assertIn("'NativeApi'", stage)
        self.assertIn('Get-BoundFiles $Stage', stage)
        self.assertIn('Queued native proof file inventory changed', stage)
        self.assertIn('New-TimeSpan -Minutes 20', stage)
        self.assertIn('-LogonType Interactive', stage)
        self.assertIn("$runLevel = if ($Stage -in @('NativeApi','PiBuild','ProductCheck','PdsVerify')) { 'Limited' } else { 'Highest' }",stage)
        self.assertIn('-LogonType Interactive -RunLevel $runLevel',stage)
        script = (root / 'guest-native-api.ps1').read_text()
        for required in ['memory-windows-native-api-v1', 'Get-FileHash',
                         'Native API filter selected zero tests', 'test result: ok',
                         'WaitForExit($Seconds * 1000)', '/PID $process.Id /T /F',
                         'execution_verified=$false', 'credentials::windows::tests',
                         'workspace::filesystem::windows::tests', 'agents::settings::filesystem::tests',
                         'runtime_env_windows::tests',
                         'pds_pipe::native::tests',
                         'collector::codex::windows::native::tests']:
            self.assertIn(required, script)
        self.assertNotIn('Invoke-Expression', script)
        self.assertNotIn('[string]$Command', script)
        self.assertIn("'zz-isolation'", script)
        self.assertIn("failed_filters=@()", script)
        self.assertIn("$receipt.failed_filters += $label", script)
        self.assertIn("$receipt.failed_filters += 'isolation-probe'", script)
        self.assertLess(script.index('if ($receipt.failed_filters.Count)'),
                        script.index('$receipt.execution_verified = $true'))

    def test_bootstrap_prepares_ssh_state_before_writing_keys(self):
        script = (Path(__file__).parent / 'guest-bootstrap.ps1').read_text()
        self.assertLess(script.index("New-Item -ItemType Directory -Force 'C:\\ProgramData\\ssh'"),
                        script.index('Copy-Item "$seed\\authorized_keys"'))
        self.assertIn("Get-Service -Name 'QEMU-GA'", script)
        self.assertIn('guest-agent-install.log', script)
        self.assertIn('Write-Error -ErrorRecord $_ -ErrorAction Continue', script)
        self.assertIn('$driverExit -notin @(0, 3010, 259)', script)
        self.assertIn('Where-Object ConfigManagerErrorCode -eq 0', script)
        self.assertIn("$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", script)

    def test_guest_observation_is_lab_scoped_and_metadata_only(self):
        script = (Path(__file__).parent / 'guest-observe.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", 'Confirm-SecureBootUEFI',
                         'Get-Tpm', 'grace_remaining_minutes', 'Get-TimeZone']:
            self.assertIn(required, script)
        for forbidden in ['Set-ItemProperty', 'Get-ChildItem Env:', 'DigitalProductId',
                          'ConvertFrom-SecureString', 'Start-Process']:
            self.assertNotIn(forbidden, script)

    def test_stage_runner_has_fixed_tasks_and_no_password_registration(self):
        script = (Path(__file__).parent / 'guest-stage.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", '-LogonType Interactive',
                         'runner_sha256', 'Queued native proof input changed',
                         'task_ended_without_completion_receipt', 'previous_execution_interrupted']:
            self.assertIn(required, script)
        self.assertNotIn('-Password', script)
        self.assertNotIn('Invoke-Expression', script)

    def test_native_pds_retains_upstream_lock_and_separates_evidence(self):
        root = Path(__file__).parent
        manifest = json.loads((root / 'native-pds.json').read_text())
        upstream = root.parents[1] / 'packaging/windows-pds/upstream'
        self.assertEqual(manifest['format'], 'memory-windows-pds-proof-v1')
        self.assertEqual(manifest['node_version'], '24.18.1')
        self.assertEqual(manifest['pds_version'], '0.5.27')
        for name, key in [('package.json', 'package_sha256'), ('pnpm-lock.yaml', 'lock_sha256')]:
            self.assertEqual(hashlib.sha256((upstream / name).read_bytes()).hexdigest(), manifest[key])
        package = json.loads((upstream / 'package.json').read_text())
        self.assertEqual(package['packageManager'], 'pnpm@' + manifest['pnpm']['version'])
        self.assertEqual(package['pnpm']['onlyBuiltDependencies'], ['better-sqlite3'])
        self.assertEqual(len(manifest['pnpm']['sha512']), 128)
        script = (root / 'guest-native-pds.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", '--frozen-lockfile',
                         '--ignore-scripts', "'rebuild','better-sqlite3'", 'pds_protocol_verified=$false']:
            self.assertIn(required, script)

    def test_native_postgres_inputs_and_lab_boundary(self):
        root = Path(__file__).parent
        manifest = json.loads((root / 'native-postgres.json').read_text())
        self.assertEqual(manifest['format'], 'memory-windows-postgres-proof-v1')
        self.assertEqual(manifest['postgres_version'], '17.11')
        self.assertEqual(manifest['pgvector_version'], '0.8.7')
        for kind, host in [('postgres', 'get.enterprisedb.com'), ('pgvector', 'codeload.github.com')]:
            artifact = manifest[kind]
            url = urlsplit(artifact['url'])
            self.assertEqual(url.scheme, 'https')
            self.assertEqual(url.hostname, host)
            self.assertIsNone(url.username)
            self.assertIsNone(url.password)
            self.assertFalse(url.query)
            self.assertEqual(Path(artifact['file']).name, artifact['file'])
            self.assertEqual(len(artifact['sha256']), 64)
            self.assertTrue(all(c in '0123456789abcdef' for c in artifact['sha256']))
        script = (root / 'guest-native-postgres.ps1').read_text()
        self.assertIn("$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", script)
        self.assertIn("-h 127.0.0.1 -p 55471", script)
        self.assertIn("--auth-host=scram-sha-256", script)
        self.assertIn("Prepared native runtime does not match", script)
        for required in ['VCToolsRedistDir', 'Microsoft.VC143.CRT', 'runtime_support=$support',
                         'clean_runtime_verified=$false', 'Prepared native extension inventory is incomplete',
                         'Prepared native support DLL changed after verification']:
            self.assertIn(required, script)
        self.assertLess(script.index("Copy-Item -LiteralPath $file.FullName"),
                        script.index("Run-Checked (Join-Path $bin 'postgres.exe')"))
        self.assertNotIn('--trust', script)

    def test_exact_tool_set_has_bounded_verified_inputs(self):
        manifest = json.loads((Path(__file__).parent / 'build-tools.json').read_text())
        self.assertEqual(manifest['format'], 'memory-windows-build-tools-v1')
        self.assertEqual(manifest['rust_toolchain'], '1.99.1')
        self.assertEqual(manifest['wix_version'], '4.0.5')
        artifacts = manifest['artifacts']
        self.assertEqual(sorted(a['id'] for a in artifacts),
                         ['dotnet', 'git', 'msvc', 'node', 'python', 'rustup'])
        for artifact in artifacts:
            location = urlsplit(artifact['url'])
            self.assertEqual(location.scheme, 'https')
            self.assertIsNone(location.username)
            self.assertIsNone(location.password)
            self.assertFalse(location.query)
            self.assertEqual(Path(artifact['file']).name, artifact['file'])
            size = {'SHA256': 64, 'SHA512': 128}[artifact['algorithm']]
            self.assertEqual(len(artifact['hash']), size)
            self.assertTrue(all(c in '0123456789abcdef' for c in artifact['hash']))
        self.assertTrue(next(a for a in artifacts if a['id'] == 'msvc')['microsoft_signature'])

    def test_tool_receipts_require_successful_version_checks(self):
        script = (Path(__file__).parent / 'guest-build-tools.ps1').read_text()
        self.assertIn('function Read-Version', script)
        self.assertIn('$LASTEXITCODE -ne 0 -or $output.Count -ne 1', script)
        self.assertIn('MSVC installation did not provide the required native toolchain', script)
        self.assertIn('Installed tool version differs from the reviewed build baseline', script)
        for tool in ['rust', 'cargo', 'node', 'git', 'python', 'dotnet', 'wix']:
            self.assertIn('$receipt.' + tool + ' = Read-Version', script)


if __name__ == '__main__':
    unittest.main()
