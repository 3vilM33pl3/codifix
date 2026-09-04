# Federation Trace Schema

These fixtures are shared trace inputs for the Lean federation spike.

The schema is intentionally flat so a Rust `serde` mirror can map it directly.

Top-level shape:

```json
{
  "name": "happy-path",
  "records": [ ... ],
  "expected": {
    "outcomes": ["acceptedGrant", "acceptedEvidence", "acceptedMemory"],
    "summary": {
      "seenRequestCount": 3,
      "evidenceCount": 1,
      "memoryVersionCount": 1,
      "activeCanonicalCount": 1,
      "activePublishers": ["alice"],
      "revokedPublishers": [],
      "canonicalHeads": [ ... ]
    }
  }
}
```

Each `record` has these shared fields:

- `requestId`: replay key
- `authority`: authenticated authority identity
- `publisher`: authenticated publisher identity
- `kind`: `grant`, `revoke`, `evidence`, or `memory`

Optional fields are used by the matching kind:

- `subject`, `capability` for `grant` and `revoke`
- `evidenceId`, `source`, `locator`, `digest` for `evidence`
- `canonicalId`, `version`, `author`, `source`, `locator`, `digest`,
  `evidenceRefs`, `tombstone` for `memory`

The expected `outcomes` list uses:

- `acceptedGrant`
- `acceptedRevoke`
- `acceptedEvidence`
- `acceptedMemory`
- `duplicateReplay`
- `unauthorized`
- `revokedPublisher`
- `versionGap`
- `missingEvidence`
- `equivocation`
- `invalidShape`

`canonicalHeads` is a list of the latest version per canonical memory:

```json
{
  "canonicalId": "mem-1",
  "version": 1,
  "publisher": "alice",
  "author": "alice",
  "source": "repo",
  "locator": "docs/spec.md#claim-1",
  "digest": "sha256:cf2a088307621323c037f42566a7870f6f2c39e88f674e422a47f5df3265f321",
  "evidenceCount": 1,
  "tombstone": false
}
```
