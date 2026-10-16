# SPDX-License-Identifier: AGPL-3.0-or-later
# Runs only in the disposable evaluation VM, once after unattended install.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Lab = 'C:\MemoryLab'
New-Item -ItemType Directory -Force $Lab | Out-Null
Start-Transcript -Path "$Lab\bootstrap.log" -Append
try {
    $seed = (Get-Volume | Where-Object FileSystemLabel -eq 'MEMSEED').DriveLetter
    if (-not $seed) { throw 'Missing lab seed volume' }
    $seed = "$seed`:"
    # Install signed native VirtIO network/serial drivers and QEMU guest agent.
    $drivers = Get-Volume | Where-Object { $_.DriveLetter -and $_.FileSystemLabel -like 'virtio*' }
    foreach ($volume in $drivers) {
        $root = "$($volume.DriveLetter):\"
        foreach ($driver in @('NetKVM\w11\amd64\netkvm.inf', 'vioserial\w11\amd64\vioser.inf')) {
            $path = Join-Path $root $driver
            if (-not (Test-Path $path)) { throw "Missing pinned driver: $driver" }
            & pnputil.exe /add-driver $path /install
            if ($LASTEXITCODE -notin @(0, 3010)) { throw "Driver installation failed: $driver" }
        }
        $agent = Join-Path $root 'guest-agent\qemu-ga-x86_64.msi'
        $process = Start-Process msiexec.exe -ArgumentList @('/i', "`"$agent`"", '/qn', '/norestart') -Wait -PassThru
        if ($process.ExitCode -notin @(0, 3010)) { throw 'Guest agent installation failed' }
    }
    $server = Get-WindowsCapability -Online | Where-Object Name -like 'OpenSSH.Server*'
    if ($server.State -ne 'Installed') { Add-WindowsCapability -Online -Name $server.Name | Out-Null }
    # The capability need not create this directory until sshd first starts.
    # Prepare it before writing the reviewed key/configuration, not afterward.
    New-Item -ItemType Directory -Force 'C:\ProgramData\ssh' | Out-Null
    Copy-Item "$seed\authorized_keys" 'C:\ProgramData\ssh\administrators_authorized_keys'
    & icacls.exe 'C:\ProgramData\ssh\administrators_authorized_keys' /inheritance:r /grant:r '*S-1-5-18:F' '*S-1-5-32-544:F'
    if ($LASTEXITCODE -ne 0) { throw 'SSH key ACL failed' }
    $config = 'C:\ProgramData\ssh\sshd_config'
    @'
Port 22
PasswordAuthentication no
PubkeyAuthentication yes
AllowUsers memorylab
Subsystem sftp sftp-server.exe
Match Group administrators
    AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys
'@ | Set-Content $config
    & 'C:\Windows\System32\OpenSSH\sshd.exe' -t
    if ($LASTEXITCODE -ne 0) { throw 'Invalid SSH configuration' }
    Set-Service sshd -StartupType Automatic
    Start-Service sshd
    # No RDP forwarding. VNC and forwarded SSH are loopback/private-host only.
    $autologon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    Set-ItemProperty $autologon AutoAdminLogon '0'
    Remove-ItemProperty $autologon DefaultPassword -ErrorAction SilentlyContinue
    Remove-Item 'C:\Windows\Panther\Unattend.xml' -ErrorAction SilentlyContinue
    Set-ItemProperty 'HKLM:\SOFTWARE\OpenSSH' DefaultShell 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
    @{
        format = 'memory-windows-lab-bootstrap-v1'
        os = (Get-CimInstance Win32_OperatingSystem).Caption
        build = (Get-CimInstance Win32_OperatingSystem).BuildNumber
        architecture = $env:PROCESSOR_ARCHITECTURE
        ssh_password_authentication = $false
        completed_at = [DateTime]::UtcNow.ToString('o')
    } | ConvertTo-Json | Set-Content "$Lab\bootstrap-result.json"
} finally { Stop-Transcript }
