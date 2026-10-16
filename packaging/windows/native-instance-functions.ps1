# SPDX-License-Identifier: AGPL-3.0-or-later
# Pure validation helpers are loaded by tests without SCM/admin side effects.
function Get-NativeInstancePlan {
    param([string]$Name, [string]$ReleaseIdentity, [string]$ExecutableName,
        [string]$HealthUri, [guid]$ExpectedInstanceId)
    if ($Name -cnotmatch '^[a-z][a-z0-9-]{0,47}$') { throw 'Use a lowercase instance name, maximum 48 characters' }
    if ($ReleaseIdentity -notmatch '^windows-v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?-build\.\d+$') { throw 'Invalid Windows release identity' }
    if ($ExecutableName -notin @('memory.exe', 'memory-federation-gateway.exe')) { throw 'Unsupported service executable' }
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
