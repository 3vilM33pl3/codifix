# Trust-only inactive transfer

Status: local transfer preparation and fresh delta verified; receiver, renewed
review and live-change gates remain open. Not permission to change the CA.

Continuation checkpoint: task `d7eda699-ab9e-4844-8b49-7c287a0b289e`, memory
`c0e647fc-36a6-4763-92d6-ccb7da374499` (22 September 2026). This is a bounded
continuation of the four-area staging and CA handoff, not a new federation or
portable-import protocol.

## Contract

- Keep the complete source archive, inventory and ownership manifest on Monolith.
  The current importer requires all of them; do not weaken its opaque verified
  partition boundary or send the complete archive to the CA.
- Relocate only the already reviewed **inactive Trust partition**, preserving its
  target-instance UUID, operation UUID, original rows and immutable import
  receipts. A validation clone is not another independently active instance.
- Derive minimized Trust verification expectations from the complete prepared
  archive locally. Verify every imported row, receipt, reference-only projection,
  table count and inert sequence state independently of the transfer payload.
- Use schema produced by the pinned signed release in a fresh isolated reference
  database. Never execute schema/functions/triggers copied from the source dump.
  Restore trusted pre-data, allowlisted table/sequence data, then trusted post-data
  in an empty isolated destination. Do not disable triggers, patch receipts,
  overwrite existing databases or use `--clean`.
- Require a successful independent local restore before labeling a candidate
  verified. A pre-export check alone does not lock the source; PostgreSQL sequence
  state is not an MVCC snapshot. Exact descriptors, isolation and continued writer
  exclusion remain required.
- Bind PostgreSQL/client/extension compatibility, release/schema hashes and every
  artifact. Encrypt private transfer material to an existing explicitly approved
  recipient available at the destination. Release signing does not authorize a
  new encryption recipient. No unapproved private data is posted to Notion.
- Keep old snapshot evidence accurately dated. Refresh the live source baseline
  and compare full-history/dependency hashes before any current/cutover claim.
  Changed decisions require renewed review; CA-only/offline data is still separate.

## Delivery slices

- [x] Resume dev Memory and checkpoint scope; acknowledge CA native verification.
- [x] Add and test the minimized expectations and constrained restore profile.
- [x] Revalidate real Trust staging; produce and restore a local candidate.
- [x] Capture a fresh encrypted online baseline and independently verify restore.
- [x] Produce snapshot delta and a metadata-only writer inventory (observational,
  not complete writer fencing).
- [x] Publish Monolith evidence and the grouped **inactive** proposal draft. The
  CA agent must supply exact local fields before it becomes an approval request.

See [operator commands](../../../scripts/areas/trust-transfer-README.md) and the
[CA-agent proposal](../../user/ca-agent-handoff/TRUST-INACTIVE-PROPOSAL.md).

## Verified on 22 September

- Fresh encrypted source snapshot `4a1b605d-8b97-4757-862a-12f49e9ed4a7` restored
  into a new network-none sandbox: **104 tables, 2,850,128 rows, three sequences**
  exactly matched. No source writer was stopped. Private artifacts:
  `/data/tmp/memory-four-area-baseline-20260922T1903`.
- Complete current review pack: **3,270 histories / 3,553 versions** across 107
  projects. The delta carried **262** unchanged decisions; **2,945** histories
  have changed dependencies and **63** are new. Exit2 / `ready:false` is the
  correct fail-closed result, not a command failure to override.
- Of the 19 previously assigned Trust histories, **two** match completely and
  **17** require renewed dependency review. No Trust memory-version or source row
  changed; six Trust relation sets changed. Dependency changes still invalidate
  approval reuse. No changed decision was automatically approved.
- Revalidated the **September 19** Trust staging partition and restored its
  minimized data using independently generated schema: **375 imported rows**,
  19 histories, 22 versions, zero tombstones, two projects, 65 relations and 185
  reference-only provenance rows. All 114 table counts, immutable import receipts,
  four inactive sequence states and all 60 migration checksums matched. Runtime,
  auth, jobs, enrollment and supervisor grants remained inactive/absent.
- Source executable DDL is excluded. The separately pinned local schema helper
  is not part of the signed ARM64 package; its schema identity is independently
  recomputed from frozen release-source migrations. PostgreSQL 17.11/pgvector 0.8.6
  and host `pg_restore` 17.9 are the tested profile, not evidence for another CA
  PostgreSQL major. The schema/image/client identities are recorded privately.
- A **301,633-byte encrypted local preservation bundle** contains only Trust
  data, trusted schema, private Trust expectations and its manifest. Exact local
  decryption recovery passed. It uses the existing backup recipient; CA access
  to that recipient is unknown. **No data was delivered to the CA.** Candidate
  and receipts: `/data/tmp/memory-trust-transfer-20260922T1909`.
- Observed both system/user Memory services and multiple watchers; 15 database
  client connections at observation. Dev/global configurations still bind the
  same shared source. Remote/intermittent CLI/library/MCP writers, CA-local
  singleton/offline stores and effective role-level fencing remain unresolved.
  Preview gateway/worker/relay units are recorded separately, not assumed source
  writers merely because their names contain Memory.

## Tests and recovery observations

All **103 scoped tests** passed with real filesystem ownership, Docker fixtures
and Rust conformance enabled (`MEMORY_AREA_TEST_DOCKER=1
MEMORY_AREA_TEST_RUST=1 node --test scripts/areas/*.test.mjs`). No skips, paid
providers or private test data. Boundary and whitespace checks passed. No Rust
application/API/Web behavior changed; a full workspace/Web/Lean run is not claimed.

The synthetic restore proves real 114+4 TOC parsing, canonical/seed ordering,
source-only malicious DDL exclusion, exact table/sequence preservation, complete
pre-COMMIT rollback and refusal of a nonempty target without mutation. Test
containers were removed; original staging/preview/source containers remain.

Keep failed preparation artifacts: one repeat exposed a PostgreSQL startup race
before schema preparation; readiness now requires both the final server process
and `pg_isready`. Its disposable container was removed and a fresh rehearsal
passed. An initial stdin-based encrypted readback failed; a file-based decryption
and exact tar comparison succeeded, recorded separately. Neither failure was
treated as a completed transfer. The sandbox's remapped root ownership caused
private-path tests to reject correctly; rerunning with actual ownership passed
without weakening validation.

| Evidence | SHA-256 |
| --- | --- |
| Fresh encrypted source DB | `cd9d83e8b90854809adb31047795cc972fab04ceef0279632c66f90d34b5a22c` |
| Fresh complete restore evidence | `9e5a89f61bf77bdf5bab01ef127a9b64556567c6a813d912a26f21bf7535219c` |
| New normalized inventory | `18bdd4ece5de41cf6a02874cb2ef292321892ace630dfa78a8f9278230c15762` |
| Full snapshot delta report | `a40afc0d8d18fce9598082a8374703e542e22ff3b85a0e53f905da84b14e828c` |
| Sanitized delta summary | `12ef860f5318450248fd902c44ec44105db6dabb0732af0e2df35f4c2fa23f6c` |
| Writer observation | `2c1574d14c3b3b41d9a2f1042183a9e01f4a97192f71405a5a9060952de9f219` |
| Private minimized Trust expectations | `71d6b80e5baf0b6b3a2e6c8965d5ff4810ba079446d5c69e7cfdd1c96f9fd247` |
| Guarded real restore (rehearsal03) | `7dfb42d58f154baa4df104ebc316bda82cb902eb1092ebafb6a625f555d9d4c2` |
| Final read-only/client-bound restore (rehearsal04) | `4ab8e4d182e151657a221837febf196a5032c3cab01831afba801e8b824dd3c2` |
| Encrypted Trust-only local bundle | `18238930dda30a61dcc6eef916e36a7ddd683f1cf3b1bd9efce52322a37d85d0` |
| Local encrypted recovery receipt | `66b49e926bdd6f11cb0c33cf69c2abfc685bee20b34ab79602b789d58cd851c9` |

The protected candidate is intentionally dated September 19. This work establishes
a safe local relocation mechanism, not current ownership approval, native CA
compatibility, receiver tooling, installation, activation or customer readiness.

## Authority and evidence boundaries

Read-only source inspection, isolated local preparation/testing and existing-key
encrypted backup are in scope. Existing Memory writers and previews remain up.
CA packages/services/databases/credentials, private data delivery, signing new
release artifacts, final writer fencing and activation need their corresponding
explicit approval. No Step CA, OpenBao, DNS, Pi, paid providers or R3 changes.

The CA agent reports native release preflight passed, **not installed**:
`f68fce0b43762cf8a5d429b18b6eaa99d2864cbcecfe5f765e1d3f4760b546c5`.
Its signature receipt is
`ba6f27c45cf505e4b6fa00bf9ba83a6022dd0bebdfffb26c633f82eba53946fb`.
These are remote-agent reports, not locally reproduced CA runtime/import tests.

Coordinate through the [existing Notion page](https://app.notion.com/p/3e38990715f48117b9e8d8f3a95637d1).
Do not imply that page comments run or authorize unattended host operations.

The [connector-readable proposal](https://app.notion.com/p/3e38990715f481d0b5f7d4d569e01e05)
was published and fetched back on 22 September. Append-only result message
`9940889a-906d-4559-95df-2132c7aab879` supplies the evidence hashes and requests
one consolidated CA metadata response. No private artifact was attached.
