# SPDX-License-Identifier: AGPL-3.0-or-later
import json
import hashlib
from pathlib import Path
import unittest
from urllib.parse import urlsplit


class BuildToolsManifestTests(unittest.TestCase):
    def test_repository_rerun_selects_only_the_native_database_repository_target(self):
        script=(Path(__file__).parent/'guest-native-db.ps1').read_text()
        self.assertIn("'Repository' {'native-repository-db'}",script)
        self.assertIn("$receipt.test_target='db_repository'",script)
        branch=script.split("elseif ($Scope -eq 'Repository')",1)[1].split("elseif",1)[0]
        self.assertIn("'-p','codifix-service','--test','db_repository'",branch)
        self.assertNotIn("'--workspace'",branch)

    def test_portability_rerun_is_exactly_four_packages_not_workspace(self):
        script=(Path(__file__).parent/'guest-native-db.ps1').read_text()
        self.assertIn("'Portability' {'native-portability-db'}",script)
        self.assertIn("$receipt.packages=@('codifix-cli','codifix-config','codifix-eval','codifix-federation-runtime')",script)
        self.assertIn("$receipt.whole_workspace_verified=$false",script)
        branch=script.split("elseif ($Scope -eq 'Portability')",1)[1].split("elseif",1)[0]
        for required in ["'test'", "'-p','codifix-cli'", "'-p','codifix-config'", "'-p','codifix-eval'",
                         "'-p','codifix-federation-runtime'", "'--all-targets'", "'--locked'",
                         "'--no-fail-fast'", "'--test-threads=1'"]:
            self.assertIn(required,branch)
        self.assertNotIn("'--workspace'",branch)
        self.assertNotIn("'--ignored'",branch)

    def test_native_database_tests_use_only_the_existing_disposable_cluster(self):
        script=(Path(__file__).parent/'guest-native-db.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", "'test-password.dpapi'",
                         "$verified.production -ne $false", "'--all-targets'", "'--test-threads=1'",
                         "$env:CODIFIX_TEST_REQUIRE_DB='1'", 'ZeroFreeBSTR',
                         'WaitForExit(7200000)', 'workflow_verified=$false']:
            self.assertIn(required,script)
        self.assertNotIn('Invoke-WebRequest',script)
        self.assertNotIn('pg_ctl',script)
        self.assertNotIn('initdb',script)

    def test_native_onboarding_task_proof_never_runs_a_sync_or_adopts_a_task(self):
        script=(Path(__file__).parent/'guest-onboarding-task.ps1').read_text()
        for required in ['Get-OnboardingPlan -Stage EnableSync', 'Assert-OnboardingTask',
                         '-LogonType Interactive -RunLevel Limited', 'Get-OnboardingTaskPolicy',
                         'Foreign task exists; no adoption', 'Unregister-ScheduledTask',
                         'task_started=$false', 'onboarding_verified=$false',
                         '$current.Actions[0].Arguments -cne $arguments']:
            self.assertIn(required,script)
        self.assertNotIn('Start-ScheduledTask',script)
        self.assertNotIn('Start-Service',script)
        self.assertNotIn('Invoke-Expression',script)

    def test_lab_ancestor_is_private_without_recursive_or_host_permission_changes(self):
        root = Path(__file__).parent
        for name in ['guest-bootstrap.ps1','guest-protect-lab.ps1']:
            script = (root / name).read_text()
            self.assertIn("Get-LocalUser -Name 'memorylab'", script)
            self.assertIn('/inheritance:r /grant:r', script)
        repair = (root / 'guest-protect-lab.ps1').read_text()
        for required in ['ancestor-acl-', 'AreAccessRulesProtected', 'Unexpected lab ancestor principal', 'ReparsePoint']:
            self.assertIn(required, repair)
        self.assertNotIn('/T', repair)
        self.assertNotIn('Get-ChildItem', repair)

    def test_native_evaluation_proof_keeps_private_labels_and_paid_campaign_separate(self):
        root = Path(__file__).parent
        script = (root / 'guest-native-eval.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", 'SessionId -eq 0',
                         'rev-parse HEAD', '--untracked-files=no', 'WaitForExit(180000)',
                         '/PID $process.Id /T /F', 'provider_requests -ne 0', 'official_evidence -ne $false']:
            self.assertIn(required, script)
        driver = (root / 'guest-evaluation-proof.mjs').read_text()
        for required in ['runOfflineExecution', 'createKnowledgeRunner', 'nativeProfileHash',
                         "assert.equal(negative.assessment.citations_valid, false)",
                         'human_judgments_unresolved: true', 'official_evidence: false',
                         'rust_grading_verified: false', 'campaign_verified: false',
                         "['EACCES','EPERM'].includes(error.code)", "Object.keys(input).sort().join(',')!=='context,question'"]:
            self.assertIn(required, driver)
        self.assertNotIn('createNativeWindowsExecutor({', driver)
        self.assertNotIn('dispatchModel', driver)
        self.assertNotIn('fetch(', driver)

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
        script=(root/'crates/codifix-supervisor-pi/scripts/build-windows.ps1').read_text()
        for required in ["--exclude=.beads/beads.jsonl",'120000 blob',"${Revision}:.beads/beads.jsonl",
                         "'issues.jsonl'",'source_link_materialization=',"policy='git-core-symlinks-false'"]:
            self.assertIn(required,script)
        self.assertNotIn('New-Item -ItemType SymbolicLink',script)
        self.assertIn("'--profile','minimal','--no-self-update'",script)

    def test_native_build_families_are_separate_but_each_is_serialized(self):
        script = (Path(__file__).parent / 'guest-stage.ps1').read_text()
        for required in ["'PiBuild' { 'Local\\MemoryLab-NativePiBuild' }",
                         "{ $_ -in @('ProductCheck','ProductBuild','NativeDb','NativeServiceDb','NativeSupervisor','NativeFleet','NativeWorkspaceDb') } { 'Local\\MemoryLab-NativeProductBuild' }",
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
        self.assertIn("@('build','--workspace','--bins','--locked','--features'",script)
        self.assertIn('$receipt.release_candidate=$false',script)

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

    def test_native_web_build_uses_locked_dependencies_and_does_not_claim_browser_acceptance(self):
        script = (Path(__file__).parent / 'guest-native-product.ps1').read_text()
        for required in ["[ValidateSet('Check','Build','Web')]", "if ($Action -eq 'Web')",
                         "@('ci','--no-audit','--no-fund')", "@('test')", "@('run','build')",
                         'web_lock_sha256', 'Pinned native Node required', 'WaitForExit(1800000)',
                         'Web build changed frozen package lock', 'browser_workflow_verified=$false']:
            self.assertIn(required, script)
        self.assertNotIn('npm install', script)

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
        self.assertIn("$runLevel = if ($Stage -in @('NativeApi','PiBuild','ProductCheck','ProductBuild','NativeDb','NativeServiceDb','NativeSupervisor','NativeFleet','NativeWorkspaceDb','PdsVerify','NativeEval')) { 'Limited' } else { 'Highest' }",stage)
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
        self.assertEqual(set(manifest['security_patches']),
                         {Path(name).name for name in package['pnpm']['patchedDependencies'].values()})
        for name, digest in manifest['security_patches'].items():
            self.assertEqual(hashlib.sha256((upstream / 'patches' / name).read_bytes()).hexdigest(), digest)
        self.assertEqual(package['pnpm']['overrides']['key-encoder>elliptic'], '-')
        self.assertEqual(len(manifest['pnpm']['sha512']), 128)
        script = (root / 'guest-native-pds.ps1').read_text()
        for required in ["$env:COMPUTERNAME -ne 'MEMORY-WINLAB'", '--frozen-lockfile',
                         '--ignore-scripts', "'rebuild','better-sqlite3'", 'pds_protocol_verified=$false',
                         '$env:CODIFIX_PDS_TEST_ROOT = $runtime', 'security.test.mjs']:
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
