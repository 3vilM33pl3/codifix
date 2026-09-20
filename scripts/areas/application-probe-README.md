# Application checks on disposable staging clones

`application-probe.mjs` is a content-suppressing assertion helper, not an
installer or a command that creates/activates an instance. Its passing synthetic
tests do not prove that real partitions work. Run real checks only after the
four imports and independent inactive-row verification have completed.

The operator must provision four **separate disposable clones** using read-only
`pg_dump` from exact verified staging targets; never start Memory on the actual
targets. Each application sees only its clone's loopback network, no outbound
route, published port, host config, source mount, Docker socket or provider key.
Use pinned existing images/binaries and temporary synthetic authentication.

The existing frozen evaluation profile can enforce a sealed non-owner database
role and suppress learning, query timeline writes, migrations/bootstrap,
watchers, enrichment, supervisor work and federation. Using this isolation
mechanism is not an R3 campaign or a quality claim. Prepare synthetic principals
and exact-project grants **only in the clone**, then seal benchmark tables;
permit only required operational auth/effect journals. Verify corpus hashes
before and after and verify actual staged targets stayed inactive/unchanged.

`probeApplication` accepts a set of imported version IDs, one own ID, one foreign
ID from each other area, an own project/question, a synthetic own token and
another area's synthetic token. The caller supplies the loopback HTTP function
and actual `memory query --json` execution. Input/output bodies stay private.

Assertions cover own-record visibility, 401 for missing/wrong/foreign credentials,
403/404 for all three foreign IDs, nonempty HTTP lexical deterministic retrieval,
actual CLI results and citations confined to imported IDs, no model-generated
answer/usage, and denied capture writes. The CLI currently uses its default
retrieval/answer mode: with no embeddings/graph corpus or provider credentials,
this is a keyless fallback check, not evidence that the CLI has a lexical-mode
flag. Outbound denial must be independently verified by the clone orchestrator;
the helper deliberately does not claim to count provider calls.

Only status, counts and safe failure codes may leave the isolated process.
Never print raw command stderr, query text, answers, source paths or API bodies.
Stop the exact disposable application processes after checking; retain private
evidence and remove only explicitly identified disposable state when approved.
Do not copy clone authentication or runtime journals back to staged databases.

```sh
node --test scripts/areas/application-probe.test.mjs
```

## Recorded four-partition smoke check

On 2026-09-20, the verified inactive imports were copied into four disposable
PostgreSQL/application pairs. Each pair passed the actual HTTP and CLI checks:
Workshop returned 8 results/2 citations, Homelab 8/3, Trust 2/2 and General 8/3
through each interface. All returned IDs belonged to the respective partition.
Each pair denied three invalid authentication cases, one record ID from each
other partition, and a capture write. Table and four-sequence fingerprints of
the original targets and sealed clones remained unchanged. Only operational
auth/effect journals were excluded from the table fingerprint. All eight
application/database containers were subsequently confirmed stopped.

The private metadata-only evidence is
`application-clones-20260920-0013/report.json` within the approved staging root,
SHA-256 `e37f80d49184f3f342950b4ac29eba990945a8f6eb2db9fe8e1d6281f0f3c626`.
It binds to independently verified imports, uses the approved pinned application
binary/images, synthetic scoped credentials and loopback-only network namespaces.
The Node runtime was invoked explicitly as Node, not as a PDS service.

This is a real application-level staging smoke check, not a retrieval-quality
benchmark, CA migration, production activation or proof of every possible
authorization path. No model credentials or external routes were supplied;
the CLI's default keyless fallback remains distinct from the explicitly lexical,
deterministic HTTP request. R3 remains paused.
