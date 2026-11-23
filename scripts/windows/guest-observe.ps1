# SPDX-License-Identifier: AGPL-3.0-or-later
# Read-only OS acceptance evidence. Never export passwords, keys or environment.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB') {
    throw 'Observation is restricted to the disposable Memory VM'
}
$os = Get-CimInstance Win32_OperatingSystem
$version = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$tpm = Get-Tpm
$boot = Confirm-SecureBootUEFI
$observedAt = [DateTime]::UtcNow
$licenses = @(Get-CimInstance SoftwareLicensingProduct | Where-Object {
    $_.Name -like 'Windows*' -and $_.PartialProductKey
} | ForEach-Object {
    # PartialProductKey selects installed licences but is deliberately not exported.
    # Some evaluation builds expose the FILETIME epoch, not an actual expiry.
    # Preserve that distinction instead of reporting 1601 as a licence date.
    $end = $_.EvaluationEndDate
    $reportedEnd = if ($end -and ([DateTime]$end).Year -ge 2000) {
        ([DateTime]$end).ToUniversalTime().ToString('o')
    } else { $null }
    $estimatedEnd = if ($_.GracePeriodRemaining -gt 0) {
        $observedAt.AddMinutes($_.GracePeriodRemaining).ToString('o')
    } else { $null }
    @{name=$_.Name;description=$_.Description;status=$_.LicenseStatus;
      grace_remaining_minutes=$_.GracePeriodRemaining;
      evaluation_end_date=$reportedEnd;estimated_grace_end_at=$estimatedEnd}
})
$services = @('sshd','QEMU-GA') | ForEach-Object {
    $service = Get-Service -Name $_ -ErrorAction SilentlyContinue
    @{name=$_;present=($null -ne $service);status=([string]$service.Status)}
}
$drivers = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object {
    $_.DeviceName -match 'VirtIO|Red Hat'
} | ForEach-Object {
    @{name=$_.DeviceName;version=$_.DriverVersion;signed=$_.IsSigned}
})
[ordered]@{
    format='memory-windows-lab-observation-v1'
    observed_at=$observedAt.ToString('o')
    computer_name=$env:COMPUTERNAME
    os=$os.Caption
    version=$os.Version
    build=$os.BuildNumber
    update_build_revision=$version.UBR
    edition=$version.EditionID
    architecture=$env:PROCESSOR_ARCHITECTURE
    timezone=(Get-TimeZone).Id
    secure_boot=$boot
    tpm_present=$tpm.TpmPresent
    tpm_ready=$tpm.TpmReady
    licenses=$licenses
    services=@($services)
    signed_virtio_drivers=$drivers
    bootstrap_completed=(Test-Path 'C:\MemoryLab\bootstrap-result.json')
} | ConvertTo-Json -Depth 6
