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

function Write-NativeJournal([string]$Path, $Value) {
    # Caller must provide an already ACL-protected, exclusively locked directory.
    $temporary = Join-Path (Split-Path -Parent $Path) ([guid]::NewGuid().ToString() + '.tmp')
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 16))
    $file = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $file.Write($bytes, 0, $bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary, $Path, [System.Management.Automation.Language.NullString]::Value) } else { [IO.File]::Move($temporary, $Path) }
}
