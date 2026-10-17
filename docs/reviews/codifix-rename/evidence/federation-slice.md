# Federation namespace and retirement slice

Source baseline: `c81fcac1`. Implementation commits: `29573b7e`, `9c3b4eb3`,
`e4a56f7b`. Independent bounded review found and verified corrections for the
device-attestation egress guard and SQL JSON-null rebind validation.

The active Lexicon namespace, record-ID URL and signature purposes/domain use
Codifix together. Original signed evidence is unchanged. A separate historical
verifier accepts legacy signatures only through an explicitly retired space with
pinned receiver, authority, key schedule, final cursor and history digest.
Current local Pull grants and exact project permissions still control reads.
Durable fences prevent old startup, sync, retry, grant activation, publication,
device attestation and provider execution. Retained laptop views remain selectable;
the one-time guarded rebind preserves logical origins and old outbox bindings.

New migration85 adds immutable retirement/rebind receipts and their bounded
functions. Existing migrations are byte-identical to the baseline. It changes
only the defaults of `federation_source_changes.id` and `project_commits.id` to
`pg_catalog.gen_random_uuid()`; existing identifiers are untouched. The reviewed
schema fingerprint omits catalog identity and the two physical-column fields
`ordinal_position`/`dtd_identifier`, sorting columns by table/name. Types,
defaults, nullability, constraints, indexes, triggers and functions remain exact.
Cargo.lock adds only an existing local verifier test dependency to the service.

Validation passed on 2026-10-17:

- `cargo check -p codifix-federation-runtime -p codifix-service --all-targets --locked`.
- Focused ATProto/verifier suites: 42 tests; regenerated unsigned namespace/rkey
  goldens after the initial stale fixture failures.
- Runtime retirement integration: 1 test, including zero fake HTTP/model calls,
  legacy verification, unchanged retained device search, fresh enrollment/dedup,
  and preserved canonical origins during rebind.
- Service retained-context integration: 1 test covering current exact project
  scope, anonymous exclusion, Pull authorization and absent active registry.
- Python operator suite: 2 tests, including real disposable PostgreSQL schema/row
  drift rejection, exact retry, SQL null/wrong-owner rejection and immutable receipts.
- Formal project: `lake build` (21 jobs) and `lake test` from its directory.
  Lean source comparison confirms only module/package tokens changed.
- `cargo fmt --all --check`, `git diff --check`, parsed lockfile comparison and
  byte comparison of every existing migration.

A fresh disposable clone of the Workshop schema84 dump was upgraded to final85.
Its normalized semantic schema exactly matched an independently migrated pristine
candidate. Retirement and immediate exact retry preserved all 159 reviewed table
fingerprints. The clone contained 1 space, 23 inbox deliveries and 23 receipts;
the read-only Rust verifier successfully verified all 23 retained records/receipts.
The local manifest SHA256 is
`6e2449e968c24039de9aa7ba5134dae173d75ab8f46180abae947f67d1f62b13`.
Metadata manifests stay mode0600 under
`/home/olivier/.local/state/codifix-rename-20261017/` as
`federation-schema-reference-final.json` and
`federation-workshop-retire-rehearsal.json`. The explicit disposable test settings
are in `federation-test.env`; no credentials or payloads are committed here.

No live service, database, CA, peer or provider was mutated by this slice.
Consolidated workspace checks and the operational stop/drain/restart
cutover remain parent release gates. Original seed restores remain untouched.
The release operators document that old processes must be drained before applying
markers and only candidate binaries may restart afterwards.

## Additional populated snapshot rehearsals

Rehearsed General, Homelab and gateway on fresh isolated clones of the existing
seed dumps. Source was checked against integrated candidate `4eca6946`: the
migrations, retirement operators and historical verifier were unchanged from
the validated E slice. The existing read-only verifier binary was reused; no
Cargo build or live action was needed. Each seed's SHA256 was checked before
and after its rehearsal and stayed identical.

| Clone | Upgrade | Preserved table fingerprints | Retired spaces | Retained records / receipts verified |
| --- | --- | --- | --- | --- |
| General | 76 → 85 | 159 | 1 | 23 / 23 |
| Homelab | 84 → 85 | 159 | 1 | 23 / 23 |
| gateway | 76 → 85 | 159 | 1 | 0 / 0 (no replica history) |

For each clone, every applied source migration checksum matched before upgrade.
After upgrade, all migration checksums and the normalized semantic schema matched
an independently migrated pristine candidate. Retirement and exact retry passed,
and all reviewed preserved table fingerprints matched the pre-retirement values.
These fingerprints omit only the explicitly reviewed operational fields described
by the operator; they are evidence of preservation, not a functional grant,
authorization or citation test. Gateway's empty verifier result does not exercise
historical signatures. The focused synthetic/HTTP tests above cover authorization
and origin behavior separately.

Mode0600 local metadata is retained under the same state directory in
`federation-additional-schema-reference.json`,
`federation-additional-rehearsal-results.json` and the three
`federation-{general,homelab,gateway}-retire-rehearsal.json` manifests. Original
seed restores and dumps remain untouched; all upgraded/retired databases are
disposable clones in the existing loopback-only PostgreSQL test container.

## Populated Mac device-cache rehearsal

The two existing custom dumps and their manifest were copied read-only over the
authorized SSH connection from the completed Mac seed
`mac-rehearsal-seed-194738Z`. No new live dump, service/configuration/credential
change, archive extraction, provider/federation request or Cargo build occurred.
Only those three files were transferred, into a mode0700 local seed directory
with mode0600 files. Their SHA256 values match the prior inventory and the
unchanged remote files. The seed is a transactionally consistent dump per DB,
not a coordinated quiesced cross-database/offline/configuration recovery bundle.

Fresh clones were restored into the existing isolated PostgreSQL17.11 container
with vector available. Every applied migration1–84 checksum matched source.
After upgrade to85, both normalized semantic schemas and all migration checksums
matched the independent pristine candidate. Operators, migration source and the
reused read-only verifier source were unchanged through candidate `a147024b`.

| Cache | Original device records | Local canonicals / versions | Curated origins / outbox | Current browse SQL joins |
| --- | --- | --- | --- | --- |
| Workshop | 1707 `canonical_state` + 1850 `memory_version` = 3557 | 4 / 4 | 4 / 4 | 1707 |
| Homelab | 459 `canonical_state` + 519 `memory_version` = 978 | 2 / 2 | 2 / 2 | 459 |

Both clones passed exact schema inspection, retirement of their single space,
immediate exact retry and all 159 reviewed preserved table fingerprints. Separate
untouched84 baseline restores then confirmed all 158 original non-migration-table
fingerprints through upgrade and retirement, plus all 84 original migration rows.
That preserves the original record payloads/digests, canonical/version/project/
principal/device/area/origin identifiers, projection cursors and old outbox
bindings. Semantic payload kinds stay `canonical_state` and `memory_version`.
Only the reviewed operational fields differ, including disabled contribution
bindings. Both local seed dump hashes remain unchanged after rehearsal.

The unchanged store browse SQL still joins the current rows shown above after
retirement. This is a populated database preservation/join check; it does not
claim application HTTP authorization, citations, Rust decoding/search, fresh
enrollment or device signature verification on this seed. These private caches
have no legacy federation replica receipt history, so the reused historical
verifier reports zero such spaces/records/receipts. The earlier synthetic tests
exercise decoding, retained reads, egress fencing and rebind behavior separately.
Role/database renaming was deliberately outside this slice.

Restricted local artifacts remain under the same state directory:
`mac-rehearsal-seed/`, `federation-mac-schema-reference.json`,
`federation-mac-rehearsal-results.json` and
`federation-mac-{workshop,homelab}-retire-rehearsal.json`. Disposable upgraded and
untouched84 baseline clone databases are retained. Original local and remote
seeds are unchanged; a final quiesced matched bundle and live cutover remain open.
