[CmdletBinding()]
param(
    [switch]$SkipBuild,
    # Windows Installer compares only three numeric fields. RC semver is not an
    # installer version; advance this number for every published Windows build.
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 65535)]
    [int]$InstallerBuild
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Resolve-Path (Join-Path $ScriptDir '..\..')
$RootDir = $RootDir.Path
$Target = 'x86_64-pc-windows-msvc'
$Executables = @('memory.exe', 'memory-windows-service.exe', 'memory-area.exe',
    'memory-area-import.exe', 'memory-gateway-schema.exe', 'memory-federation-gateway.exe',
    'memory-federation-enroll.exe', 'memory-pds-pipe.exe', 'memory-workshop-worker.exe', 'memory-pi-sandbox.exe')
$CargoTargetDir = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RootDir 'target'
}
$Version = (Select-String -Path (Join-Path $RootDir 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $Version) { $Version = '0.1.0' }
. (Join-Path $ScriptDir 'release-identity.ps1')
$Identity = Get-WindowsReleaseIdentity -ApplicationVersion $Version -InstallerBuild $InstallerBuild
$MsiVersion = $Identity.installer_version
$SourceCommit = (& git -C $RootDir rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $SourceCommit -notmatch '^[0-9a-f]{40}$') { throw 'Cannot resolve source commit' }
$SourceChanges = & git -C $RootDir status --porcelain --untracked-files=no
if ($LASTEXITCODE -ne 0 -or $SourceChanges) { throw 'Windows candidates must be built from a clean committed source tree' }

$BuildRoot = Join-Path $RootDir 'target\windows'
$BuildReceiptPath = Join-Path $BuildRoot 'client-build-receipt.json'
$StageDir = Join-Path $BuildRoot 'stage'
$DistDir = Join-Path $BuildRoot "dist\$($Identity.release_identity)"
$WxsPath = Join-Path $BuildRoot 'memory-layer.wxs'
$ZipPath = Join-Path $DistDir "$($Identity.asset_prefix).zip"
$MsiPath = Join-Path $DistDir "$($Identity.asset_prefix).msi"
$ServerToolsMsiPath = Join-Path $DistDir "$($Identity.asset_prefix)-server-tools.msi"
$WixPdbPath = [System.IO.Path]::ChangeExtension($MsiPath, '.wixpdb')

function Assert-BuildPath([string]$Path) {
    $fullPath = [System.IO.Path]::GetFullPath($Path)
    $rootPath = [System.IO.Path]::GetFullPath($BuildRoot).TrimEnd('\') + '\'
    if (-not $fullPath.StartsWith($rootPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the Windows build directory: $fullPath"
    }
    return $fullPath
}

function Copy-Tree($Source, $Destination) {
    $Destination = Assert-BuildPath $Destination
    if (Test-Path $Destination) { Remove-Item -Recurse -Force $Destination }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Copy-Item -Recurse -Force -Path (Join-Path $Source '*') -Destination $Destination
}

function Xml-Escape([string]$Value) {
    return [System.Security.SecurityElement]::Escape($Value)
}

function Get-RelativePath([string]$BasePath, [string]$Path) {
    $baseFullPath = [System.IO.Path]::GetFullPath($BasePath).TrimEnd('\', '/') + '\'
    $pathFullPath = [System.IO.Path]::GetFullPath($Path)
    $baseUri = [System.Uri]::new($baseFullPath)
    $pathUri = [System.Uri]::new($pathFullPath)
    return [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($pathUri).ToString()).Replace('/', '\')
}

function Safe-Id([string]$Value) {
    $safe = $Value -replace '[^A-Za-z0-9_\.]', '_'
    if ($safe -notmatch '^[A-Za-z_]') { $safe = "_$safe" }
    if ($safe.Length -gt 44) { $safe = $safe.Substring(0, 44) }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        $hash = ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').Substring(0, 12)
    } finally {
        $sha.Dispose()
    }
    return "${safe}_$hash"
}

function Get-DeterministicGuid([string]$Value) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        $hex = ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').Substring(0, 32)
    } finally {
        $sha.Dispose()
    }
    return "$($hex.Substring(0, 8))-$($hex.Substring(8, 4))-$($hex.Substring(12, 4))-$($hex.Substring(16, 4))-$($hex.Substring(20, 12))"
}

function Normalize-Msi([string]$Path, [string]$PackageCode) {
    $installer = $null
    $database = $null
    $summary = $null
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.GetType().InvokeMember(
            'OpenDatabase',
            'InvokeMethod',
            $null,
            $installer,
            @([System.IO.Path]::GetFullPath($Path), 1)
        )
        $summary = $database.GetType().InvokeMember(
            'SummaryInformation',
            'GetProperty',
            $null,
            $database,
            @(3)
        )
        $fixedTimestamp = [datetime]'2000-01-01T00:00:00'
        $summary.GetType().InvokeMember('Property', 'SetProperty', $null, $summary, @(9, "{$PackageCode}")) | Out-Null
        $summary.GetType().InvokeMember('Property', 'SetProperty', $null, $summary, @(12, $fixedTimestamp)) | Out-Null
        $summary.GetType().InvokeMember('Property', 'SetProperty', $null, $summary, @(13, $fixedTimestamp)) | Out-Null
        $summary.GetType().InvokeMember('Persist', 'InvokeMethod', $null, $summary, $null) | Out-Null
        $database.GetType().InvokeMember('Commit', 'InvokeMethod', $null, $database, $null) | Out-Null
    } finally {
        foreach ($comObject in @($summary, $database, $installer)) {
            if ($null -ne $comObject -and [System.Runtime.InteropServices.Marshal]::IsComObject($comObject)) {
                [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($comObject)
            }
        }
        $summary = $null
        $database = $null
        $installer = $null
        [System.GC]::Collect()
        [System.GC]::WaitForPendingFinalizers()
    }

    # An MSI is an unsigned CFB compound file. Windows Installer updates the root
    # storage timestamps when the Summary Information stream is committed. Clear
    # those non-semantic fields so identical database contents produce identical
    # package bytes. Creation and modification FILETIMEs occupy bytes 0x64..0x73
    # in the root directory entry.
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $signature = [byte[]](0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1)
    for ($index = 0; $index -lt $signature.Length; $index++) {
        if ($bytes[$index] -ne $signature[$index]) { throw "Not a CFB MSI file: $Path" }
    }
    $sectorSize = 1 -shl [System.BitConverter]::ToUInt16($bytes, 0x1E)
    $directorySector = [System.BitConverter]::ToUInt32($bytes, 0x30)
    if ($directorySector -gt 0x7FFFFFFF) { throw "Invalid CFB directory sector in $Path" }
    $rootEntryOffset = ([int64]$directorySector + 1) * $sectorSize
    if ($rootEntryOffset + 0x74 -gt $bytes.LongLength) { throw "CFB root entry is outside $Path" }
    for ($index = 0x64; $index -lt 0x74; $index++) { $bytes[$rootEntryOffset + $index] = 0 }
    [System.IO.File]::WriteAllBytes($Path, $bytes)
}

function New-DeterministicZip([string]$Source, [string]$Destination) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force }
    $stream = [System.IO.File]::Open($Destination, [System.IO.FileMode]::CreateNew)
    try {
        $archive = [System.IO.Compression.ZipArchive]::new(
            $stream,
            [System.IO.Compression.ZipArchiveMode]::Create,
            $false
        )
        try {
            foreach ($file in (Get-ChildItem -LiteralPath $Source -Recurse -File | Sort-Object FullName)) {
                $relative = (Get-RelativePath $Source $file.FullName).Replace('\', '/')
                $entry = $archive.CreateEntry($relative, [System.IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = [System.DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [System.TimeSpan]::Zero)
                $input = [System.IO.File]::OpenRead($file.FullName)
                $output = $entry.Open()
                try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            }
        } finally {
            $archive.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function New-WixDirectoryXml([string]$DirectoryPath, [string]$DirectoryId, [array]$AllFiles, [ref]$ComponentRefs, [int]$Depth) {
    $indent = '  ' * $Depth
    $name = Split-Path -Leaf $DirectoryPath
    $xml = @()
    $xml += "$indent<Directory Id=`"$DirectoryId`" Name=`"$(Xml-Escape $name)`">"

    $cleanupId = "cleanup_$DirectoryId"
    $removeId = "remove_$DirectoryId"
    $cleanupGuid = Get-DeterministicGuid "Memory Layer|component|$cleanupId"
    $ComponentRefs.Value += $cleanupId
    $xml += "$indent  <Component Id=`"$cleanupId`" Guid=`"{$cleanupGuid}`">"
    $xml += "$indent    <RemoveFolder Id=`"$removeId`" On=`"uninstall`" />"
    $xml += "$indent    <RegistryValue Root=`"HKCU`" Key=`"Software\Memory Layer\Components`" Name=`"$cleanupId`" Type=`"integer`" Value=`"1`" KeyPath=`"yes`" />"
    $xml += "$indent  </Component>"

    $files = $AllFiles | Where-Object { (Split-Path -Parent $_.FullName) -eq $DirectoryPath } | Sort-Object FullName
    foreach ($file in $files) {
        $relative = Get-RelativePath $StageDir $file.FullName
        $idBase = Safe-Id($relative)
        $componentId = "cmp_$idBase"
        $fileId = "fil_$idBase"
        $componentGuid = Get-DeterministicGuid "Memory Layer|component|$componentId"
        $ComponentRefs.Value += $componentId
        $xml += "$indent  <Component Id=`"$componentId`" Guid=`"{$componentGuid}`">"
        $xml += "$indent    <File Id=`"$fileId`" Source=`"$(Xml-Escape $file.FullName)`" />"
        $xml += "$indent    <RegistryValue Root=`"HKCU`" Key=`"Software\Memory Layer\Components`" Name=`"$componentId`" Type=`"integer`" Value=`"1`" KeyPath=`"yes`" />"
        $xml += "$indent  </Component>"
    }

    $dirs = Get-ChildItem -LiteralPath $DirectoryPath -Directory | Sort-Object FullName
    foreach ($dir in $dirs) {
        $relative = Get-RelativePath $StageDir $dir.FullName
        $childId = "dir_$(Safe-Id $relative)"
        $xml += New-WixDirectoryXml -DirectoryPath $dir.FullName -DirectoryId $childId -AllFiles $AllFiles -ComponentRefs $ComponentRefs -Depth ($Depth + 1)
    }

    $xml += "$indent</Directory>"
    return $xml
}

if (-not $SkipBuild) {
    Write-Host 'Building web UI...'
    npm --prefix (Join-Path $RootDir 'web') ci
    if ($LASTEXITCODE -ne 0) { throw 'Web dependency installation failed' }
    npm --prefix (Join-Path $RootDir 'web') run build
    if ($LASTEXITCODE -ne 0) { throw 'Web build failed' }

    Write-Host 'Building Windows release binary...'
    if (-not $env:CARGO_PROFILE_RELEASE_LTO) {
        $env:CARGO_PROFILE_RELEASE_LTO = 'false'
    }
    if (-not $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS) {
        $env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '16'
    }
    Write-Host "Using Windows Cargo release overrides: LTO=$env:CARGO_PROFILE_RELEASE_LTO CODEGEN_UNITS=$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
    cargo build --release --locked --features full --bin memory --target $Target --manifest-path (Join-Path $RootDir 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Memory build failed' }
    cargo build --release --locked -p mem-platform -p mem-area -p mem-area-ops -p mem-federation-runtime -p mem-supervisor-runtime -p mem-supervisor-pi --bins --target $Target --manifest-path (Join-Path $RootDir 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'First-party native operators build failed' }
    New-Item -ItemType Directory -Force -Path $BuildRoot | Out-Null
    [ordered]@{
        source_commit = $SourceCommit
        toolchains = [ordered]@{ rustc = ((& rustc --version) -join "`n"); cargo = ((& cargo --version) -join "`n"); node = ((& node --version) -join "`n"); npm = ((& npm --version) -join "`n"); powershell = $PSVersionTable.PSVersion.ToString() }
        executables = @($Executables | ForEach-Object {
            [ordered]@{ name = $_; sha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $CargoTargetDir "$Target\release\$_")).Hash.ToLowerInvariant() }
        })
        # Bind cached Web assets as well as the executables for -SkipBuild.
        web_files = @(Get-ChildItem -LiteralPath (Join-Path $RootDir 'web\dist') -File -Recurse | Sort-Object FullName | ForEach-Object {
            [ordered]@{ name = Get-RelativePath (Join-Path $RootDir 'web\dist') $_.FullName; sha256 = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant() }
        })
    } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $BuildReceiptPath
}

if (-not (Test-Path -LiteralPath $BuildReceiptPath)) { throw 'Missing successful build receipt; run without -SkipBuild first' }
$buildReceipt = Get-Content -LiteralPath $BuildReceiptPath -Raw | ConvertFrom-Json
if ($buildReceipt.source_commit -ne $SourceCommit) { throw 'Cached binaries were built from another commit' }
foreach ($exe in @($buildReceipt.executables)) {
    if ($exe.name -notin $Executables) { throw 'Invalid build receipt executable' }
    $actual = (Get-FileHash -Algorithm SHA256 (Join-Path $CargoTargetDir "$Target\release\$($exe.name)")).Hash.ToLowerInvariant()
    if ($actual -ne $exe.sha256) { throw 'Cached executable changed since build' }
}
if (@($buildReceipt.executables).Count -ne $Executables.Count -or (Compare-Object -ReferenceObject $Executables -DifferenceObject @($buildReceipt.executables | ForEach-Object { $_.name }))) { throw 'Incomplete executable build receipt' }
$actualWeb = @(Get-ChildItem -LiteralPath (Join-Path $RootDir 'web\dist') -File -Recurse | Sort-Object FullName | ForEach-Object {
    "$(Get-RelativePath (Join-Path $RootDir 'web\dist') $_.FullName)|$((Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant())"
})
$expectedWeb = @($buildReceipt.web_files | ForEach-Object { "$($_.name)|$($_.sha256)" })
if (Compare-Object -ReferenceObject $expectedWeb -DifferenceObject $actualWeb) { throw 'Cached Web assets changed since build' }

$StageDir = Assert-BuildPath $StageDir
$DistDir = Assert-BuildPath $DistDir
if (Test-Path -LiteralPath $DistDir) { throw "Candidate output already exists; use a new InstallerBuild or preserve it elsewhere: $DistDir" }
Remove-Item -Recurse -Force $StageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $StageDir, $DistDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $StageDir 'bin') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $StageDir 'share\memory-layer') | Out-Null

foreach ($executable in $Executables) {
    $ExePath = Join-Path $CargoTargetDir "$Target\release\$executable"
    if (-not (Test-Path $ExePath)) { throw "Missing Windows binary: $ExePath" }
    Copy-Item -Force $ExePath (Join-Path $StageDir "bin\$executable")
}
Copy-Item -Force (Join-Path $RootDir 'README.md') (Join-Path $StageDir 'README.md')
Copy-Item -Force (Join-Path $RootDir 'memory-layer.toml.example') (Join-Path $StageDir 'memory-layer.toml.example')
Copy-Item -Force (Join-Path $RootDir 'packaging\windows\postgres.compose.yaml') (Join-Path $StageDir 'share\memory-layer\postgres.compose.yaml')
Copy-Tree (Join-Path $RootDir 'web\dist') (Join-Path $StageDir 'share\memory-layer\web')
Copy-Tree (Join-Path $RootDir '.agents\skills') (Join-Path $StageDir 'share\memory-layer\skill-template')
Copy-Tree (Join-Path $RootDir 'packaging\windows-pds') (Join-Path $StageDir 'share\memory-layer\windows-pds')
New-Item -ItemType Directory -Force -Path (Join-Path $StageDir 'share\memory-layer\operators') | Out-Null
foreach ($operator in @('native-instance.ps1', 'native-instance-functions.ps1', 'release-identity.ps1',
    'native-prerequisites.ps1', 'native-prerequisites-functions.ps1')) {
    Copy-Item -LiteralPath (Join-Path $ScriptDir $operator) -Destination (Join-Path $StageDir "share\memory-layer\operators\$operator")
}

New-Item -ItemType Directory -Force -Path (Join-Path $StageDir 'completions') | Out-Null
& (Join-Path $StageDir 'bin\memory.exe') completion powershell | Out-File -Encoding utf8 (Join-Path $StageDir 'completions\memory.ps1')
if ($LASTEXITCODE -ne 0) { throw 'PowerShell completion generation failed' }

$fixedTimestamp = [datetime]::SpecifyKind([datetime]'2000-01-01T00:00:00', [System.DateTimeKind]::Utc)
Get-ChildItem -LiteralPath $StageDir -Recurse -Force | ForEach-Object { $_.LastWriteTimeUtc = $fixedTimestamp }
New-DeterministicZip -Source $StageDir -Destination $ZipPath

$allFiles = Get-ChildItem -LiteralPath $StageDir -File -Recurse | Sort-Object FullName
$componentRefs = New-Object System.Collections.ArrayList
$rootFiles = $allFiles | Where-Object { (Split-Path -Parent $_.FullName) -eq $StageDir }
$directoryXml = @()

foreach ($file in $rootFiles) {
    $relative = Get-RelativePath $StageDir $file.FullName
    $idBase = Safe-Id($relative)
    $componentId = "cmp_$idBase"
    $fileId = "fil_$idBase"
    $componentGuid = Get-DeterministicGuid "Memory Layer|component|$componentId"
    [void]$componentRefs.Add($componentId)
    $directoryXml += "        <Component Id=`"$componentId`" Guid=`"{$componentGuid}`">"
    $directoryXml += "          <File Id=`"$fileId`" Source=`"$(Xml-Escape $file.FullName)`" />"
    $directoryXml += "          <RegistryValue Root=`"HKCU`" Key=`"Software\Memory Layer\Components`" Name=`"$componentId`" Type=`"integer`" Value=`"1`" KeyPath=`"yes`" />"
    $directoryXml += "        </Component>"
}

[void]$componentRefs.Add('cmp_InstallFolderCleanup')
$installCleanupGuid = Get-DeterministicGuid 'Memory Layer|component|cmp_InstallFolderCleanup'
$directoryXml += "        <Component Id=`"cmp_InstallFolderCleanup`" Guid=`"{$installCleanupGuid}`">"
$directoryXml += '          <RemoveFolder Id="remove_InstallFolder" On="uninstall" />'
$directoryXml += '          <RegistryValue Root="HKCU" Key="Software\Memory Layer\Components" Name="cmp_InstallFolderCleanup" Type="integer" Value="1" KeyPath="yes" />'
$directoryXml += '        </Component>'

foreach ($dir in (Get-ChildItem -LiteralPath $StageDir -Directory | Sort-Object FullName)) {
    $relative = Get-RelativePath $StageDir $dir.FullName
    $directoryXml += New-WixDirectoryXml -DirectoryPath $dir.FullName -DirectoryId "dir_$(Safe-Id $relative)" -AllFiles $allFiles -ComponentRefs ([ref]$componentRefs) -Depth 4
}

$componentRefs += 'cmp_ProgramsFolderCleanup'

$featureRefs = $componentRefs | Sort-Object | ForEach-Object { "      <ComponentRef Id=`"$_`" />" }
$upgradeCode = '7E6D7DA0-7D74-43F0-B816-E422B9E01B82'
$productCode = Get-DeterministicGuid "Memory Layer|$($Identity.release_identity)|x64|perUser"
$packageCode = Get-DeterministicGuid "Memory Layer|package|$($Identity.release_identity)|$SourceCommit|x64|perUser"
$programsCleanupGuid = Get-DeterministicGuid 'Memory Layer|component|cmp_ProgramsFolderCleanup'
$userPathGuid = Get-DeterministicGuid 'Memory Layer|component|cmp_UserPath'
$wxs = @"
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package ProductCode="{$productCode}" Name="Memory Layer" Manufacturer="Memory Layer" Version="$MsiVersion" UpgradeCode="{$upgradeCode}" Scope="perUser">
    <MajorUpgrade DowngradeErrorMessage="A newer version of Memory Layer is already installed." />
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="LocalAppDataFolder">
      <Directory Id="ProgramsFolder" Name="Programs">
        <Component Id="cmp_ProgramsFolderCleanup" Guid="{$programsCleanupGuid}">
          <RemoveFolder Id="remove_ProgramsFolder" On="uninstall" />
          <RegistryValue Root="HKCU" Key="Software\Memory Layer\Components" Name="cmp_ProgramsFolderCleanup" Type="integer" Value="1" KeyPath="yes" />
        </Component>
        <Directory Id="INSTALLFOLDER" Name="Memory Layer">
$($directoryXml -join "`n")
          <Component Id="cmp_UserPath" Guid="{$userPathGuid}">
            <Environment Id="env_UserPath" Name="PATH" Value="[INSTALLFOLDER]bin" Action="set" Part="last" System="no" />
            <RegistryValue Root="HKCU" Key="Software\Memory Layer" Name="InstallDir" Type="string" Value="[INSTALLFOLDER]" KeyPath="yes" />
          </Component>
        </Directory>
      </Directory>
    </StandardDirectory>
    <Feature Id="Main" Title="Memory Layer" Level="1">
$($featureRefs -join "`n")
      <ComponentRef Id="cmp_UserPath" />
    </Feature>
  </Package>
</Wix>
"@
Set-Content -Path $WxsPath -Value $wxs -Encoding utf8

$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) { throw 'WiX CLI not found. Install with: dotnet tool install --global wix' }
& $wix.Source build $WxsPath -arch x64 -out $MsiPath
if ($LASTEXITCODE -ne 0) { throw "WiX build failed with exit code $LASTEXITCODE" }
Remove-Item -Force $WixPdbPath -ErrorAction SilentlyContinue
Normalize-Msi -Path $MsiPath -PackageCode $packageCode

# Separate machine-level tools scaffold. It neither provisions third-party
# dependencies nor starts services/custom actions under an installer identity.
# State and immutable service releases remain outside the MSI-owned directory.
$serverUpgradeCode = 'B209C344-2641-4F0C-BEAF-37A9C827C243'
$serverProductCode = Get-DeterministicGuid "Memory Layer|server-tools|$($Identity.release_identity)|x64"
$serverPackageCode = Get-DeterministicGuid "Memory Layer|server-tools-package|$($Identity.release_identity)|$SourceCommit|x64"
$serverDirectoryXml = ($directoryXml -join "`n").Replace('Root="HKCU"', 'Root="HKLM"').Replace('Software\Memory Layer\Components', 'Software\Memory Layer Native Server Tools\Components')
$serverDirectoryXml = [regex]::Replace($serverDirectoryXml, 'Guid="\{([A-Fa-f0-9-]+)\}"', {
    param($match)
    'Guid="{' + (Get-DeterministicGuid "Memory Layer|server-tools-component|$($match.Groups[1].Value)") + '}"'
})
$serverFeatureRefs = $componentRefs | Where-Object { $_ -ne 'cmp_ProgramsFolderCleanup' } | Sort-Object | ForEach-Object { "      <ComponentRef Id=`"$_`" />" }
$serverWxsPath = Join-Path $BuildRoot 'memory-layer-server-tools.wxs'
$serverWxs = @"
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package ProductCode="{$serverProductCode}" Name="Memory Layer Native Server Tools" Manufacturer="Memory Layer" Version="$MsiVersion" UpgradeCode="{$serverUpgradeCode}" Scope="perMachine">
    <MajorUpgrade DowngradeErrorMessage="A newer Memory Layer server tools build is installed." />
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="ProgramFiles64Folder">
      <Directory Id="INSTALLFOLDER" Name="Memory Layer Native Server Tools">
$serverDirectoryXml
      </Directory>
    </StandardDirectory>
    <Feature Id="Main" Title="Native server tools (dependencies installed separately)" Level="1">
$($serverFeatureRefs -join "`n")
    </Feature>
  </Package>
</Wix>
"@
Set-Content -LiteralPath $serverWxsPath -Value $serverWxs -Encoding utf8
& $wix.Source build $serverWxsPath -arch x64 -out $ServerToolsMsiPath
if ($LASTEXITCODE -ne 0) { throw "Server tools WiX build failed with exit code $LASTEXITCODE" }
Remove-Item -LiteralPath ([IO.Path]::ChangeExtension($ServerToolsMsiPath, '.wixpdb')) -ErrorAction SilentlyContinue
Normalize-Msi -Path $ServerToolsMsiPath -PackageCode $serverPackageCode

foreach ($artifact in @($ZipPath, $MsiPath, $ServerToolsMsiPath)) {
    $hash = Get-FileHash -Algorithm SHA256 $artifact
    "$($hash.Hash.ToLowerInvariant())  $(Split-Path -Leaf $artifact)" | Set-Content -Encoding ascii "$artifact.sha256"
}

$artifacts = @($ZipPath, $MsiPath, $ServerToolsMsiPath) | ForEach-Object {
    [ordered]@{ name = (Split-Path -Leaf $_); sha256 = (Get-FileHash -Algorithm SHA256 $_).Hash.ToLowerInvariant(); bytes = (Get-Item $_).Length }
}
$manifest = [ordered]@{
    schema_version = 1
    platform = 'windows-x86_64'
    application_version = $Version
    release_identity = $Identity.release_identity
    installer_build = $InstallerBuild
    installer_version = $MsiVersion
    source_commit = $SourceCommit
    build_toolchains = $buildReceipt.toolchains
    packaging_toolchains = [ordered]@{ wix = ((& $wix.Source --version) -join "`n"); powershell = $PSVersionTable.PSVersion.ToString() }
    upgrade_code = $upgradeCode
    product_code = $productCode
    package_code = $packageCode
    machine_tools_installer = [ordered]@{ upgrade_code = $serverUpgradeCode; product_code = $serverProductCode; package_code = $serverPackageCode; provisions_dependencies = $false; starts_services = $false }
    signing_status = 'unsigned'
    capabilities = @('per-user-client')
    first_party_executables = $Executables
    native_stack_bundled = $false
    payload_files = @(Get-ChildItem -LiteralPath $StageDir -Recurse -File | Sort-Object FullName | ForEach-Object {
        [ordered]@{ name = (Get-RelativePath $StageDir $_.FullName).Replace('\', '/'); sha256 = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant() }
    })
    artifacts = @($artifacts)
}
$manifestPath = Join-Path $DistDir "$($Identity.asset_prefix).manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 $manifestPath
$manifestHash = (Get-FileHash -Algorithm SHA256 $manifestPath).Hash.ToLowerInvariant()
"$manifestHash  $(Split-Path -Leaf $manifestPath)" | Set-Content -Encoding ascii "$manifestPath.sha256"
@($artifacts | ForEach-Object { "$($_.sha256)  $($_.name)" }) + @("$manifestHash  $(Split-Path -Leaf $manifestPath)") | Set-Content -Encoding ascii (Join-Path $DistDir 'SHA256SUMS')

Write-Host "Built $ZipPath"
Write-Host "Built $MsiPath"
Write-Host "Built $ServerToolsMsiPath (native dependency installation/acceptance still required)"
