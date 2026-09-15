# Bracken evidence desk

A fictional internal evidence publisher. Its mutable capture buffer, review
snapshots, correction proposals and fake transport are separate concerns.
No network, database, credentials or external Rust dependencies are used.

Run with `cargo run --offline`; commands are whitespace-delimited. Body and
identity fields are single tokens for this small fixture protocol.

- `capture AUTHOR BODY`: replace the mutable working capture.
- `grant PUBLISHER` / `revoke PUBLISHER`: operator-controlled local policy.
- `review REVIEW_ID PUBLISHER`: freeze a snapshot for the next shared version.
- `publish REQUEST_ID REVIEW_ID PUBLISHER none|before|after`: fake transport.
- `propose PROPOSAL_ID BASE_VERSION EDITOR BODY`: propose a version-scoped edit.
- `approve PROPOSAL_ID REVIEW_ID PUBLISHER`: review a correction for publication.
- `show`: list accepted immutable versions with attribution.

The starter passes a simple publication but has historical review, correction
and retry defects. The task contract is in contract.md. Private acceptance tests
are not part of the agent repository. This is a deterministic in-process store,
not a disk-durability or cryptographic implementation.
