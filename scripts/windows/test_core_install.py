# SPDX-License-Identifier: AGPL-3.0-or-later
"""Portable contract checks only: never operates a guest, SCM or database."""
from pathlib import Path
import shutil
import subprocess
import unittest


SCRIPT = Path(__file__).with_name("guest-core-install.ps1")
NATIVE_FUNCTIONS = SCRIPT.parents[2] / "packaging/windows/native-instance-functions.ps1"


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
        for required in ["Get-WindowsReleaseIdentity '2.1.0-rc.5' $InstallerBuild", "lab_only=$true;release_candidate=$false",
                         "installer_source_commit=$head;binary_source_commit=$build.source_commit",
                         "'native-instance.ps1'", "'-Action','Preview','-PreviewAction','Install'",
                         "'-Action','Install','-ExpectedInputSha256',$preview.input_sha256",
                         "'-DatabaseCompatibilityConfirmed'", "schema.Trim() -cne '85:0'",
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
                         "return Join-Path $CoreRoot 'code\\codifix.exe'", "return Join-Path $DebugRoot $File",
                         "Copy-CoreInstallFile $from (Join-Path $payload ('bin\\'+$file)) $pin[0].sha256 $file",
                         "Pinned core installation input unavailable or changed: '+$Component"]:
            self.assertIn(required, source)
        self.assertNotIn("Copy-CoreInstallFile (Join-Path $debug $file)", source)

    def test_zip_names_are_explicit_canonical_files_and_new_builds_do_not_rebind(self):
        source = SCRIPT.read_text()
        for required in ["[ValidateRange(60000,60999)][int]$InstallerBuild=60000",
                         "installer_build=$InstallerBuild", "'LAB-ONLY-memory-rc5-build'+$InstallerBuild+'.zip'",
                         "New-CoreInstallArchive $payload $package $payloadFiles",
                         "New-NativeCanonicalArchive $Payload $Package $PayloadFiles",
                         "Selected LAB-ONLY build already exists; preserve it"]:
            self.assertIn(required, source)
        writer = NATIVE_FUNCTIONS.read_text().split("function New-NativeCanonicalArchive", 1)[1].split("function Get-NativeInputDigest", 1)[0]
        for required in ["[ValidateRange(1,65536)][int]$FileLimit=20000",
                         "$entryName=([string]$item.name).Replace('\\','/');Assert-NativeArchiveName $entryName",
                         "$archive.CreateEntry($entryName,[IO.Compression.CompressionLevel]::Optimal)",
                         "$entryName.EndsWith('/')", "$sha.ComputeHash($inputFile)",
                         "$entryName -cne $item.name", "ReparsePoint", "[IO.FileMode]::CreateNew", "$stream.Flush($true)"]:
            self.assertIn(required, writer)
        self.assertNotIn("[IO.Compression.ZipFile]::CreateFromDirectory", source)
        self.assertNotIn("installer_build=60000", source)

    def test_runtime_packages_share_canonical_writer_without_weakening_reader(self):
        source = SCRIPT.with_name("guest-runtime-pack.ps1").read_text()
        self.assertIn("$fileLimit=if($manifest.kind -ceq 'memory-native-pds-runtime'){65536}else{20000}", source)
        self.assertIn("New-NativeCanonicalArchive $script:payload $zip $manifest.payload_files -FileLimit $fileLimit", source)
        self.assertNotIn("[IO.Compression.ZipFile]::CreateFromDirectory", source)
        guard = NATIVE_FUNCTIONS.read_text().split("function Assert-NativeArchiveName", 1)[1].split("function New-NativeCanonicalArchive", 1)[0]
        self.assertIn("$Name.Contains('\\')", guard)

    def test_native_scm_failures_persist_only_static_bounded_diagnostics(self):
        source = (NATIVE_FUNCTIONS.parent / "native-instance.ps1").read_text()
        function = source.split("function Invoke-Sc", 1)[1].split("function Get-SelectedService", 1)[0]
        self.assertIn("[Diagnostics.ProcessStartInfo]::new()", function)
        self.assertIn("$start.Arguments=($Arguments | ForEach-Object {ConvertTo-NativeCommandArgument $_}) -join ' '", function)
        self.assertIn("$start.UseShellExecute=$false", function)
        self.assertIn("WaitForExit(30000)", function)
        self.assertIn("$out.Length+$err.Length -gt 64KB", function)
        self.assertIn("$code=$process.ExitCode", function)
        self.assertNotIn("@Arguments 2>&1", function)
        self.assertIn("$journal.scm_failure=Get-NativeScmFailure $Arguments[0] $code", function)
        self.assertLess(function.index("Write-NativeJournal $journalPath $journal"), function.index('throw "SCM'))
        helper = NATIVE_FUNCTIONS.read_text().split("function Get-NativeScmFailure", 1)[1].split("function Assert-NativeArchiveName", 1)[0]
        self.assertIn("[ComponentModel.Win32Exception]::new($ExitCode).Message", helper)
        self.assertIn("$message.Substring(0,512)", helper)
        self.assertIn("@{operation=$Operation;exit_code=$ExitCode;win32_message=$message}", helper)

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
                         "'bin\\codifix.exe'", "UseProxy=$false", "AllowAutoRedirect=$false",
                         "Test-NativeReadinessHealth $health $instance", "$htmlHash -cne $receipt.web_index_sha256",
                         "'/v1/query'", "retrieval_mode='lexical';answer_mode='deterministic'",
                         "'/v1/projects/native-smoke/memories'", "$_.id -ceq $core.curated_memory_id"]:
            self.assertIn(required, source)

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell unavailable; native/parser proof remains unclaimed")
    def test_powershell_parser_and_pure_helpers(self):
        for test in ["test-core-install.ps1", "test-runtime-pack.ps1"]:
            subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File",
                            str(SCRIPT.with_name(test))], check=True)
        subprocess.run([shutil.which("pwsh"), "-NoProfile", "-File",
                        str(NATIVE_FUNCTIONS.with_name("test-native-instance.ps1"))], check=True)


if __name__ == "__main__":
    unittest.main()
