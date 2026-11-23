# SPDX-License-Identifier: AGPL-3.0-or-later
"""Portable recipe checks; no Windows guest, build, database or provider runs."""
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = Path(__file__).with_name("guest-native-eval-bootstrap.ps1")


class NativeEvalBootstrapTests(unittest.TestCase):
    def test_fixed_prepare_only_lab_identity_and_no_activation(self):
        script = SCRIPT.read_text()
        for required in ["[ValidateSet('PrepareOnly')]", "$env:COMPUTERNAME -ne 'MEMORY-WINLAB'",
                         "$env:PROCESSOR_ARCHITECTURE -ne 'AMD64'", "SessionId -eq 0",
                         "MEMORY-WINLAB\\memorylab", "--untracked-files=no", "'--offline'",
                         "Local\\MemoryLab-NativeProductBuild", "EnvironmentVariables.Clear()",
                         "postgresql://synthetic:unused@127.0.0.1:9/memory_r3_prepare",
                         "'--bootstrap-prepare'", "-OuterJobOwner", "WaitForExit($Seconds*1000)",
                         "campaign_verified=$false", "database_contacted=$false", "provider_requests=0"]:
            self.assertIn(required, script)
        for forbidden in ["Invoke-WebRequest", "Start-Service", "New-Service", "initdb", "pg_ctl",
                          "test-password.dpapi", "ConvertTo-SecureString", "--mediated", "--wire",
                          "Register-ScheduledTask", "Invoke-Expression", "Set-ExecutionPolicy"]:
            self.assertNotIn(forbidden, script)
        self.assertNotRegex(script, r"(?i)\$(?:home|codex_home)\s*=")

    def test_new_evidence_preserves_and_rechecks_original_native_rust_profile(self):
        script = SCRIPT.read_text()
        for required in ["$RustProofOperationId", "positive_compilation_verified -ne $true",
                         "wrong_answer_rejected -ne $true", "broken_source_rejected -ne $true",
                         "Assert-EvalInventory $oldProfile.toolchains[0].root",
                         "old_profile_sha256", "old_result_sha256", "[IO.FileMode]::CreateNew",
                         "[IO.File]::Replace(", "ReparsePoint", "Get-EvalInventory $source",
                         "descriptor_sha256", "pack_manifest_sha256", "pack_runtime_sha256",
                         "launcher.outer_job_reaped -ne $true", "launcher.execution_started -ne $false"]:
            self.assertIn(required, script)
        self.assertNotIn("Copy-Item", script)
        self.assertNotRegex(script, r"(?:Write-Eval\w*|Set-Content).*\$old(?:Profile|Result)Path")
        self.assertEqual(script.count("Assert-EvalInventory $oldProfile.toolchains[0].root"), 2)
        self.assertIn("foreach($name in @($selected.Keys))", script)

    def test_only_current_first_party_targets_are_built_without_installing_tools(self):
        script = SCRIPT.read_text()
        for target in ["'codifix-windows-evaluation-helper'", "'codifix'", "'r3_snapshot'", "'r3_score'", "'r3_retriever'"]:
            self.assertIn(target, script)
        for forbidden in ["rustup install", "cargo install", "npm install", "--workspace", "--all-targets"]:
            self.assertNotIn(forbidden, script)
        self.assertIn("C:\\MemoryLab\\build\\product-msvc", script)
        self.assertIn("Native build changed frozen Cargo.lock", script)
        self.assertIn("7200-[int]$buildClock.Elapsed.TotalSeconds", script)

    @unittest.skipUnless(shutil.which("node"), "Node is required for the portable hash-driver fixture")
    def test_exact_embedded_profile_driver_preserves_canonical_hashes_and_rejects_old_drift(self):
        script = SCRIPT.read_text()
        driver = re.search(r"GetBytes\(@'\n(.*?)\n'@\)\)", script, re.S).group(1)
        with tempfile.TemporaryDirectory(prefix="memory-eval-bootstrap-test-") as directory:
            directory = Path(directory)
            driver_path = directory / "hash-profile.mjs"
            driver_path.write_text(driver)
            old_profile, old_result = directory / "old.json", directory / "result.json"
            new_profile, output = directory / "profile.json", directory / "execution.json"
            value = {"schema_version": 1, "node": {"path": "C:\\private\\node.exe", "sha256": "a" * 64}, "unicode": "caf\u00e9"}
            old_profile.write_text("\ufeff" + json.dumps(value))
            new_profile.write_text(json.dumps({**value, "schema_version": 2}))
            calculate = subprocess.run([shutil.which("node"), "--input-type=module", "-e",
                "import {nativeProfileHash} from './evals/r3/isolation/native-windows.mjs';"
                "console.log(nativeProfileHash(JSON.parse(process.argv[1])));", json.dumps(value)],
                cwd=ROOT, text=True, capture_output=True, check=True)
            old_result.write_text(json.dumps({"profile_sha256": calculate.stdout.strip()}))
            command = [shutil.which("node"), str(driver_path), str(ROOT / "evals/r3"),
                       str(old_profile), str(old_result), str(new_profile), str(output)]
            first = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(first.returncode, 0, first.stderr)
            result = json.loads(output.read_text())
            self.assertEqual(result["kind"], "windows-native-v1")
            self.assertEqual(result["profile"]["schema_version"], 2)
            self.assertRegex(result["profile_sha256"], r"^[0-9a-f]{64}$")
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0, "Existing execution evidence must not be replaced")
            output.unlink()
            old_result.write_text(json.dumps({"profile_sha256": "0" * 64}))
            failed = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn("Prior native profile hash changed", failed.stderr)
            self.assertFalse(output.exists())

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is unavailable for portable parser/function checks")
    def test_powershell_parser_and_exclusive_copy_helpers(self):
        subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File",
                        str(SCRIPT.with_name("test-native-eval-bootstrap.ps1"))], check=True)


if __name__ == "__main__":
    unittest.main()
