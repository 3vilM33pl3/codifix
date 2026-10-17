# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure parser contracts. Native dumpbin execution is performed by the builder.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'runtime-dependencies.ps1')
$dlls = @(ConvertFrom-DumpbinDependencies -Lines @(
    '',
    'Dump of file C:\build\codifix.exe',
    '  Image has the following dependencies:',
    '    KERNEL32.dll',
    '    VCRUNTIME140.dll',
    '    vcruntime140.DLL',
    '    api-ms-win-crt-runtime-l1-1-0.dll',
    '  Summary',
    '        2000 .data',
    ''
))
if (($dlls -join ',') -cne 'api-ms-win-crt-runtime-l1-1-0.dll,kernel32.dll,vcruntime140.dll') {
    throw 'PE DLL imports were not normalized/deduplicated correctly'
}
$rejected = $false
try { $null = ConvertFrom-DumpbinDependencies -Lines @('not valid dumpbin dependency output') }
catch { $rejected = $true }
if (-not $rejected) { throw 'Empty/native inspection failure was silently accepted' }
$rejected = $false
try { $null = Get-WindowsRuntimeDependencies -Binaries @('missing.exe') -DumpbinPath 'missing-dumpbin.exe' }
catch { $rejected = $true }
if (-not $rejected) { throw 'Missing native inspector was silently accepted' }
Write-Output 'Windows PE runtime-dependency parser tests passed'
