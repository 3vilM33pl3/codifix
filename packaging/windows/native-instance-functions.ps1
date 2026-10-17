# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure validation helpers are loaded by tests without SCM/admin side effects.
function Set-NativePrivateDirectory {
    param([string]$Path, [string]$ServiceSid = '', [switch]$ReadOnly, [switch]$SharedAncestor)
    $preserved = @()
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        while ($null -ne $item) {
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse points forbidden in operator paths' }
            $parent = Split-Path -Parent $item.FullName
            $item = if ($parent -and $parent -ne $item.FullName) { Get-Item -LiteralPath $parent -Force } else { $null }
        }
        $existing = Get-Acl -LiteralPath $Path
        $operatorSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $writers = @('S-1-5-18', 'S-1-5-32-544', $operatorSid)
        if ($ServiceSid -and -not $ReadOnly -and -not $SharedAncestor) { $writers += $ServiceSid }
        if ($existing.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $writers) { throw 'Unexpected directory owner' }
        $writeMask = [Security.AccessControl.FileSystemRights]::Write -bor [Security.AccessControl.FileSystemRights]::Delete -bor [Security.AccessControl.FileSystemRights]::DeleteSubdirectoriesAndFiles -bor [Security.AccessControl.FileSystemRights]::ChangePermissions -bor [Security.AccessControl.FileSystemRights]::TakeOwnership
        foreach ($rule in $existing.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -eq 'Allow' -and ($rule.FileSystemRights -band $writeMask) -ne 0 -and $rule.IdentityReference.Value -notin $writers) { throw 'Unexpected directory writer' }
            # Keep only exact previously installed service read ACEs on shared
            # ancestors. No inheritance into a sibling's state or authority.
            if ($SharedAncestor -and -not $rule.IsInherited -and $rule.AccessControlType -eq 'Allow' -and
                $rule.IdentityReference.Value -match '^S-1-5-80-(\d+-){4}\d+$' -and
                $rule.InheritanceFlags -eq 'None' -and $rule.PropagationFlags -eq 'None' -and
                ($rule.FileSystemRights -band $writeMask) -eq 0) { $preserved += $rule }
        }
    } else { New-Item -ItemType Directory -Path $Path | Out-Null }
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    foreach ($rule in $preserved) { $acl.AddAccessRule($rule) }
    if ($ServiceSid) {
        if ($ServiceSid -notmatch '^S-1-5-80-(\d+-){4}\d+$') { throw 'Exact virtual-service SID required' }
        $rights = if ($ReadOnly -or $SharedAncestor) { 'ReadAndExecute' } else { 'Modify' }
        $inherit = if ($SharedAncestor) { 'None' } else { 'ContainerInherit,ObjectInherit' }
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($ServiceSid), $rights, $inherit, 'None', 'Allow'))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Get-NativeInstancePlan {
    param([string]$Name, [string]$ReleaseIdentity, [string]$ExecutableName,
        [string]$HealthUri, [guid]$ExpectedInstanceId,
        [guid]$ExpectedSpaceId = [guid]::Empty, [guid]$StatusAudienceId = [guid]::Empty)
    if ($Name -cnotmatch '^[a-z][a-z0-9-]{0,47}$') { throw 'Use a lowercase instance name, maximum 48 characters' }
    if ($ReleaseIdentity -notmatch '^windows-v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?-build\.\d+$') { throw 'Invalid Windows release identity' }
    if ($ExecutableName -notin @('memory.exe', 'memory-federation-gateway.exe','memory-pds-pipe.exe')) { throw 'Unsupported service executable' }
    if ($ExpectedInstanceId -eq [guid]::Empty) { throw 'An exact expected instance UUID is required' }
    if($ExecutableName -ceq 'memory-pds-pipe.exe'){
        if($HealthUri -or $ExpectedSpaceId -ne [guid]::Empty -or $StatusAudienceId -ne [guid]::Empty){throw 'Native PDS readiness uses its protected local pipe, not HTTP/gateway bindings'}
        return [pscustomobject][ordered]@{service_name="MemoryLayer-$Name";account="NT SERVICE\MemoryLayer-$Name";name=$Name
            release_identity=$ReleaseIdentity;executable_name=$ExecutableName;health_uri=$null
            expected_instance_id=$ExpectedInstanceId.ToString();readiness_mode='native_pipe_public_xrpc_v1'}
    }
    if ($ExecutableName -eq 'memory-federation-gateway.exe') {
        if ($HealthUri -or $ExpectedSpaceId -eq [guid]::Empty -or $StatusAudienceId -eq [guid]::Empty) { throw 'Gateway needs exact space/authority/audience, not a Memory health URL' }
        return [pscustomobject][ordered]@{
            service_name = "MemoryLayer-$Name"; account = "NT SERVICE\MemoryLayer-$Name"; name = $Name
            release_identity = $ReleaseIdentity; executable_name = $ExecutableName; health_uri = $null
            expected_instance_id = $ExpectedInstanceId.ToString(); expected_space_id = $ExpectedSpaceId.ToString()
            status_audience_id = $StatusAudienceId.ToString(); readiness_mode = 'signed_loopback_status_v1'
        }
    }
    if ($ExpectedSpaceId -ne [guid]::Empty -or $StatusAudienceId -ne [guid]::Empty) { throw 'Gateway bindings do not apply to a Memory service' }
    $uri = [uri]$HealthUri
    if (-not $uri.IsAbsoluteUri -or $uri.Scheme -ne 'http' -or -not $uri.IsLoopback -or
        $uri.UserInfo -or $uri.Query -or $uri.Fragment -or $uri.AbsolutePath -cne '/healthz') {
        throw 'Readiness must be a credential-free loopback HTTP /healthz URL'
    }
    if ($ExpectedInstanceId -eq [guid]::Empty) { throw 'An exact expected instance UUID is required' }
    return [pscustomobject][ordered]@{
        service_name = "MemoryLayer-$Name"
        account = "NT SERVICE\MemoryLayer-$Name"
        name = $Name
        release_identity = $ReleaseIdentity
        executable_name = $ExecutableName
        health_uri = $uri.AbsoluteUri
        expected_instance_id = $ExpectedInstanceId.ToString()
    }
}

function Get-NativeServiceArguments([string]$ExecutableName, [string]$ConfigPath) {
    if ($ExecutableName -eq 'memory.exe') { return @('--config', $ConfigPath, 'service', 'run') }
    if ($ExecutableName -eq 'memory-federation-gateway.exe') { return @($ConfigPath) }
    if ($ExecutableName -eq 'memory-pds-pipe.exe') { return @($ConfigPath) }
    throw 'Unsupported service executable'
}

function Assert-NativePdsServiceConfig($Config,$Runtime,$RuntimePlan,[string]$Name,[string]$State,[string]$ConfigurationDirectory,[string]$EnvironmentSha256){
    Assert-NativeRuntimeFields $Config @('schema_version','pipe','node','node_sha256','entry','entry_sha256','helper_sha256','environment')
    Assert-NativeRuntimeFields $Config.pipe @('name','server_sid','client_sid')
    Assert-NativeRuntimeFields $Config.environment @('path','sha256')
    if($Config.schema_version -ne 1 -or $Runtime.runtime_kind -cne 'memory-native-pds-runtime' -or
        $Config.pipe.name -cne "\\.\pipe\memory-pds-$Name" -or
        $Config.pipe.server_sid -cnotmatch '^S-1-5-80-(?:\d+-){4}\d+$' -or
        $Config.pipe.client_sid -cnotmatch '^S-1-5-80-(?:\d+-){4}\d+$' -or $Config.pipe.server_sid -ceq $Config.pipe.client_sid -or
        $Config.node -cne $Runtime.node_executable -or $Config.entry -cne $Runtime.pds_entrypoint -or
        $Config.environment.path -cne (Join-Path $ConfigurationDirectory 'pds-environment.json') -or
        $EnvironmentSha256 -cnotmatch '^[0-9a-f]{64}$' -or $Config.environment.sha256 -cne $EnvironmentSha256){throw 'Native PDS config differs from exact runtime, service/pipe or protected environment'}
    $pins=Assert-NativePdsPathPatch $RuntimePlan.path_patch
    foreach($pair in @(@('node/node.exe','node_sha256'),@('pds/pds-server.mjs','entry_sha256'),@('pds/http-stdio.mjs','helper_sha256'))){
        $member=@($RuntimePlan.payload_files | Where-Object {$_.name -ceq $pair[0]})
        if($member.Count -ne 1 -or $Config.($pair[1]) -cne $member[0].sha256){throw 'Native PDS executable/adapter digest differs from pinned runtime'}
    }
}
function Assert-NativePdsEnvironment($Environment,[string]$State){
    $names=@($Environment.PSObject.Properties.Name)
    if($names.Count -lt 1 -or $names.Count -gt 64){throw 'Bounded explicit PDS environment required'}
    foreach($name in $names){
        if($name -cnotmatch '^PDS_[A-Z0-9_]+$' -or $name.Length -gt 128 -or $Environment.$name -isnot [string] -or
            $Environment.$name.Length -gt 8192 -or $Environment.$name.Contains([char]0)){throw 'Only bounded PDS environment strings are accepted'}
    }
    if($Environment.PDS_DATA_DIRECTORY -cne (Join-Path $State 'data') -or
        $Environment.PDS_BLOBSTORE_DISK_LOCATION -cne (Join-Path $State 'data\blobs') -or
        $Environment.PDS_DEV_MODE -cne 'false' -or $Environment.PDS_DISABLE_SSRF_PROTECTION -cne 'false' -or
        $Environment.PDS_INVITE_REQUIRED -cne 'true' -or -not $Environment.PDS_HOSTNAME -or
        -not $Environment.PDS_JWT_SECRET -or -not $Environment.PDS_ADMIN_PASSWORD -or
        -not $Environment.PDS_PLC_ROTATION_KEY_K256_PRIVATE_KEY_HEX){throw 'PDS state must be service-local with required credentials and unchanged protection policies'}
}
function Test-NativePdsReadiness($Ready,[string]$ServerSid,[string]$ClientSid,[string]$PipeName){
    try{
        $observed=[DateTimeOffset]::Parse($Ready.observed_at);$now=[DateTimeOffset]::UtcNow
        return $Ready.schema_version -eq 1 -and $Ready.server_sid -ceq $ServerSid -and $Ready.client_sid -ceq $ClientSid -and
            $Ready.pipe_name -ceq $PipeName -and $Ready.readiness_scope -ceq 'verified_pipe_peer_and_public_xrpc_only' -and
            $observed -ge $now.AddSeconds(-30) -and $observed -le $now.AddSeconds(1)
    }catch{return $false}
}

function Assert-NativeGatewayConfig($Config, [guid]$Space, [guid]$Authority, [guid]$Audience) {
    if ([guid]$Config.space -ne $Space -or [guid]$Config.authority -ne $Authority -or
        @($Config.credentials | Where-Object { [guid]$_.instance -eq $Audience }).Count -ne 1) {
        throw 'Gateway configuration differs from the reviewed space, authority or enrolled status audience'
    }
}

function Test-NativeGatewayReadiness($Ready, [guid]$Space, [guid]$Authority, [guid]$Audience) {
    try {
        $now = [DateTimeOffset]::UtcNow; $observed = [DateTimeOffset]::Parse($Ready.observed_at)
        return $Ready.schema_version -eq 1 -and [guid]$Ready.space -eq $Space -and
            [guid]$Ready.authority -eq $Authority -and [guid]$Ready.audience -eq $Audience -and
            $Ready.readiness_scope -ceq 'signed_identity_current_pull_and_writer_fence_only' -and
            $observed -le $now.AddSeconds(1) -and $observed -ge $now.AddSeconds(-30)
    } catch { return $false }
}

function ConvertTo-NativeCommandArgument([string]$Value) {
    if ($Value.Contains([char]0)) { throw 'NUL is forbidden in native arguments' }
    # .NET/CommandLineToArgvW syntax; never pass this through a shell.
    return '"' + ([regex]::Replace($Value, '(\\*)"', '$1$1\"') -replace '(\\+)$', '$1$1') + '"'
}

function Assert-NativeArchiveName([string]$Name) {
    if (-not $Name -or $Name.Length -gt 240 -or $Name.Contains('\') -or $Name.Contains(':') -or
        $Name.StartsWith('/') -or $Name.Contains([char]0)) { throw 'Unsafe ZIP entry name' }
    foreach ($part in $Name.TrimEnd('/').Split('/')) {
        if (-not $part -or $part -in @('.', '..') -or $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Unsafe ZIP path component' }
    }
}

function New-NativeCanonicalArchive([string]$Root,[string]$Package,$PayloadFiles,
    [ValidateRange(1,65536)][int]$FileLimit=20000) {
    Add-Type -AssemblyName System.IO.Compression
    $assertPath={param([string]$Path)
        $current=[IO.Path]::GetFullPath($Path)
        while($current){
            if((Test-Path -LiteralPath $current) -and ((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){
                throw 'Canonical archive paths must not traverse reparse points'
            }
            $parent=[IO.Path]::GetDirectoryName($current);if($parent -eq $current){break};$current=$parent
        }
    }
    &$assertPath $Root;&$assertPath $Package
    if(-not $PayloadFiles -or @($PayloadFiles).Count -gt $FileLimit){throw 'Bounded complete archive inventory required'}
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    $stream=[IO.File]::Open($Package,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
    try{
        $archive=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$true)
        try{
            $total=[int64]0
            foreach($item in $PayloadFiles){
                # .NET Framework CreateFromDirectory can emit Windows '\' names.
                # Inventory and file-only ZIP names are explicitly canonical.
                $entryName=([string]$item.name).Replace('\','/');Assert-NativeArchiveName $entryName
                if($entryName -cne $item.name -or $entryName.EndsWith('/') -or -not $seen.Add($entryName) -or $item.sha256 -cnotmatch '^[0-9a-f]{64}$'){
                    throw 'Duplicate, noncanonical or malformed archive inventory entry'
                }
                $path=Join-Path $Root $entryName.Replace('/',[IO.Path]::DirectorySeparatorChar);&$assertPath $path
                $inputFile=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
                try{
                    $total+=$inputFile.Length;if($total -gt 2GB){throw 'Expanded archive exceeds 2GiB'}
                    $sha=[Security.Cryptography.SHA256]::Create()
                    try{$hash=([BitConverter]::ToString($sha.ComputeHash($inputFile))).Replace('-','').ToLowerInvariant()}finally{$sha.Dispose()}
                    if($hash -cne $item.sha256){throw 'Payload changed before ZIP publication'}
                    $inputFile.Position=0
                    $entry=$archive.CreateEntry($entryName,[IO.Compression.CompressionLevel]::Optimal);$output=$entry.Open()
                    try{$inputFile.CopyTo($output)}finally{$output.Dispose()}
                }finally{$inputFile.Dispose()}
            }
        }finally{$archive.Dispose()}
        $stream.Flush($true)
    }finally{$stream.Dispose()}
}

function Get-NativeInputDigest($Value) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 12 -Compress))))).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}

function Assert-NativeRetry($Existing, [string]$InputDigest) {
    if ($Existing.input_sha256 -ne $InputDigest) { throw 'Operation ID is already bound to different inputs' }
}

function Test-NativeReadinessHealth($Health, [guid]$ExpectedAreaId) {
    return $Health.status -eq 'ok' -and $Health.area_instance_id -eq $ExpectedAreaId.ToString()
}

function Write-NativeJournal([string]$Path, $Value) {
    # Caller must provide an already ACL-protected, exclusively locked directory.
    $temporary = Join-Path (Split-Path -Parent $Path) ([guid]::NewGuid().ToString() + '.tmp')
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 16))
    $file = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary, $Path, [System.Management.Automation.Language.NullString]::Value) } else { [IO.File]::Move($temporary, $Path) }
}

function Assert-NativePayload([string]$Root, $PayloadFiles) {
    $files = @(Get-ChildItem -LiteralPath $Root -Recurse -File -Force)
    if ($files.Count -ne @($PayloadFiles).Count) { throw 'Release payload count differs from manifest' }
    $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $PayloadFiles) {
        Assert-NativeArchiveName $entry.name
        if (-not $names.Add($entry.name) -or $entry.sha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'Duplicate or malformed payload digest' }
        $target = Join-Path $Root $entry.name.Replace('/', [IO.Path]::DirectorySeparatorChar)
        if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw 'Release payload differs from manifest' }
    }
}

function Initialize-NativeReleaseGuard {
    if ('MemoryLayer.ProtectedRelease' -as [type]) { return }
    # Handle-held validation prevents file replacement/content writes between
    # inspection, hashing and SCM startup. This is not pathname-only Get-Acl.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using Microsoft.Win32.SafeHandles;
namespace MemoryLayer {
    public sealed class ProtectedRelease : IDisposable {
        readonly List<SafeFileHandle> handles = new List<SafeFileHandle>();
        readonly string caller = WindowsIdentity.GetCurrent().User.Value;
        const uint ReadControl = 0x20000, GenericRead = 0x80000000;
        [StructLayout(LayoutKind.Sequential)] struct Info {
            public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh,
                WriteLow, WriteHigh, Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
        }
        [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
        static extern SafeFileHandle CreateFileW(string path, uint access, uint share,
            IntPtr security, uint disposition, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool GetFileInformationByHandle(SafeFileHandle file, out Info info);
        [DllImport("advapi32.dll")]
        static extern uint GetSecurityInfo(SafeFileHandle file, int type, uint fields,
            out IntPtr owner, IntPtr group, out IntPtr dacl, IntPtr sacl, out IntPtr descriptor);
        [DllImport("advapi32.dll")] static extern uint GetSecurityDescriptorLength(IntPtr descriptor);
        [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
        static void Check(bool valid, string message) { if (!valid) throw new IOException(message); }
        bool Trusted(string sid, bool ancestor) {
            return sid == caller || sid == "S-1-5-18" || sid == "S-1-5-32-544" ||
                (ancestor && sid == "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464");
        }
        bool Hold(string path, bool ancestor) {
            // Ancestors deny deletion but allow unrelated sibling operations.
            // Payload files/directories additionally deny write-sharing.
            var handle = CreateFileW(path, GenericRead | ReadControl,
                ancestor ? 3u : 1u, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero);
            if (handle.IsInvalid) { handle.Dispose(); throw new Win32Exception(Marshal.GetLastWin32Error(), "Protected release handle unavailable"); }
            handles.Add(handle);
            Info info;
            Check(GetFileInformationByHandle(handle, out info), "Release identity unavailable");
            Check((info.Attributes & 0x400) == 0, "Release path is a reparse point");
            bool directory = (info.Attributes & 0x10) != 0;
            Check(directory || info.Links == 1, "Release file has multiple hard links");
            IntPtr owner, dacl, descriptor;
            uint result = GetSecurityInfo(handle, 1, 5, out owner, IntPtr.Zero, out dacl, IntPtr.Zero, out descriptor);
            Check(result == 0, "Release security information unavailable");
            try {
                uint length = GetSecurityDescriptorLength(descriptor);
                Check(length > 0 && length <= 65536, "Release descriptor exceeds bounds");
                byte[] bytes = new byte[length]; Marshal.Copy(descriptor, bytes, 0, (int)length);
                var security = new RawSecurityDescriptor(bytes, 0);
                Check(security.Owner != null && Trusted(security.Owner.Value, ancestor), "Release owner is not trusted");
                Check(security.DiscretionaryAcl != null, "Release has a null DACL");
                uint dangerous = ancestor ? 0x500D0152u : 0x500D0156u;
                foreach (GenericAce generic in security.DiscretionaryAcl) {
                    if ((generic.AceFlags & AceFlags.InheritOnly) != 0) continue;
                    var ace = generic as CommonAce;
                    Check(ace != null && !ace.IsCallback, "Release uses an unsupported ACL policy");
                    if (ace.AceQualifier == AceQualifier.AccessDenied) continue;
                    Check(ace.AceQualifier == AceQualifier.AccessAllowed, "Release uses an unsupported ACE");
                    Check(Trusted(ace.SecurityIdentifier.Value, ancestor) || ((uint)ace.AccessMask & dangerous) == 0,
                        "Release is writable or replaceable by another principal");
                }
            } finally { if (descriptor != IntPtr.Zero) LocalFree(descriptor); }
            return directory;
        }
        public static ProtectedRelease Open(string root) {
            var result = new ProtectedRelease();
            try {
                root = Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar);
                var ancestors = new Stack<string>();
                string parent = Path.GetDirectoryName(root);
                while (!String.IsNullOrEmpty(parent)) {
                    ancestors.Push(parent);
                    string next = Path.GetDirectoryName(parent);
                    if (next == parent) break;
                    parent = next;
                }
                foreach (string ancestor in ancestors) Check(result.Hold(ancestor, true), "Release ancestor is not a directory");
                Check(result.Hold(root, false), "Release root is not a directory");
                var pending = new Queue<string>(); pending.Enqueue(root);
                int count = 0;
                while (pending.Count > 0) {
                    foreach (string entry in Directory.EnumerateFileSystemEntries(pending.Dequeue())) {
                        Check(++count <= 40000, "Release tree exceeds entry bound");
                        if (result.Hold(entry, false)) pending.Enqueue(entry);
                    }
                }
                return result;
            } catch { result.Dispose(); throw; }
        }
        public void Dispose() {
            for (int i = handles.Count - 1; i >= 0; i--) handles[i].Dispose();
            handles.Clear();
        }
    }
}
'@
}
