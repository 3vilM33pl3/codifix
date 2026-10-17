# SPDX-License-Identifier: AGPL-3.0-or-later
"""Portable contract checks, not native SCM/application acceptance."""
import os
from pathlib import Path
import shutil
import subprocess
import unittest


SCRIPT = Path(__file__).with_name("guest-installed-core-check.ps1")
PWSH = os.environ.get("MEMORY_TEST_PWSH") or shutil.which("pwsh")


class InstalledCoreCheckTests(unittest.TestCase):
    def test_exact_lab_and_forwarded_function_only_inputs(self):
        source = SCRIPT.read_text()
        for required in ["'Windows_NT'", "'MEMORY-WINLAB'",
                         "$InstallOperationId -eq [guid]::Empty",
                         "$SourceRoot -cnotmatch '^C:\\\\MemoryLab\\\\source",
                         "$SourceRoot.Split('\\') -contains '..'",
                         "-FunctionsOnly -InstallOperationId $InstallOperationId -SourceRoot $SourceRoot"]:
            self.assertIn(required, source)
        self.assertLess(source.index("Exact disposable lab installation required"),
                        source.index("-FunctionsOnly"))

    def test_original_core_config_and_binary_are_bound_before_http(self):
        source = SCRIPT.read_text()
        for required in ["$old.format -cne 'memory-native-core-install-v1'",
                         "$old.operation_id -cne $InstallOperationId.ToString()",
                         "$old.lab_only -ne $true", "$old.release_candidate -ne $false",
                         "$old.real_provider_calls -ne 0",
                         "$old.installer_build -lt 60000", "$old.installer_build -gt 60999",
                         "(Get-CoreInstallHash $corePath) -cne $old.core_receipt_sha256",
                         "$journal.phase -cne 'completed'",
                         "$journal.input_sha256 -cne $old.preview_input_sha256",
                         "$journal.operation_id -cne $InstallOperationId.ToString()",
                         "(Get-CoreInstallHash $config) -cne $old.config_sha256",
                         '(Get-CoreInstallHash "$release\\bin\\memory.exe") -cne $core.binary_sha256',
                         "Get-CoreInstallBinding ([IO.File]::ReadAllText($config))",
                         "original_receipt_sha256=(Get-CoreInstallHash $oldPath)"]:
            self.assertIn(required, source)
        self.assertLess(source.index("Get-CoreInstallBinding ([IO.File]::ReadAllText($config))"),
                        source.index("$client=[Net.Http.HttpClient]::new($handler)"))
        binding = SCRIPT.with_name("guest-core-install.ps1").read_text()
        self.assertIn("'Exact provider-disabled completed core configuration required'", binding)
        self.assertIn("'llm_curation = false'", binding)
        self.assertIn("'create_enabled = false'", binding)

    def test_recovery_appends_separate_evidence_and_restarts_only_exact_service(self):
        source = SCRIPT.read_text()
        for required in ["'MemoryLayer-lab-core-'+$InstallOperationId.ToString('N').Substring(0,16)",
                         "$old.service_name -cne $name", "$preview.plan.service_name -cne $name",
                         "Restart-Service -Name $name", "$scm.StartName -ine $account",
                         "$owner.Sid -cne $sid", "$children.Count -ne 1",
                         "'installed-verification.json'", "Preserve existing installed verification evidence",
                         "Write-CoreInstallNew $proofPath", "real_provider_calls=0",
                         "reboot_verified=$false;upgrade_verified=$false;sync_verified=$false",
                         "'/v1/projects/native-smoke/memories'", "retrieval_mode='lexical'",
                         "answer_mode='deterministic'", "$hash -cne $old.web_index_sha256"]:
            self.assertIn(required, source)
        self.assertEqual(source.count("Restart-Service"), 1)
        for forbidden in ["Write-CoreInstallNew $oldPath", "Set-Content", "WriteAllText",
                          "Remove-Item", "Stop-Service", "New-Service", "Start-Service",
                          "-Action Install", "-Action Upgrade", "CREATE DATABASE", "DROP DATABASE",
                          "cargo", "Invoke-WebRequest", "initdb", "migrate", "Compress-Archive"]:
            # The header accurately says it does not migrate; inspect executable
            # lines rather than rejecting the explanatory comment.
            code = "\n".join(line for line in source.splitlines()
                             if not line.lstrip().startswith("#"))
            self.assertNotIn(forbidden, code)

    @unittest.skipUnless(PWSH, "PowerShell parser unavailable")
    def test_powershell_syntax_without_execution(self):
        command = ("$errors=$null; [Management.Automation.Language.Parser]::ParseFile("
                   "$env:MEMORY_PS_PARSE_TARGET,[ref]$null,[ref]$errors) | Out-Null; "
                   "if($errors){$errors | Out-String | Write-Error; exit 1}")
        environment = dict(os.environ, MEMORY_PS_PARSE_TARGET=str(SCRIPT))
        result = subprocess.run([PWSH, "-NoProfile", "-Command", command],
                                text=True, capture_output=True, timeout=30, env=environment)
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
