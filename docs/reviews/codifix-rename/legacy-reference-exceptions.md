# Deliberately retained legacy references

These are preservation exceptions, not active product aliases. The application,
commands, package identities, new configuration and active federation protocol
use Codifix. Do not run a blanket string replacement over the repository.

| Reference | Reason and boundary |
| --- | --- |
| `migrations/0001` through `0084` | Previously applied SQLx bytes/checksums are immutable. New changes belong in migration 85 or later. |
| Original release notes, review evidence, signed manifests and captured outputs | Historical evidence retains the name, hashes and paths that were true when recorded. |
| `memory-layer-local-owner`, `memory-layer-legacy-service-token` in principal tests | Assert that explicit preserved UUID constants equal the original branded derivation. |
| `memory-layer:loop:{id}:{version}` | Existing durable loop execution identity must remain identical across the rename. |
| `mem-area-materialized-partition-v1`, `mem-area-provenance-projection-v1` | Hash preimages authenticate existing preserved transfer/provenance artifacts. |
| `memory_onboarding_operation` provider correlation attribute | Existing identity-provider objects retain their exact idempotency binding. |
| `mem-analyze-v2` | Stored analyzer algorithm version; the rename does not change the analysis algorithm. |
| Retired federation namespaces and signed-purpose strings | Accepted only by explicit historical readers pinned to archived evidence; active ingestion uses Codifix. |
| Pinned Pi upstream patches, `memory-windows-pipe`, `MEMORY_BROKER_HANDLE`, `memory-broker.invalid` | Keep the reviewed third-party source/lock/patch baseline and its matching broker protocol unchanged. Product wrappers and packaging use Codifix. |
| `MEMORY_AGENT_BENCH_*`, MemoryAgentBench | Independent external benchmark integration, not the product's branding. |
| Old names in cutover instructions and CA preflight | Identify exact existing objects for inventory/retirement. They do not create runtime aliases or change the CA access boundary. |
| `scripts/deploy-workshop-review-preview.mjs`, `scripts/deploy-workshop-questions-preview.mjs` | Historical one-shot preview operators pinned to old deployments/schema; retain as evidence and do not run them with the candidate. |
| Remaining Mac/CA and disposable dev database names | Monolith application and gateway databases are renamed. Remaining installations are pending work, not completion exceptions. |
| Existing service DNS names, repository DID, certificate subjects and backing Docker volume/network identifiers | Retained deployment/trust identities; new protocol records and product components use Codifix. Do not rewrite signed identities or move storage by string substitution. |
| `/home/olivier/Projects/memory` symlink | Temporary bridge for already-running app sessions to the canonical `/home/olivier/Projects/codifix` checkout. Remove after clients reopen the new path. |
| Installed legacy embedding guest-bridge token environment names | The retained guest binaries require their existing `MEMORY_*` token names. Equivalent Codifix application settings and restored units use the same existing credentials; replace these bridges during their package update. |
| `evidence/historical-assets/memory-layer-infograph.png` | Preserved original diagram; active docs use `codifix-flow.svg`. The old image is outside the public site tree. |
| Existing screenshots and old build-cache ignore rules | Screenshots may document historical UI; old cache/state exclusions preserve recoverable files during transition. Refresh current screenshots before declaring visual acceptance. |

Semantic memory vocabulary remains unchanged: `memory_id`, `memory_entries`,
`memory_version`, `canonical_state`, `/v1/memory`, and Rust `std::mem` describe
knowledge or language primitives. Test-only temporary-directory labels have no
installed product identity or persistence contract.
