# Populated Workshop functional read acceptance (isolated, 2026-10-17)

Candidate executable was copied out of target/ before use and pinned at SHA256 `94e50052aed58b3d5e227f2cafb801805a19114b81e95347e14c479166d3f8c3`. Rust source matches `3701fd10`, which includes cold-start fix `15e1f828`; the corrected CI candidate `afbde0b8` has identical Rust sources. No Cargo build or source edit occurred in this acceptance.

The already-retired populated Workshop snapshot `codifix_e_snapshot_final` was cloned to `codifix_populated_read_acceptance` inside the existing disposable PostgreSQL17 loopback test container. Schema85 and its one retirement marker were confirmed before startup. Original snapshot and clone matched at baseline. The original snapshot and clone still match at completion: normalized semantic schema plus exact row-multiset SHA256 for all 161 public tables, including all 159 prior reviewed tables and both migration85 marker tables. No ignored operational fields were used. This covers original IDs, texts, provenance, hashes, auth, projections, migration rows, outbox and retirement receipts. Source and clone before/after metadata manifests are private.

The real candidate service cold-started successfully with prod profile, explicit private config, read_only=true, database.migrations=verify_only, an allowlisted environment with separate HOME/XDG directories, and a synthetic local token in single_user mode. All LLM/embedding/background/automation/offline/reinforcement/cluster/telemetry/provenance worker facilities were disabled. Service PostgreSQL connections additionally set default_transaction_read_only=on. The owned service process was stopped afterward.

Verified application requests:

- GET health: 200, status ok, database up.
- GET memory: 200; exact original version/canonical IDs, version number, text, summary and source count.
- GET history: 200; all 3 original version IDs/numbers and full version text/summary matched database rows.
- GET project browse: 200 on populated data.
- POST semantic read-only query: deterministic lexical current and history retrieval, each 200 with 10 results and 3 citations. Every returned memory/canonical/version ID and every citation matched original rows; all 21 current-result and 19 history-result provenance references matched original file path, commit, kind and excerpt.
- single_user anonymous read returned 200, matching the existing anonymous-reader policy. This is explicitly not a multi-user denial test.

The explicit retired-history verifier independently verified 1 space, 23 retained records and23 receipts in a normal transaction, followed by unchanged whole-table/schema fingerprints. Its strict PostgreSQL read-only-transaction probe failed with SQLSTATE 25006 because ReplicaStore::audited_snapshot calls load with SELECT FOR UPDATE (replica.rs). This is a concrete limitation: the historical verifier needs row-lock permission, although its successful normal-transaction execution preserved all row content.

Limits: this owner Workshop seed does not exercise Mac device-cache HTTP selection or original remote-origin citation rendering. 52 chunks exist, but there are zero stored vectors, embedding model/provider/space values or separate embedding rows; semantic/vector acceptance remains unexercised. There is no dedicated attachment table or attachment-file-serving route in this schema/service, so no attachment-byte acceptance is claimed; source references are provenance. No fresh federation enrollment/publication, peer connectivity, provider execution or CA action was exercised. Configuration and clean environment constrain execution, but this run does not claim packet-level proof of no transient egress.

Private machine evidence: result.json, baseline.json, after.json, source-baseline.json and source-after.json are retained in the private populated-read-acceptance workspace. Result JSON SHA256: `db7388fd9de7ec7fae3f05b66a0f6b41b724ffce319f79ca9e3dc8d9e2217c92`. Metadata only; no private knowledge bodies or credentials are in this draft or result JSON.
