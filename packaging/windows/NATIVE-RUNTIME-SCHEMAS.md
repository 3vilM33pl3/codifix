# Protected native runtime payloads

`native-prerequisites.ps1` installs immutable, already-built bytes. Its existing
Preview/Install confirmation, protected input directory, archive checks, complete
file hashes and durable operation receipts apply to each kind independently.
It never downloads dependencies, runs npm hooks, initializes state, creates
accounts/services or activates a supervisor. A successful receipt proves payload
installation, not native execution or complete-stack acceptance.

PostgreSQL's existing schema, destination and default consumer binding remain
unchanged. The two additional schema-version-1 kinds are Windows-x86_64 only;
unknown/missing fields and unpinned versions fail closed. Each includes
`crt_version`, `package_sha256`, a complete `payload_files` array of exact
`name`/`sha256` pairs, and component notices/provenance. Archive paths and
case-insensitive duplicates are checked. Operator state, credential files and
unlisted bytes are rejected. The exact reviewed dependency tree is retained,
including public library test/example fixtures; filename filtering is not a
secret detector or permission to add live credentials. Retain third-party terms; this
operator does not establish redistribution rights.

## Node/PDS: `memory-native-pds-runtime`

The descriptor requires `node_version: 24.18.1`, `pds_version: 0.5.27`,
`pds_source_revision`, `pds_image_digest`, `pds_package_sha256`,
`pds_lock_sha256`, `pnpm_version: 10.34.1`, `better_sqlite3_version: 12.11.1`,
`sqlite_version: 3.53.2` (the native engine), and
`native_addons`. The source/image/package/lock pins match the existing reviewed
native PDS dependency baseline; they are exact constants in the validator.

Payload layout:

- `node/node.exe` and app-local `vcruntime140.dll`, `vcruntime140_1.dll`,
  `msvcp140.dll`.
- `pds/package.json`, `pds/pnpm-lock.yaml`, and the exact reviewed
  `pds-server.mjs` / `http-stdio.mjs` adapters.
- Complete prepared `pds/node_modules/**`, including PDS `dist/index.js` and
  the native `better_sqlite3.node`. Every `.node` has an exact matching
  `native_addons` name/hash entry; no install-time compilation occurs.
- `notices/*.txt`, with exactly five component entries: `node`, `pds`,
  `locked-dependencies`, `memory-pds-adapter`, `msvc-crt`. Each has exact
  `name`, `version`, `source_sha256`, `license`, `notice_path` fields.

No `.env`, credentials, database, repository storage or PDS identity belongs in
operator-owned payload paths. Library fixtures are opaque reviewed dependency
bytes, not live state. Up to 65,536 files / 100,000 ZIP entries accommodate the
complete prepared tree, with a 32 MiB manifest bound. PostgreSQL/Pi retain their
smaller file/entry limits and 8 MiB manifest bound.
Runtime configuration and enrollment remain separate operations.

### Explicit Windows path-patched PDS profile

Baseline manifests remain byte-compatible. A patched manifest adds both
`original_prepared_sha256` and `path_patch`; neither is inferred from a version
number. The latter has exact `layout: windows-sha256-did-paths-v1`, the reviewed
`windows-paths.mjs` digest, and exactly two `name`/`original_sha256`/
`patched_sha256` entries (`actor-store/actor-store.js`, `disk-blobstore.js`).
Original upstream version/source/image/lock pins remain unchanged. Both patched
payload hashes must match the fixed reviewed outputs, and the patch script is
retained as `pds/windows-paths.mjs` with a separate AGPL notice/component.
No new wire identity, DID/CID format or upstream version is introduced.

The lab-only `scripts/windows/guest-runtime-pack.ps1 -Kind Pds` accepts
`-PdsProfilePath` plus `-PdsProfileSha256` to select the existing
`memory-native-pds-profile-v1` descriptor. Its exact preparation digest, original
preparation receipt, complete inventory and two-file delta are checked before
packaging. Changed unrelated files, altered patch metadata or an invalid selected
profile fail closed; they never fall back to the original runtime. No PDS is
started. A runtime package/installer receipt still does not prove account or
federation operation.

## Pi: `memory-native-pi-runtime`

The descriptor requires `pi_version: 0.5.1`, the complete Windows `artifact`
provenance object, `source_archive_sha256`, `upstream_lock_sha256`,
`rust_toolchain: nightly-2026-08-31`, `rustc_commit`, and
`target: x86_64-pc-windows-msvc`. The original source revision, no-tools patch,
broker patch, safe-handle patch/helper and original/patched lock digests must
match the existing pinned Windows Pi build; registry versions are not changed.

Payload layout:

- `pi/pi.exe` matching `artifact.binary_sha256` and the three app-local CRT DLLs.
- `source/pi-source.tar`, `no-tools-1.patch`, `windows-broker-1.patch`,
  `windows-broker-safe-handle-1.patch`, `windows-pipe-helper/Cargo.toml`,
  `windows-pipe-helper/src/lib.rs`, and the patched `Cargo.lock`.
- Component notices for `pi`, `memory-windows-pipe`, `msvc-crt`.
  `notices/pi.txt` must be the exact full pinned upstream licence, including
  its OpenAI/Anthropic rider; the descriptor labels it
  `MIT-with-OpenAI-Anthropic-Rider`, not plain MIT. The local helper is
  `AGPL-3.0-or-later`. Keep any distribution/legal assessment separate.

No sessions, models/settings, credentials or supervisor grants are included.
Installing Pi bytes grants neither execution authority nor model disclosure.

## Operator and consumer binding

Use the same explicit invocation for each bundle:

```powershell
./native-prerequisites.ps1 -Action Preview -OperationId UUID -PackagePath ZIP -ManifestPath JSON -ManifestSha256 SHA256
# Review the returned destination and input digest, then:
./native-prerequisites.ps1 -Action Install -OperationId UUID -PackagePath ZIP -ManifestPath JSON -ManifestSha256 SHA256 -ExpectedInputSha256 REVIEWED_DIGEST
```

Each manifest gets a distinct hash-identified runtime directory and receipt.
Repeated execution verifies the same bytes; changed inputs require a new
operation. Existing PostgreSQL consumers still request PostgreSQL by default.
A future PDS/Pi consumer must explicitly pass its exact `ExpectedKind` to
`Open-NativePrerequisiteLease`; another kind cannot satisfy that dependency.
The returned paths are protected payload bindings, not service or readiness
attestations. Wiring native service/sandbox consumers and native clean-machine
installation evidence remain separate work.

Portable checks: `test-native-prerequisites.ps1`,
`test-native-runtime-kinds.ps1`, and `test-native-postgres-plan.ps1`.

## CRT notice input and remaining distribution evidence

The lab packer requires an existing `-CrtNoticePath` and exact
`-CrtNoticeSha256`; it retains those bytes, without fetching or synthesizing
terms. DLL paths/hashes and the version come from the retained native PostgreSQL
`prepared.json` (`crt_source` and `runtime_support`), not a guessed VS version.
The installed lab Build Tools root is `C:\MemoryLab\BuildTools`.
`Licenses\1033\Redist.txt` there is the redistributable-file inventory, **not
verified full licensing/redistribution terms**. The complete applicable terms
have not yet been located/verified; the list alone must not be represented as
that evidence. No unpinned substitute or automatic download is permitted.

Node's retained notice is
`C:\MemoryLab\Tools\node-v24.18.1-win-x64\LICENSE`. PDS/dependency notices stay in
the complete prepared `node_modules` tree; the package adds a digest index of
those original files. Pi includes its exact original licence with rider and
the local helper's actual AGPL text. PostgreSQL's full notice remains an explicit
`-PostgresNoticePath` input; pgvector uses the retained source `LICENSE`.
Successful portable checks do not resolve distribution rights or establish a
completed all-stack installer.
