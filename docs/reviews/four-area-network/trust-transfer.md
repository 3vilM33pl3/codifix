# Trust-only inactive transfer

Status: implementation in progress; not permission to change the CA or cut over.

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
- [ ] Add and test the minimized expectations and constrained restore profile.
- [ ] Revalidate real Trust staging; produce and restore a local candidate.
- [ ] Capture a fresh encrypted online baseline and independently verify restore.
- [ ] Produce snapshot delta and a metadata-only writer inventory.
- [ ] Publish Monolith evidence and the grouped **inactive** host-change proposal.

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
