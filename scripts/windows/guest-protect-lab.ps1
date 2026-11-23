# SPDX-License-Identifier: AGPL-3.0-or-later
# One-time lab ancestor repair, not a customer installation permission change.
[CmdletBinding()]
param([Parameter(Mandatory=$true)][guid]$OperationId)
$ErrorActionPreference='Stop'
if ($env:OS -ne 'Windows_NT' -or $env:COMPUTERNAME -ne 'MEMORY-WINLAB' -or $OperationId -eq [guid]::Empty) { throw 'Explicit disposable lab operation required' }
$root='C:\MemoryLab'
$item=Get-Item -LiteralPath $root -Force
if (-not $item.PSIsContainer -or $item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Exact regular lab directory required' }
$user=Get-LocalUser -Name 'memorylab'
if (-not $user.Enabled -or $user.SID.Value -notmatch '^S-1-5-21-[0-9-]+$') { throw 'Existing enabled lab identity required' }
$receipt=Join-Path $root ('ancestor-acl-'+$OperationId.ToString()+'.json')
if (Test-Path -LiteralPath $receipt) { throw 'ACL operation exists; inspect rather than overwrite' }
$before=(Get-Acl -LiteralPath $root).Sddl
@{format='memory-windows-lab-ancestor-acl-v1';operation_id=$OperationId.ToString();path=$root;before=$before;state='prepared'} |
    ConvertTo-Json | Set-Content -LiteralPath $receipt -Encoding UTF8
# Do not recurse/rewrite protected descendants. Inherited broad permissions are
# removed at their source; independent explicit service/user policies survive.
& "$env:SystemRoot\System32\icacls.exe" $root /inheritance:r /grant:r "*$($user.SID.Value):(OI)(CI)F" '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE) { throw 'Lab ancestor ACL failed; inspect retained intent' }
$after=Get-Acl -LiteralPath $root
if (-not $after.AreAccessRulesProtected) { throw 'Lab ancestor still inherits permissions' }
$allowed=@($user.SID.Value,'S-1-5-18','S-1-5-32-544')
foreach ($rule in $after.Access) {
    if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin $allowed) { throw 'Unexpected lab ancestor principal; no broad grant allowed' }
}
@{format='memory-windows-lab-ancestor-acl-v1';operation_id=$OperationId.ToString();path=$root;before=$before;after=$after.Sddl;state='succeeded';observed_at=[DateTime]::UtcNow.ToString('o')} |
    ConvertTo-Json | Set-Content -LiteralPath $receipt -Encoding UTF8
Write-Output 'Disposable lab ancestor protected; no other path selected'
