# Four-area staging evidence

This report covers the approved [dependable-preview and staging contract](dependable-staging.md).
It is **not a cutover**. Original writers and the two live previews remain
operational. No service is started against a staging target. Trust is not
installed on the CA; General is not live. R3 remains paused.

## Snapshot and complete preservation

Snapshot `2154cc37-c3cd-4784-b335-37062e6d240d` contains 107 projects,
3,207 canonical histories, 3,482 memory versions and 2,310 captures. Its isolated
restore independently verified all 104 tables, 2,587,041 rows and three sequence
states. The approved-plan checkpoint added a memory after the original planning
count; the snapshot, not that earlier estimate, defines this stage.

The complete archive preserves every original row, including records that are
not eligible for an area database. Source accounting assigns each original row
exactly once:

| Disposition | Original rows | Meaning |
| --- | ---: | --- |
| Exact area copy | 417,510 | Reviewed knowledge and same-area dependencies |
| Protected archive | 1,859,031 | Private, uncertain, mixed, unsupported or historical material |
| Archive and rebuild | 291,092 | Derived data retained as recovery evidence, not activated |
| Archive and retire | 1,981 | Old runtime state retained, without execution authority |
| Archive and project provenance | 17,427 | Original evidence retained privately; reviewed opaque references imported |
| **Total** | **2,587,041** | **No unaccounted original rows** |

The source-accounting report proves file preservation, not database import.
Target row counts also include project metadata copied into more than one area;
they must not be added to protected rows to infer the original total.

The private ownership ledger records the reviewing agents, content hashes and
decision basis. It does not represent Olivier personally approving each record,
nor grant model or federation disclosure permission. Full histories were reviewed;
newest-version-only assignment was not used. The ledger's archive classification
field uses `ambiguous`; specific sensitive, mixed, test and unsupported reasons
remain in the private decision basis rather than a fabricated category breakdown.

## Reviewed partitions

These are exact materialization counts. Database replay, independent verification
and application-clone results are recorded separately below.

| Area | Histories | Versions | Tombstones | Projects | Source references | Relations | Target rows |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Workshop | 1,536 | 1,677 | 4 | 4 | 10,297 | 377,104 | 394,577 |
| Homelab | 393 | 426 | 0 | 7 | 2,831 | 18,322 | 24,075 |
| Trust | 19 | 22 | 0 | 2 | 185 | 65 | 375 |
| General | 742 | 808 | 0 | 33 | 4,114 | 8,262 | 15,956 |
| Protected, not imported | 517 | 549 | 1 | — | — | — | — |

Original IDs, version history, authorship, timestamps and tombstones are retained.
The 17,427 source projections are **archive references**, not copied evidence
bodies or newly inferred excerpts. Source paths, captures, sessions and original
task context are not made searchable by these projections. General preserves
the original project boundaries rather than merging everything into one project.

## Import and recovery status

Fresh inactive targets use schema head 60 and separate instance identities. They
have no network, published ports, copied credentials, enrollment, supervisor
authority or application workers. Imports use a single transaction, stable
operation IDs and snapshot-bound receipts.

All four imports have committed, including Workshop's 394,577 rows and 10,297
provenance references. Workshop initially encountered two PostgreSQL
out-of-memory failures. Both rolled back the complete transaction, leaving no
projects, knowledge or import receipts. A larger container allowance did not
resolve the underlying defect, so retries stopped for synthetic diagnosis.

The operator disables SQLx's statement cache, but SQLx 0.8.6 persistent queries
still allocated named server statements without retaining handles to close them.
The 20,000-row reproduction accumulated about 497 MB of backend memory. Fix
`3cb4beae` makes importer queries nonpersistent without splitting transactions or
weakening checks. A 100,000-relation synthetic import and exact replay stayed near
6.6 MiB with zero retained named statements. Workshop then imported successfully
with the same snapshot, target UUID and operation ID. See the
[reproduction and regression evidence](../../../crates/mem-area-store/README.md#large-partition-postgresql-memory-regression).

Exact replay passed for all four targets (`already_applied=true`). A changed
inventory binding was rejected without changing the selected target. At
`2026-09-20T00:12:42.453Z`, the independent verifier matched every target row,
provenance/operation receipt, history and relation against the archive/manifest.
It verified 114 tables and four pristine sequence identities/configurations per
target, inert authority seeds, the kill switch and empty runtime/credential/
enrollment state. Isolated application-clone checks remain pending; no staging
completion is claimed by this report yet.

Preparation also exposed a distinct bounded-materialization issue: the reviewed
snapshot exceeded the default 256 MiB retained-target budget. The operator now
supports an explicit 256–1024 MiB budget, bound into the preparation receipt and
required unchanged at import. This stage uses 512 MiB. The row, schema, hash and
per-envelope checks remain unchanged. Failed partial artifacts are preserved and
are never treated as import authority.

## Protected backlog and usability limits

| Remaining work | Preserved count | Required action |
| --- | ---: | --- |
| Uncurated captures | 22 of 2,310 captures | Review locally; no automatic curation or inferred ownership |
| Pending replacement proposals | 410 of 714 | Resolve with the original owner before choosing a replacement |
| Pending loop proposals | 1,270 of 1,274 | Review ownership and disposition; do not reactivate old loops |
| Protected histories | 517 | Consult the private basis; preservation does not require making test/sensitive material searchable |
| Cross-area relations | 13,922 | Retain privately; new exchange needs explicit policy |
| Relations with an archived endpoint | 50,544 | Preserve without dangling target links |
| Derived rows to rebuild | 291,092 | Separately review indexing/rebuild work and execution authority |

Embeddings, graph indexes, reinforcement and background execution are not
silently restored. Application validation will establish only the tested local
read/search paths, not quality equivalence with the original deployment. No paid
provider calls are required or authorized for verification.

## Evidence bindings

Private artifacts live under `memory-four-area-nas-restore-20260919T2202` and
`memory-four-area-staging-20260919T2350` on the protected staging disk. Do not copy
their content into this repository, application corpus or a model prompt.

| Artifact | SHA-256 |
| --- | --- |
| Normalized complete inventory | `997ecdf64b7490e3a104a5d4c9efa01b88ab77dfb85fa47fad04d195b1770251` |
| Ownership ledger | `7c39cfea52d2e64a08fd7df89014e588905ffb09e52b117c23c7b9ca59f6ada9` |
| Reviewed manifest file | `bdd0c261c97eb8d92b6eed3265e09ddfa043a1aa9cfafd882f6ef813caf2eedc` |
| Complete archive | `25080a385cf2f3ca5c0c400184e775253ac7c812c6e91786795dfd6863577a78` |
| Preparation receipt | `6b7122a15000f91528ec06d64e16d82b8c68c4d764acdeb4daf96f460b728ed6` |
| Source accounting | `05596ac770eae05de89efd788c4cbb086474c7872ed7d2d56298285385b0494d` |
| Independent four-database verification | `f1349e1fd71d25d56f2042a624bba021a36159610b4bffec5214914fc034a724` |
| Corrected import executable | `19d12e328a44b94654dc4136fe9e10158056bf4992c79b637eb5c0fd64870901` |
| Encrypted preservation bundle | `f49201e36bdcfbae9aafd0a6cc5c0fa4a789e66feea419f09eb85499e5138235` |

The preservation bundle includes the full archive, manifest, review pack/index,
annotations, ledger and CA handoff. The NAS copy's encrypted hash and the hash of
its completely decrypted stream match the local bundle. Database imports and
their later verification reports are separate evidence, not implied by this
backup.

## Focused regression evidence

These checks use disposable databases, synthetic records and fake providers:

- `MEMORY_AREA_TEST_DOCKER=1 MEMORY_AREA_TEST_RUST=1 node --test scripts/areas/*.test.mjs`:
  86 passed, including encrypted restore, sequence mismatches, full-history
  review/delta checks, source accounting and the independent staging verifier.
- `python3 packaging/tests/test-ca-preflight.py -v`: 13 passed; no CA connection.
- Required-database area-store/operator correction checks: 17 passed, including
  atomic rollback, histories, projections, receipt corruption, exact replay and
  the new zero-cache regression. The explicit 100,000-relation import/replay and
  bounded 20,000-row diagnostic passed separately.
- Required-database federation runtime/store regressions: 62 passed, covering
  rotating credentials, selective authentication recovery and existing exchange
  invariants. Supervisor-store regressions: 35 passed, including grant, revocation
  and budget behavior.
- Refresh-operator tests and credential-boundary review passed, including
  competing maintainers, interrupted/ambiguous refresh and atomic bundle swaps.
  Fake-provider grant tests cover expiry, retained usage and old-profile rejection.
- Web: 145 tests passed and the production build passed (existing chunk-size
  warning retained). Live Web, CLI and PTY/TUI checks display renewal and grant
  health; cited two-way exchange, denied bypasses and restart recovery are in the
  [preview execution ledger](dependable-staging.md).
- Relevant strict Rust Clippy, `cargo fmt --check`, `scripts/check-boundaries.sh`
  and the lean TUI-capable CLI build passed. These are scoped regression results,
  not a claim of a new full-workspace or customer-release certification.

## Next source delta and CA handoff

Original writers continue, so this snapshot necessarily ages. Follow the exact
[delta and future-cutover commands](staging-cutover.md) with a fresh isolated
restore. Only unchanged full-history **and dependency** hashes may reuse a
decision. An unchanged-snapshot rehearsal carried all 3,207 decisions; it does
not claim that the live source has stopped changing.

The [CA console package](../../../deploy/areas/CA-CONSOLE-HANDOFF.md) is read-only
and prepared, not executed. CA-only databases, offline work and external evidence
remain an explicit coverage question. Staging Trust on Monolith proves neither
CA compatibility nor migration. Activation, writers, credentials, model consent,
federation policies and any final cutover need their own subsequent decisions.
