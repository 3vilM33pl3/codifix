# Trust-only inactive transfer

Update, 24 September 2026: the side-by-side CA Trust preparation is superseded.
Olivier selected the faster operational path: replace `15/main` with `17/main`
while retaining PostgreSQL 15 as stopped rollback material, and upgrade the
existing Memory singleton to signed release `2.1.0-rc.1`. Trust data transfer,
the four-area cutover, federation and Pi follow after that working baseline.

Status: the 25 September current Trust-only candidate, encrypted source backup,
and PostgreSQL-17 receiver are verified locally. The CA-recipient-bound package
was copied to removable USB and verified after an unmount/remount readback. The
CA subsequently verified, decrypted and extracted it, then restored it into a
new isolated `17/trust` cluster. The restored target is stopped, manual and not
activated. Application release preparation, credentials, service installation
and activation remain open.

Update, 24 September 2026: Olivier authorized the CA agent to perform one
bounded autonomous inactive PostgreSQL-17 preparation without intermediate
human approvals. The agent received the exact PGDG repository/package binding,
ordered safety checks and a single completion/blocker report contract. This
does not extend to Trust data transfer, restore, application activation or
singleton migration.

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
  CA agent supplied the exact local fields; the bounded PostgreSQL-only portion
  is now authorized without intermediate approvals.
- [x] Record the CA's PostgreSQL15.18/no-pgvector observation, retain the tested
  compatibility fallback, then select and verify PostgreSQL17.11 as the Trust
  deployment target at Olivier's direction.
- [x] Build a portable, fail-closed PostgreSQL17 receiver; prove inspect,
  transactional restore, complete verification and replay refusal.

See [operator commands](../../../scripts/areas/trust-transfer-README.md) and the
[CA-agent proposal](../../user/ca-agent-handoff/TRUST-INACTIVE-PROPOSAL.md).

## Verified current candidate on 25 September

Steps 1–4 of the current Trust-data preparation completed locally without
stopping the shared source or changing the CA:

1. Snapshot `8ffff72d-3754-4a3f-b5fd-409d609b6475` was encrypted to the existing
   operator recipient and restored into a new network-none PostgreSQL 17
   container. All **125 tables, 3,127,417 rows and four sequence states** matched.
2. The complete redacted review covered **3,325 histories / 3,610 versions**.
   Schema and dependency changes invalidated every old carried decision, so none
   was silently reused. Eighteen histories were reviewed as Trust; 3,307 remain
   protected and non-searchable. One prior Trust item about deploying the
   unrelated `random-web` application was removed. A new certificate-consumer
   workflow also remains protected because it is not authoritative CA operation.
   These are agent-reviewed decisions, not Olivier's personal approval or model
   disclosure consent.
3. The current Trust staging import verified **18 histories, 21 versions, 371
   rows and 183 reference-only provenance projections**. A minimized 273,431-byte
   encrypted bundle was created. Decryption reproduced the exact plaintext tar
   digest; the all-area archive is not in the receiver bundle.
4. The minimized bundle was restored transactionally into fresh disposable
   PostgreSQL **17.11** with pgvector **0.8.6**, `pgcrypto` **1.3** and `pg_trgm`
   **1.6**. Independent verification and replay refusal passed. No service,
   writer, federation worker or supervisor was started.

Protected local roots:

- source snapshot: `/data/tmp/memory-four-area-baseline-20260924T2250`
- full protected preparation: `/data/tmp/memory-four-area-trust-prepared-20260925T0015`
- inactive Trust stage: `/data/tmp/memory-four-area-trust-stage-20260925T0020`
- independent restore rehearsal: `/data/tmp/memory-four-area-trust-rehearsal-20260925T0035`
- minimized receiver: `/data/tmp/memory-four-area-trust-receiver-20260925T0045`

| Current evidence | SHA-256 |
| --- | --- |
| Encrypted source database | `c576468bdbf377c2351d1334879e5998841aaa89a53127014513d55c49bbcfb5` |
| Source restore evidence | `31c15bede5c7fc50aa9b25b7962f7a9b3e718fa4ef036fa22cf0a7d79843c38a` |
| Complete inventory file | `d1065eba05e13a6b5fd8137d6d03438518a2a81d0d6a03e6152a14e06ffe7df7` |
| Current Trust decision ledger | `ff4ac428324dfc4051117babeb1d47c2d43d310669687f9aa184f9a8b2d7f885` |
| Current Trust manifest file | `b4a1ae3110c7df778240f92e83e709fbdafe04a57c4f4e0dad74544ba6155d67` |
| Prepared receipt | `502a2590b270241bad18bb9143b1f4b3d9ec493dafe458d699604801da05e6a4` |
| Minimized expectation | `46b177741175f059910cd0ac31a29f37f75fddde065b49c42ec836d8f4f47bf9` |
| Independent restore result | `148f064aa5ad3fcda7dfc7a75ad4a134673aebbc4ee82879b20eae29518bb528` |
| Receiver manifest | `15dc9804d36def59985c33055a81b74018278f336b6b1a00ca5839da84c04046` |
| Receiver plaintext tar | `c59a67429364f38f173186424f5d16a28eff010670719fe62b7ec9b7d88dc48e` |
| Encrypted receiver tar | `8783b54c0a8ba3840f1331fb78a3fc68ac22450b6b447fbbab458a0a3d87f9a7` |
| Encryption read-back receipt | `0d87faec119a9815a2e3a1d2d5e64b33583f83eea80b0101c6695ee317219e7d` |

The ciphertext uses the existing operator backup recipient, not a proven CA
recipient. It therefore remains on Monolith. Transfer requires a destination
decryption decision and independently authenticated delivery instructions.

### CA recipient-bound transfer candidate

At 05:25 UTC on 25 September, the CA agent reported a dedicated CA-local data
recipient and a passing synthetic round trip. Monolith independently reconstructed
the exported public key, matched its reported SHA-256, parsed the exact primary
and encryption-subkey fingerprints, confirmed its expiry and found no secret-key
packets. The release-signing key was not reused.

- primary: `ACDBD1CB5ECB39FCA84439ABC2E27E7B42623A5E`
- forced CV25519 encryption subkey:
  `D39EB8070E71832B63E9E4516CA8C0F9CBCB9BF3`
- public-key SHA-256:
  `64d58dc720a940bd1408215507ac1f299346cd80de0f9b018427a3e17088f321`
- expiry: 24 September 2028

The already verified plaintext receiver tar was encrypted directly to that
subkey with hidden recipient key IDs. A clean extraction of the transfer archive
revalidated every contained checksum and the recipient fingerprints.

| Prepared transfer evidence | SHA-256 |
| --- | --- |
| CA ciphertext | `0f73361ea5633945bbd7e47b0a624e824f5d6512634796608789b970fae97011` |
| Transfer manifest | `ae01fcec4ec3fdcfcfabb9c60aa48d9e45f52db4b9395750b7d92751ac615bfc` |
| Transfer `SHA256SUMS` | `d06d2c1b7dfcc2419cf273b33ba8e54944a3d55c9d837286223226d454a65810` |
| Complete transfer tar | `79c0b5ed84484dcd3284baf8480d44e1dfce2ff7309f4116cc06b206319b6e17` |
| Included receiver | `0ca47c5ba5ed6f661f8b42da60b5f6adedbc08be6ee8a8651663423ec18a9855` |

Protected local transfer root:
`/data/tmp/memory-four-area-trust-ca-transfer-20260925T0530`; portable tar:
`/data/tmp/memory-trust-ca-transfer-20260925T0530.tar`.

At 05:40 UTC on 25 September the candidate directory and portable tar were
copied without overwriting existing files to the removable VFAT filesystem with
label `BIOS UPGRAD` and UUID `66F0-75AD`. Every package checksum and the portable
tar digest passed both before and after an unmount/remount cycle. The device was
then cleanly unmounted. The local receipt is
`/data/tmp/memory-four-area-trust-ca-transfer-20260925T0530/usb-copy-receipt.json`.

This is transport staging, not proof of CA receipt. Private ciphertext was not
uploaded to Notion. Restore and activation remain false in the immutable
transfer manifest.

At 05:40 UTC the CA agent reported that its Notion connector could fetch the
page body but only the first 16 comments in the main discussion. It therefore
could not see the USB instruction and stopped without touching the host. The
same sanitized instruction was republished at the top of the coordination page
body as `monolith-ca-20260925-trust-usb-body-v1`; connector read-back confirmed
the message ID, status and transfer digest.

The body instruction also corrects the next boundary: this receiver's `inspect`
command connects to an already-provisioned dedicated PostgreSQL target on port
5433. No such target has yet been verified on the CA. The current authorized CA
step ends after copy, checksum verification, recipient verification, decryption,
receiver-tar verification and private extraction. It executes no SQL and changes
no database, role, cluster or service. Target provisioning and receiver inspection
require a later host-change instruction.

The CA agent subsequently reported `VERIFIED_DECRYPTED_EXTRACTED_ONLY`. The
portable tar, all five package checksums, recipient fingerprints, decrypted tar
and receiver manifest matched the Monolith identities. Its verification receipt
SHA-256 is
`8e31ae044df06bafc9ac401f291ec8bcd32ac774f576ba2891a03f3bd0a274e3`;
private evidence remains at
`/var/lib/memory-layer/trust-transfer-fb2ddecb-f60d-4caa-9a40-a7ac2397aaa4`.
It reported no SQL, database, role, cluster or service change. This is CA-reported
evidence, not a Monolith read-back of the CA filesystem.

The CA observed removable-filesystem UUID `5A93-3F75`, whereas Monolith observed
`66F0-75AD` during the verified copy. Both transport observations are retained;
the matching content hashes identify the payload. The next connector-readable
instruction, `monolith-ca-20260925-trust-target-restore-v1`, authorizes one new
manual, Unix-socket-only PostgreSQL `17/trust` cluster on port 5433, a fresh
unprivileged passwordless `memory_trust` role/database, receiver inspection and
transactional restore, followed by stopping that cluster. It explicitly excludes
service credentials, Memory activation and changes to `17/main` or protected
services.

The CA then reported
`ca-monolith-20260925-trust-target-restore-result-v1` with status
`RESTORED_VERIFIED_INACTIVE_NOT_ACTIVATED`. PostgreSQL `17/main` remained online
on port 5432; `17/trust` finished down on port 5433 with manual start policy,
empty `listen_addresses` and Unix socket directory `/var/run/postgresql`. The
unprivileged passwordless `memory_trust` role owns database `memory_trust` and
has no elevated role attributes. Receiver inspection matched PostgreSQL 17.11,
pgvector 0.8.6, `pgcrypto` 1.3 and `pg_trgm` 1.6. Complete restore verification
matched 371 rows, 18 histories, 21 versions and 183 projected sources; source
DDL was not executed. Restore-receipt SHA-256 is
`0e4840874dc80beb50344dc5663c54d5aef10232c9ef290be1eac14222b3a431`.
The CA reported the existing Memory service, `17/main`, Step CA and OpenBao
unchanged. This remains CA-reported evidence; no service credentials, Trust
service or activation exists.

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
a safe local relocation mechanism and receiver, not current ownership approval,
native CA package availability, installation, activation or customer readiness.

## PostgreSQL-17 receiver evidence, 24 September

- CA-reported read-only observation: PostgreSQL/server/client 15.18, `pgcrypto`
  1.3 and `pg_trgm` 1.6 metadata present, no pgvector package available in its
  current APT metadata. No existing CA data-decryption recipient was observed.
- Olivier selected PostgreSQL17. The deployment topology is a side-by-side
  `17/trust` cluster on port 5433, database/role `memory_trust`, leaving the
  PostgreSQL15 singleton untouched. The CA agent reported auto-cluster behavior;
  Monolith bound the official PGDG repository, key and exact ARM64 packages.
- Exact isolated receiver profile: PostgreSQL/client 17.11, pgvector 0.8.6,
  `pgcrypto` 1.3, `pg_trgm` 1.6. The pinned image ID is
  `sha256:17a06c0a60bf6fb548a8493b8f6057dc830b791bbbe1f1ab706ca4b9d97c8180`.
- Private unencrypted bundle:
  `/data/tmp/memory-trust-receiver-pg17-20260924T0840/bundle`; manifest SHA-256
  `10dd74498e9fcb11abcfd81f22ce6c086731258722def4001895211d9a63a057`.
  It is local, not transferred or activated.
- Portable Python receiver accepted the exact fresh profile, restored in one
  transaction, verified 114 tables, four sequences, 60 migrations, ownership,
  375 imported rows and 185 projected sources, wrote a mode-0600 receipt, then
  rejected exact replay without changing the first receipt.
- The earlier exact PostgreSQL15.18 compatibility proof also passes and is kept
  as fallback evidence. It is no longer the selected deployment target.

Focused offline tests passed, along with both opt-in Docker integrations:
PostgreSQL17-to-exact-15.18 compatibility and the portable PostgreSQL17 receiver.
No CA, source database, current service or private transfer was changed.

## Authority and evidence boundaries

Read-only source inspection, isolated local preparation/testing and existing-key
encrypted backup are in scope. Existing Memory writers and previews remain up.
The bounded CA PostgreSQL package/empty-database preparation is authorized.
Private data delivery, signing new release artifacts, final writer fencing and
activation remain outside it. No Step CA, OpenBao, DNS, Pi, paid providers or R3
changes.

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
