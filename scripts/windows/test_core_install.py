# SPDX-License-Identifier: AGPL-3.0-or-later
"""Portable contract checks only: never operates a guest, SCM or database."""
from pathlib import Path
import shutil
import subprocess
import unittest


SCRIPT = Path(__file__).with_name("guest-core-install.ps1")


class CoreInstallTests(unittest.TestCase):
    def test_exact_lab_fake_only_and_preserved_evidence(self):
        source = SCRIPT.read_text()
        for required in ["'MEMORY-WINLAB'", "'AMD64'", "'MEMORY-WINLAB\\memorylab'",
                         "WindowsBuiltInRole]::Administrator", "'GEMINI_API_KEY'", "'DATABASE_URL'",
                         "'Exact provider-disabled completed core configuration required'",
                         "'llm_curation = false'", "'create_enabled = false'",
                         "[IO.FileMode]::CreateNew", "ReparsePoint", "EnvironmentVariables.Clear()",
                         "no automatic repeat", "real_provider_calls=0", "failure_data_runtime_preserved=$true",
                         "reboot_verified=$false", "upgrade_verified=$false", "sync_verified=$false"]:
            self.assertIn(required, source)
        for forbidden in ["Remove-Item", "DROP DATABASE", "CREATE DATABASE", "ConvertTo-SecureString",
                          "test-password.dpapi", "Stop-Service", "Unregister-ScheduledTask", "taskkill",
                          "cargo.exe", "git checkout", "Invoke-Expression", "Invoke-WebRequest"]:
            self.assertNotIn(forbidden, source)
        self.assertLess(source.index("if($FunctionsOnly){return}"), source.index("$env:OS"))

    def test_real_installer_preview_digest_and_separate_build_provenance(self):
        source = SCRIPT.read_text()
        for required in ["Get-WindowsReleaseIdentity '2.1.0-rc.5' 60000", "lab_only=$true;release_candidate=$false",
                         "installer_source_commit=$head;binary_source_commit=$build.source_commit",
                         "'native-instance.ps1'", "'-Action','Preview','-PreviewAction','Install'",
                         "'-Action','Install','-ExpectedInputSha256',$preview.input_sha256",
                         "'-DatabaseCompatibilityConfirmed'", "schema.Trim() -cne '84:0'",
                         "Get-CoreInstallExecutableSource $file $coreRoot $debug", "$pin[0].sha256 -cne $core.binary_sha256",
                         "payload_files=$payloadFiles", "package_sha256=Get-CoreInstallHash $package",
                         "'LAB-ONLY-manifest.json'", "'unsigned-lab-debug'", "database_reused=$true"]:
            self.assertIn(required, source)
        self.assertNotIn("$build.source_commit -cne $head", source)
        self.assertNotIn("New-Service", source, "Installation must remain delegated to the authority script")
        self.assertLess(source.index("'schema-read'"), source.index("'-Action','Preview'"))
        self.assertLess(source.index("'-Action','Preview'"), source.index("'-Action','Install'"))

    def test_fresh_install_attempt_retains_core_and_uses_immutable_memory(self):
        source = SCRIPT.read_text()
        for required in ["[guid]$InstallOperationId", "$InstallOperationId -eq [guid]::Empty",
                         "$InstallOperationId -eq $OperationId", r"runs\$InstallOperationId\core-install",
                         r"runs\$OperationId\core-workflow", "core_operation_id=$OperationId.ToString()",
                         "operation_id=$InstallOperationId.ToString()", "'-OperationId',$InstallOperationId.ToString()",
                         "return Join-Path $CoreRoot 'code\\memory.exe'", "return Join-Path $DebugRoot $File",
                         "Copy-CoreInstallFile $from (Join-Path $payload ('bin\\'+$file)) $pin[0].sha256 $file",
                         "Pinned core installation input unavailable or changed: '+$Component"]:
            self.assertIn(required, source)
        self.assertNotIn("Copy-CoreInstallFile (Join-Path $debug $file)", source)

    def test_sanitized_installer_environment_preserves_fixed_windows_identity(self):
        source = SCRIPT.read_text()
        clear = source.index("$info.EnvironmentVariables.Clear()")
        start = source.index("$null=$process.Start()", clear)
        environment = source[clear:start]
        for key, value in [("OS", "Windows_NT"), ("COMPUTERNAME", "MEMORY-WINLAB"),
                           ("PROCESSOR_ARCHITECTURE", "AMD64")]:
            self.assertIn(f"$info.EnvironmentVariables['{key}']='{value}'", environment)
            self.assertNotIn(f"$info.EnvironmentVariables['{key}']=$env:", environment)
        self.assertIn("$info.EnvironmentVariables['SystemRoot']=$env:SystemRoot", environment)
        self.assertIn("$info.EnvironmentVariables['PATH']=$env:SystemRoot+'\\System32'", environment)
        self.assertNotIn("GetEnvironmentVariables", environment)
        self.assertNotIn("API_KEY", environment)

    def test_known_folders_are_explicit_and_private_diagnostics_exclude_bodies(self):
        source = SCRIPT.read_text()
        for name in ["ProgramData='CommonApplicationData'", "ALLUSERSPROFILE='CommonApplicationData'", "ProgramFiles='ProgramFiles'",
                     "'ProgramFiles(x86)'='ProgramFilesX86'", "CommonProgramFiles='CommonProgramFiles'",
                     "'CommonProgramFiles(x86)'='CommonProgramFilesX86'"]:
            self.assertIn(name, source)
        for required in ["[Environment]::GetFolderPath($folder)", "[IO.Path]::IsPathRooted($path)",
                         "Required Windows known folder unavailable:", "$info.EnvironmentVariables[$key]=$knownFolders[$key]",
                         "($_.InvocationInfo.PositionMessage,$_.ScriptStackTrace)", "$Label+'.failure.log'",
                         "New-CoreInstallerDiagnosticCommand $Arguments", "[IO.FileMode]::CreateNew"]:
            self.assertIn(required, source)
        diagnostic = source.split("$catch=@'", 1)[1].split("'@", 1)[0]
        for forbidden in ["Exception.Message", "ConfigPath", "Get-Content", "request", "Body"]:
            self.assertNotIn(forbidden, diagnostic)

    def test_running_exact_virtual_identity_and_actual_http_proofs(self):
        source = SCRIPT.read_text()
        for required in ["Get-CimInstance Win32_Service", "$scm.StartName -ine $account", "GetOwnerSid",
                         "$owner.Sid -cne $serviceSid", "$childOwner.Sid -cne $serviceSid",
                         "'bin\\memory.exe'", "UseProxy=$false", "AllowAutoRedirect=$false",
                         "Test-NativeReadinessHealth $health $instance", "$htmlHash -cne $receipt.web_index_sha256",
                         "'/v1/query'", "retrieval_mode='lexical';answer_mode='deterministic'",
                         "'/v1/projects/native-smoke/memories'", "$_.id -ceq $core.curated_memory_id"]:
            self.assertIn(required, source)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell unavailable; native/parser proof remains unclaimed")
    def test_powershell_parser_and_pure_helpers(self):
        subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File",
                        str(SCRIPT.with_name("test-core-install.ps1"))], check=True)


if __name__ == "__main__":
    unittest.main()
