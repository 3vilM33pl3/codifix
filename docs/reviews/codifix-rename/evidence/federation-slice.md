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

No live service, database, CA, peer or provider was mutated or contacted by this
slice. Populated Mac device-cache rehearsal, other populated area/gateway clone
checks, consolidated workspace checks and the operational stop/drain/restart
cutover remain parent release gates. Original seed restores remain untouched.
The release operators document that old processes must be drained before applying
markers and only candidate binaries may restart afterwards.
