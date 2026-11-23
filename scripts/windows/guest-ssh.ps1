# SPDX-License-Identifier: AGPL-3.0-or-later
# Lab-only alternative when Windows Features-on-Demand download has failed.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ArchivePath)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'SSH provisioning is restricted to the disposable Memory VM'
}
# Official PowerShell/Win32-OpenSSH 10.0.0.0p2-Preview release asset. This is
# VM administration tooling, not a Memory runtime prerequisite or public claim.
$digest = '23f50f3458c4c5d0b12217c6a5ddfde0137210a30fa870e98b29827f7b43aba5'
if ((Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $digest) {
    throw 'OpenSSH archive differs from the reviewed upstream asset'
}
if (Get-Service sshd -ErrorAction SilentlyContinue) {
    throw 'An SSH service already exists; inspect it rather than replace its identity'
}
if (Get-Process dismhost -ErrorAction SilentlyContinue) {
    throw 'Windows servicing is still active; do not race the optional-feature installer'
}
$seedVolume = @(Get-Volume | Where-Object FileSystemLabel -eq 'MEMSEED')
if ($seedVolume.Count -ne 1 -or -not $seedVolume[0].DriveLetter) { throw 'Missing unique lab seed' }
$key = Join-Path ($seedVolume[0].DriveLetter + ':\') 'authorized_keys'
if (-not (Test-Path -LiteralPath $key)) { throw 'Existing lab public key is missing' }
$destination = Join-Path $env:ProgramFiles 'MemoryLab-OpenSSH-10.0.0.0p2'
if (Test-Path -LiteralPath $destination) { throw 'OpenSSH staging already exists; inspect interrupted execution' }
New-Item -ItemType Directory -Path $destination | Out-Null
& icacls.exe $destination /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-32-545:(OI)(CI)RX' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'SSH binary directory ACL failed' }
Expand-Archive -LiteralPath $ArchivePath -DestinationPath $destination
$binaries = Join-Path $destination 'OpenSSH-Win64'
foreach ($name in @('sshd.exe','ssh-keygen.exe','install-sshd.ps1')) {
    $signature = Get-AuthenticodeSignature -LiteralPath (Join-Path $binaries $name)
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
        throw 'OpenSSH Microsoft signature validation failed'
    }
}
& (Join-Path $binaries 'install-sshd.ps1') -Confirm:$false
if (-not (Get-Service sshd -ErrorAction SilentlyContinue)) { throw 'SSH installation did not create its service' }
$state = 'C:\ProgramData\ssh'
New-Item -ItemType Directory -Force -Path $state | Out-Null
$authorized = Join-Path $state 'administrators_authorized_keys'
if (Test-Path -LiteralPath $authorized) {
    if ((Get-FileHash -LiteralPath $authorized).Hash -ne (Get-FileHash -LiteralPath $key).Hash) {
        throw 'Existing SSH key differs; no key replacement is permitted'
    }
} else { Copy-Item -LiteralPath $key -Destination $authorized }
& icacls.exe $authorized /inheritance:r /grant:r '*S-1-5-18:F' '*S-1-5-32-544:F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'SSH public-key ACL failed' }
$config = Join-Path $state 'sshd_config'
if (Test-Path -LiteralPath $config) { Copy-Item -LiteralPath $config -Destination ($config + '.before-memory-lab') }
@'
Port 22
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AllowUsers memorylab
Subsystem sftp sftp-server.exe
Match Group administrators
    AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys
'@ | Set-Content -LiteralPath $config -Encoding ASCII
& (Join-Path $binaries 'ssh-keygen.exe') -A
if ($LASTEXITCODE -ne 0) { throw 'SSH host-key initialization failed' }
& (Join-Path $binaries 'sshd.exe') -t
if ($LASTEXITCODE -ne 0) { throw 'SSH configuration validation failed' }
# The VM has only QEMU user networking; the host forward is loopback-only.
New-NetFirewallRule -Name 'MemoryLab-KeyOnlySSH' -DisplayName 'Memory lab key-only SSH from private host' -Direction Inbound -Action Allow -Protocol TCP -LocalPort 22 -RemoteAddress '10.77.0.2' | Out-Null
Set-ItemProperty 'HKLM:\SOFTWARE\OpenSSH' DefaultShell 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
Set-Service sshd -StartupType Automatic
Start-Service sshd
[ordered]@{
    format='memory-windows-lab-ssh-v1'; archive_sha256=$digest
    source='https://github.com/PowerShell/Win32-OpenSSH/releases/tag/10.0.0.0p2-Preview'
    service_running=((Get-Service sshd).Status -eq 'Running')
    password_authentication=$false; public_key_reused=$true
    scope='disposable_vm_only'; observed_at=[DateTime]::UtcNow.ToString('o')
} | ConvertTo-Json | Set-Content -LiteralPath 'C:\MemoryLab\ssh-result.json' -Encoding UTF8
