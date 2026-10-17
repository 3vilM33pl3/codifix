# Codifix 3 packaging source validation

Input source: `a6aadd37`. Packaging implementation commits: `55526094` and
`e027b994`. Tests ran in the isolated packaging worktree on Linux; no Cargo
build, installation, live deployment, SSH operation, publication or service
activation was performed. These are source-contract results, not native
installer or populated-cutover acceptance.

## Results

| Check | Result |
| --- | --- |
| Debian lifecycle/named-instance/package staging (`bash packaging/tests/test-debian-lifecycle.sh`) | Pass; repeated after final source changes |
| Bookworm Python suite (`python3 -m unittest discover -s packaging/ca-bookworm -p 'test_*.py'`) | 99 tests, pass, 2 explicit native skips; repeated after final source changes |
| Named-instance launcher (`python3 packaging/tests/test-instance-launcher.py`) | 3 tests pass |
| CA read-only preflight (`python3 packaging/tests/test-ca-preflight.py`) | 13 tests pass |
| Release manifests (`test_release.py`) | 18 tests pass |
| Release instances (`test_instance.py`) | 21 tests pass |
| Console/unit source digest (`test_console.py`) | 26 tests pass after regenerating the reviewed unit digest |
| Release upgrade (`test_upgrade.py`) | 18 tests pass |
| Deploy federation and enrollment plus PDS stdio/path contracts (`node --test deploy/federation/*.test.mjs deploy/federation/pds-enrollment/*.test.mjs packaging/windows-pds/windows-paths.test.mjs packaging/windows-pds/http-stdio.test.mjs`) | 96 tests, 94 pass, 2 skips requiring the exact pinned PDS source tree |
| Company topology/bootstrap after example-host rename | 11 tests pass |
| macOS package payload/guards (`test_package_contract.py`) | 23 tests pass; actual staged executable is regular, no old/extra CLI aliases, new receipt/helper identities |
| macOS first-run (`test_onboarding_first_run.py`) | 13 tests pass after correcting the test fixture's internal kwargs lookup |
| Initial full macOS Python suite | 147 tests; 146 passed, one test-fixture error loaded before the correction above. Focused 13-test suite subsequently passed. No final full-suite green result is claimed here |
| Windows Pi smoke | 2 tests pass; source-binding test blocked by root-owned `scripts/windows/native-pi.json` still containing pre-rename paths/digests in this isolated worktree. Regenerate after integration |
| Source syntax | 15 shell sources pass `bash -n`; deployment TOML and release YAML parse; both Ruby formula sources pass `ruby -c` |
| Wrapper/source pin consistency | Current PDS stdio/path-wrapper digests match production native-prerequisite guards and their fixtures |

An unrelated pre-existing `hserver.service` unknown `SideFX` section warning
appeared during the Debian test's systemd source verification; the test passed.

## Preserved evidence and explicit exceptions

Byte equality against the input commit was checked for:

- `deploy/areas/CA-BOOKWORM-CANDIDATE.md`
- `deploy/areas/CA-CONSOLE-HANDOFF.md`
- `deploy/federation/ca-watched-agent-rc4-handoff.md`
- `packaging/windows-pds/upstream/package.json`
- `packaging/windows-pds/upstream/pnpm-lock.yaml`
- `packaging/macos/runtime-sources.json`

Vendored upstream PDS sources/patches were excluded from transformation. Existing
Pi upstream/helper package and lock pins remain unchanged as an explicit
integration exception. Existing database names/roles, semantic memory fields,
resource-limit names and the exact `MEMORY-WINLAB` host guard remain. Legacy
spellings in the reinstall guide and negative alias assertions are intentional.
The new source-format names do not reinterpret old signed release artifacts.

## Integration and native gates

The integrator owns root Windows manifest/helper path/digest regeneration,
PDS path-wrapper pins in `scripts/windows/guest-pds-patched-runtime.mjs` and
`scripts/windows/test-runtime-pack.ps1`, and aggregate asset validation for the
RC build-1/stable build-2 Windows ledger. Re-run the binding tests after that
integration.

PowerShell was unavailable in this Linux worktree. Windows native/MSI and
operator suites remain unexecuted here. Neither architecture's Mac
signing/notarization/install/Keychain transfer nor Linux native package install,
ABI build, populated old-name cutover or matched restoration was performed.
Homebrew remains HEAD-only until the actual Codifix frozen source archive has a
verified checksum. The explicit platform reinstall procedure is
[CODIFIX-3-REINSTALL.md](CODIFIX-3-REINSTALL.md).

## Reviewed source digests

These SHA-256 values identify source files, not generated release packages:

```text
233a546a81a9abda71b51dc909eb51059ff4d3a96309285e85ab1e638cf0c989  packaging/build-deb.sh
df9db4d831d5b85061f6fd0b1ac8c63c0456b47d96be4903428224daacff4c48  packaging/build-pkg.sh
a9329d17de75fe33ca84ef69f3ea21c45d9ddc92ee67b2d201966e09e6e5696a  packaging/windows/build-windows.ps1
7c5c9f1834a64773185feb4cbd96f9105db7793996e80f286043eece8cbd1334  packaging/debian/codifix@.service
9d7a04af99b878e4afdcce064e7c64b4a316bee345c96dfc14fce2ad14e481a2  packaging/windows-pds/http-stdio.mjs
79c8c8378fdab250457e663d2d48518cb28dbabf5a81b3ae832aa70a7b860441  packaging/windows-pds/windows-paths.mjs
1e8c009c6b895477cbd94eb1f61bbd16f270a841b43c713ae6e8a51f9d0c0b58  scripts/areas/releases/release.py
7d9a56780df95c05c6a2db765027be02f7f05123fbcee90e5451391ad59d5e34  scripts/areas/releases/console.py
2663a5cc23c15ebda396edd07c522a7c7add6622f4c0dc9a67bd780040192aef  .github/workflows/release.yml
5e687dafca1df222bdf0ce9d47dffbec7e934efe74cb95fc80d53947d31cbfa3  Formula/codifix.rb
```
