# SPDX-License-Identifier: AGPL-3.0-or-later
# Read-only native PE inspection. This does not copy/install CRT libraries.
function ConvertFrom-DumpbinDependencies {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][AllowEmptyCollection()][string[]]$Lines)
    $dlls = @($Lines | ForEach-Object {
        if ($_ -match '^\s+([A-Za-z0-9_.+-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
    } | Sort-Object -Unique)
    if (-not $dlls.Count) { throw 'Native PE dependency inspection returned no DLL imports' }
    return $dlls
}

function Get-WindowsRuntimeDependencies {
    param(
        [Parameter(Mandatory = $true)][string[]]$Binaries,
        [string]$DumpbinPath
    )
    if (-not $DumpbinPath) {
        $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
        if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Native MSVC dependency inspection unavailable' }
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Native MSVC dependency inspection unavailable' }
        $toolsets = @(Get-ChildItem -LiteralPath (Join-Path $vs 'VC\Tools\MSVC') -Directory |
            Sort-Object { [version]$_.Name } -Descending)
        if (-not $toolsets.Count) { throw 'Native MSVC toolset missing' }
        $DumpbinPath = Join-Path $toolsets[0].FullName 'bin\Hostx64\x64\dumpbin.exe'
    }
    if (-not (Test-Path -LiteralPath $DumpbinPath -PathType Leaf)) { throw 'Native dumpbin.exe unavailable' }
    $entries = @()
    foreach ($binary in $Binaries) {
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Packaged executable missing during PE inspection' }
        $output = @(& $DumpbinPath /nologo /dependents $binary 2>&1)
        if ($LASTEXITCODE -ne 0) { throw 'Native PE dependency inspection failed' }
        $dlls = @(ConvertFrom-DumpbinDependencies -Lines $output)
        $entries += [ordered]@{
            executable = [IO.Path]::GetFileName($binary)
            dlls = $dlls
            visual_cpp_v14_required = [bool]@($dlls | Where-Object { $_ -match '^(?:vcruntime|msvcp|concrt)140(?:_[0-9]+)?\.dll$' }).Count
        }
    }
    return [ordered]@{
        inspection_tool = 'Microsoft dumpbin /DEPENDENTS'
        tool_version = (Get-Item -LiteralPath $DumpbinPath).VersionInfo.FileVersion
        direct_imports_only = $true
        crt_bundled = $false
        executables = $entries
    }
}
