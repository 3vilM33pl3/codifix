# Private canonical-history review pack

`classify.mjs` creates a human review aid from the verified, network-isolated
restore container. It does not invoke a model or approve migration ownership.
Use explicit protected inventory, unresolved manifest, valid inventory report,
restore evidence and a new output path:

```sh
node scripts/areas/classify.mjs \
  --inventory /protected/partition-inventory.json \
  --manifest /protected/partition-unresolved-draft.json \
  --validation /protected/partition-inventory-validation.json \
  --restore-evidence /protected/restore-evidence.json \
  --output /protected/canonical-review-pack.json
```

All inputs must be owner-only regular files. The output parent must have mode
0700 and belong to the current user. The new output is mode0600 and cannot
overwrite another artifact. Operational stdout contains aggregate counts only;
raw database output and errors are not forwarded. Real review packs must never
be committed to the repository.

The source binds the restore container, source/snapshot IDs, inventory byte and
semantic digests, manifest, inventory-validation and restore-evidence hashes.
One read-only repeatable-read transaction reads every remaining canonical version
and linked source record. Exact lineage/version/project counts are reconciled.
Complete original version/source row hashes use the inventory's UTC PostgreSQL
JSON serialization. This pack does not independently compare each hash against
the million-row inventory; the existing full restore/inventory evidence remains
part of the trust boundary.

Each entry keeps original project IDs, all historical version IDs, tombstones,
timestamps, row hashes, source/task/session/capture references, bounded previews,
and explicit abbreviation/omission indicators. Only the latest readable content
is excerpted; all remaining historical content is scanned for rule markers.
Raw capture payloads and full task prompts are not inspected.

Rules version 2 defaults to the original project area: Memory to Workshop,
Homelab to Homelab, and other projects to General with their project IDs intact.
Content topics that disagree with that default create review flags, not automatic
relocation. Memory Layer initialization in SCTP, Rillforge or CanalGame can mean
using the tool, not developing it. `ca-2` never establishes Certificate Authority
ownership. CA/PKI mentions and application consumers such as Turris admin TLS
remain in their original area. Only explicit CA-operator markers in a Homelab
project, without detected consumer, negated-operation or Workshop context, can
propose Trust. Even this narrow lexical proposal requires human validation.

Delimited evaluation/test/smoke/fixture project markers (including hyphen,
underscore, slash, dot and colon boundaries), explicit synthetic material, contradictory
history, multiple original projects, possible secrets and terminal tombstones
produce protected review/archive proposals. Credential-shaped material is
redacted from previews; this heuristic is not a complete secret scanner. Shared
task/session links spanning area or retention candidates create separate proposed
archive holds for their supporting sources. Structural sharing is a risk signal,
not proof that the raw payload is mixed. Names such as `memory-bundle-smoke`,
`memory-quality-eval`, `keyless-gate-test` and `ingest-test` receive archive-review
holds. False positives such as `skyrim-test` also remain on explicit review hold;
the heuristic does not declare them disposable or approve their archival.

Rules version 1 over-weighted generic specialized topic markers and missed test
tokens after hyphens. Keep earlier artifacts as evidence; generate a new v2 pack
at a distinct path rather than overwriting a previous review.

Every proposal remains unapproved. The output is deliberately a different format
from the migration manifest. A reviewer must examine all necessary history and
provenance, resolve false positives and missing context, and make an explicit
manifest decision. No migration manifest, import, project merge, provider call,
credential access, or live cutover is produced.

Synthetic tests: `node --test scripts/areas/classify.test.mjs`.
