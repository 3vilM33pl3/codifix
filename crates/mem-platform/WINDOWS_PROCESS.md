# Native AppContainer process primitive

`windows_process::spawn` is a Windows-only credential-free process primitive.
It is not yet the Pi driver or evaluation adapter. Existing SCM service hosting
is unchanged.

The caller prepares a dedicated flat execution copy containing only the reviewed
executable and individually SHA-256-pinned runtime files, plus an initially empty
private scratch directory. Ordinary `mem-platform-files` validators inspect and
pin those objects **before** temporary sandbox grants. Authority originals,
credentials, databases and other areas must never enter this execution copy.

The primitive creates a fresh per-attempt AppContainer profile with zero
capabilities. Handle-based ACL changes grant that exact SID read/execute access
to the reviewed code copy and read/write access to scratch; scratch files do not
receive execute rights. Scratch gets a temporary low mandatory-integrity label:
a permissive DACL alone cannot authorize a low-integrity process to write a
medium-integrity object. See Microsoft's [mandatory integrity control](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control)
and [security-information access requirements](https://learn.microsoft.com/en-us/windows/win32/secauthz/security-information).

Only duplicated stdin/stdout/stderr and one already-admitted broker pipe handle
enter the `STARTUPINFOEX` handle list. There is no caller-provided environment:
only sandbox paths, the kernel-reported Windows directory and broker handle are
supplied. Provider credentials remain with the trusted broker. No network
capabilities or loopback exemptions are configured.
Windows' normal AppContainer-readable OS resources are not claimed to disappear;
the denial boundary is private application/authority data and explicitly granted
execution inputs, not an invented empty Windows filesystem.

Creation is suspended. Before any child code runs, the parent verifies the
AppContainer token, exact SID and empty capability set, attaches the process to a
kill-on-close Job Object, then resumes the initial thread. Limits are two active
processes, 512 MiB total job/process memory, at most one logical CPU and restricted
UI access. Breakaway is not enabled. The owning guard is not detachable.

Termination waits for the entire job to become empty before restoring original
ACLs/integrity and validating the pinned code again. Cleanup errors remain errors;
uncertain cleanup retains profile/pins/grants for operator repair rather than
claiming success or revoking resources beneath a possibly running process.
Retain `profile_name()` and the dedicated state path in the caller's operation
journal. `try_wait()` supports owner-thread cancellation loops without changing
the caller's real-time deadline.

## Native acceptance helper

The helper is excluded from default builds and release packages. In the Windows
VM, build and run it explicitly:

```powershell
cargo build --locked -p mem-platform --features windows-native-tests --bin memory-windows-isolation-probe
.\target\debug\memory-windows-isolation-probe.exe
```

It uses only synthetic files and a local TCP sentinel. It tests broker access,
scratch writes, denied unrelated-file reads, denied code writes and scratch
execution, exclusion of an otherwise inheritable authority handle, loopback
denial and denial of a third process. Evidence is retained in a uniquely named
private temporary directory. Repeat it under the real service host to test nested
Jobs and the intended Windows service identity.

Cross-compilation and Clippy with that feature pass; Linux's seven existing
platform tests pass. **No native runtime result is recorded yet.** Filesystem
escape variants, service-host nesting, resource exhaustion and broker/Pi protocol
acceptance remain required before this primitive is considered deployed isolation.
