# Native prepare-only bootstrap launcher

`memory-windows-evaluation-helper.exe --bootstrap-prepare DESCRIPTOR --sha256 SHA256
--output NEW_PRIVATE_UUID_DIRECTORY` supplies the missing trusted outer Job for
`rehearse.mjs --native --prepare-only`. The helper starts the reviewed Node
executable suspended, assigns its kill-on-close Job before resuming it, supplies
only an explicit environment, and terminates/reaps that whole Job before
accepting a prepare-only summary. A direct, uncontained Node launch still fails.

This entrypoint cannot launch a full campaign or accept arbitrary arguments,
environment variables, shell commands or provider settings. Native campaign
execution remains guarded pending native evidence. Preparation does not access a
database or provider, approve labels, or establish native agent isolation.

The operator must already have a current helper built with
`windows-evaluation-helper`, a private reviewed native execution profile, its
private immutable toolchains, and a private copy of the entire `evals/r3` source
tree. All regular files in that source tree must be inventoried. The existing
native inventory validator rejects missing/extra files, reparse points,
alternate streams and aliases, and holds the source handles for the launch.
Source and I/O trees must be disjoint. The execution profile's helper pin must
match the helper being invoked, so refresh that reviewed pin after rebuilding it.

The following PowerShell recipe runs on the Windows test host; paths and the
disposable database value must be set to the exact reviewed local fixture. It
does not create or change permissions on an existing directory.

```powershell
$Source = 'C:\MemoryLab\private\source\evals\r3'
$ExecutionPath = 'C:\MemoryLab\private\execution.json'
$Memory = 'C:\MemoryLab\private\tools\memory.exe'
$Snapshot = 'C:\MemoryLab\private\tools\r3_snapshot.exe'
$Score = 'C:\MemoryLab\private\tools\r3_score.exe'
$Retriever = 'C:\MemoryLab\private\tools\r3_retriever.exe'
$Compiler = 'C:\MemoryLab\private\rust\bin\rustc.exe'
$DisposableDatabase = 'postgresql://TEST_USER:TEST_PASSWORD@127.0.0.1:5432/memory_test'

$Execution = Get-Content -LiteralPath $ExecutionPath -Raw | ConvertFrom-Json
$IoRoot = $Execution.profile.io_root
$Destination = Join-Path (Join-Path $IoRoot ([guid]::NewGuid().ToString())) 'pack'
$Output = Join-Path $IoRoot ([guid]::NewGuid().ToString())
$DescriptorPath = Join-Path $IoRoot ('bootstrap-' + [guid]::NewGuid().ToString() + '.json')
$Pin = {
    param($Path)
    @{ path = [IO.Path]::GetFullPath($Path)
       sha256 = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
}
$Files = @(Get-ChildItem -LiteralPath $Source -Recurse -File -Force |
    ForEach-Object { & $Pin $_.FullName })
$Descriptor = @{
    schema_version = 1
    profile = 'memory-r3-native-bootstrap-prepare-v1'
    node = $Execution.profile.node
    source = @{ root = $Source; files = $Files }
    execution = & $Pin $ExecutionPath
    destination = $Destination
    compiler = $Compiler
    binaries = @{ memory = $Memory; snapshot = $Snapshot; score = $Score; retriever = $Retriever }
    database_url = $DisposableDatabase
    one_family_per_track = $true
    deadline_ms = 120000
}
$Bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Descriptor | ConvertTo-Json -Depth 12))
$File = [IO.File]::Open($DescriptorPath, [IO.FileMode]::CreateNew,
    [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $File.Write($Bytes, 0, $Bytes.Length); $File.Flush($true) } finally { $File.Dispose() }
$DescriptorHash = (Get-FileHash -LiteralPath $DescriptorPath -Algorithm SHA256).Hash.ToLowerInvariant()
& $Execution.profile.helper.path --bootstrap-prepare $DescriptorPath --sha256 $DescriptorHash --output $Output
if ($LASTEXITCODE -ne 0) { throw 'Native preparation failed; retain private logs and do not repeat automatically.' }
```

Both UUID directories must be new siblings below `profile.io_root`: one receives
the pack, the other receives bounded private `stdout.bin`/`stderr.bin` logs. The
launcher accepts at most a 120-second deadline and 8 MiB of combined diagnostics;
these are observed output bounds, not a disk quota. It retains failed private
artifacts. Never repeat the same destination after an uncertain result.

Success returns `memory-r3-native-bootstrap-prepare-result-v1` with
`state: prepared`, exact descriptor/Node/execution/log digests,
`outer_job_reaped: true`, `execution_started: false`, and
`official_evidence: false`. This result confirms pack preparation only. Existing
external reviewed toolchains remain dependencies of the prepared pack.

Native fixture coverage is opt-in: set `MEMORY_R3_BOOTSTRAP_HELPER`,
`MEMORY_R3_BOOTSTRAP_DESCRIPTOR`, `MEMORY_R3_BOOTSTRAP_SHA256`,
`MEMORY_R3_BOOTSTRAP_OUTPUT`, and `MEMORY_R3_REQUIRE_NATIVE_BOOTSTRAP=1`, then run
the `windows_evaluation_helper::bootstrap::tests::native_configured_bootstrap_prepares_through_real_helper_outer_job`
test in the native `mem-platform` test binary. Each actual fixture invocation
needs fresh descriptor/destination/output identities.
