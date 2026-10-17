# Native Codifix runtime candidate acceptance — 2026-10-17

Private native arm64 staged candidate built successfully from producer source at `cb609fdead64b8f34be9063af83945ff2aafbd20`. The build-runtime.py, runtime_bundle.py and runtime-sources.json SHA-256 values on the Mac independently match those exact Git objects. No rebuild was performed during acceptance.

Candidate: `py3148-pg1711-vector087-aarch64-codifix-v1`, exact intended prefix `/usr/local/share/codifix/runtime/py3148-pg1711-vector087-aarch64-codifix-v1`.

Manifest SHA-256: `d079d2e89026f22838f9cd847f104fb8c0489956387f14bef38c05f18712a7f3`.

## Checks performed on laptop.metatao.net

- Full protected-tree manifest validation passed before and after execution: 4,734 files, including SHA-256, modes, ownership, no unsupported aliases or extra inventory.
- Pinned source archives rehashed successfully; build-input journal matches exact prefix, aarch64 and reviewed source pins: OpenSSL 3.5.9, PostgreSQL 17.11, pgvector 0.8.7, Python 3.14.8.
- All 138 Mach-O files contain arm64 and pass `codesign --verify --strict`. All 191 dependency entries resolve to system libraries or existing internal `@loader_path` files. No Homebrew or old Memory-prefix dylib dependency accepted.
- Staged Python reports 3.14.8 and arm64. Isolated Python imports ctypes, sqlite3, hashlib, json, pathlib, subprocess, socket and plistlib; ctypes calls libSystem strlen successfully; SQLite arithmetic and SHA-256 known-answer checks pass. SQLite reports 3.51.0.
- postgres, initdb, pg_ctl, pg_dump, pg_restore and psql each report PostgreSQL 17.11.
- Fresh disposable cluster under private `/private/tmp/codifix-pg-probe-*`: socket-only server, empty listen_addresses, socket mode 0700, trust-local/reject-host authentication. Fresh non-superuser role/database; vector 0.8.7 installation, non-superuser pgcrypto and pg_trgm installation, digest/similarity/vector query all pass. Server stopped and temporary directory automatically removed.

Machine-readable sanitized evidence and the probe script are retained in the private rehearsal workspace and on the Mac; no database contents, credentials or raw build logs are included in this draft.

## Acceptance boundary

This verifies the fresh staged native runtime. It does not install or bind the runtime, modify live databases/services/Keychain/launchd, activate a candidate, or upload public artifacts. Signatures are ad hoc; Developer ID signing/notarization remains separate. The disposable cluster used the producer's C/SQL_ASCII fixture; full application schema/data/encoding preservation and installed exact-prefix behavior remain separate gates.

## Proposed next isolated rehearsal

Use the reviewed helper at f948e2ec/7e3acc89 in a private retained source layout and a fresh synthetic schema-2 fixture. Execute real Darwin preflight against this accepted runtime and a separately pinned native Codifix candidate. Populate genuine synthetic cluster roles/OIDs, migration metadata and device/cache identities; prepare independent reconciliation proof and archived journal/plists. Use only an isolated maintenance PostgreSQL and unique unused loopback ports; never select the live laptop root, labels, ports, credentials, subscription/device IDs or Keychain.

Exercise inspect → bind → clean maintenance stop → cold PostgreSQL launcher → area verify-only startup, retaining disabled inert plists and checking all file pins and stable cluster identity. Observe network behavior and reject authority/sync/watch paths. This would close native preflight, publication and real SQL parsing gaps; it still would not authorize or prove live state transfer/activation. Rehearsal has only been proposed here, not executed.
