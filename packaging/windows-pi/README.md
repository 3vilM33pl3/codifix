# Native Pi lab proof preparation

The lab adapter reuses `crates/mem-supervisor-pi/scripts/build-windows.ps1`.
`scripts/windows/native-pi.json` binds the original source archive, lockfile,
compiler, existing no-tools correction, Windows broker patch and strict proof
inputs. The original source and Linux artifacts are not modified.

**Current blocker:** pinned upstream `src/lib.rs:20` forbids unsafe code, but
`windows-broker-1.patch` introduces an unsafe `File::from_raw_handle` block.
The minimal compiler reproduction rejects that block before linking. Native
build and execution are not verified; preserve the unsafe prohibition when
repairing the portability patch.

After the portability repair is reviewed and manifest hashes are refreshed,
run from the disposable `MEMORY-WINLAB` VM only:

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

The three fake-provider cases cover HTTP 200 with default and disabled tool
tiers, then HTTP 429 without retry. They validate the original no-tools/RPC
contract using the shared CA proof validators, with no real provider credentials
or calls. This direct synthetic loopback proof does **not** verify AppContainer,
inherited broker transport or the deployed supervisor. Those require separate
native acceptance evidence.
