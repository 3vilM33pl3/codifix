# Native Pi lab proof preparation

The lab adapter reuses `crates/codifix-supervisor-pi/scripts/build-windows.ps1`.
`scripts/windows/native-pi.json` binds the original source archive, lockfile,
compiler, existing no-tools correction, Windows broker patch and strict proof
inputs. The original source and Linux artifacts are not modified.

The original Windows broker patch introduces unsafe handle adoption, which
conflicts with pinned Pi's `forbid(unsafe_code)`. A separate compatibility patch
replaces it with a dependency-free local helper. Only a successful non-inheritable
`DuplicateHandle` result becomes owned; `GetFileType` and `GetNamedPipeInfo`
require an actual pipe. The original handle is never closed. Parent-whitelisted
inheritance remains the authority check; this helper is not broker authentication.
Pi's unsafe prohibition, source pin and all registry versions remain unchanged.
The original and patched lock hashes, helper bytes and compatibility patch are
recorded separately. **Native build and execution remain unverified.**
The Windows-only artifact descriptor now requires these additional provenance
fields; old Windows candidates missing them fail closed. Existing Linux artifact
descriptors and binaries are unchanged.

The ownership/type checks follow Microsoft's
[DuplicateHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle)
and [GetNamedPipeInfo](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-getnamedpipeinfo)
contracts. This is a trusted-launcher input boundary, not a general socket or
arbitrary external-handle import API.

After reviewing the input manifest, run from the disposable `MEMORY-WINLAB` VM only:

```powershell
& C:\MemoryLab\source\scripts\windows\guest-native-pi.ps1 `
  -Action Prepare -ManifestPath C:\MemoryLab\source\scripts\windows\native-pi.json `
  -SourceRoot C:\MemoryLab\source -OperationId <new-operation-uuid>
```

Use `-Action Verify` with that same UUID after a successful build. A failed
partial build is preserved; do not retry it under another identity without
diagnosing the failure. Host tests check inputs and the shared strict protocol;
they do not execute Pi:

```sh
python3 -m unittest discover -s packaging/windows-pi -p 'test_*.py'
```

The builder first runs four native ownership-helper tests (null/pseudo/closed
handles, disk rejection, pipe exchange, duplicate noninheritance and preservation
of the original). Host Windows-target compilation is not their execution result.

The three fake-provider cases cover HTTP 200 with default and disabled tool
tiers, then HTTP 429 without retry. They validate the original no-tools/RPC
contract using the shared CA proof validators, with no real provider credentials
or calls. This direct synthetic loopback proof does **not** verify AppContainer,
inherited broker transport or the deployed supervisor. Those require separate
native acceptance evidence.
