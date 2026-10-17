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
                         "Copy-CoreInstallFile (Join-Path $debug $file)", "$pin[0].sha256 -cne $core.binary_sha256",
                         "payload_files=$payloadFiles", "package_sha256=Get-CoreInstallHash $package",
                         "'LAB-ONLY-manifest.json'", "'unsigned-lab-debug'", "database_reused=$true"]:
            self.assertIn(required, source)
        self.assertNotIn("$build.source_commit -cne $head", source)
        self.assertNotIn("New-Service", source, "Installation must remain delegated to the authority script")
        self.assertLess(source.index("'schema-read'"), source.index("'-Action','Preview'"))
        self.assertLess(source.index("'-Action','Preview'"), source.index("'-Action','Install'"))

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
