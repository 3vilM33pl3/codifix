# ADR 0009: Private federation v1

Status: implementation in progress; live interoperability gate not yet passed.

The approved [federation-first execution roadmap](../../reviews/federation-first/README.md)
requires two independently operated Memory instances in the first paid pilots.
The private gateway profile below remains the implementation candidate. Native
ATProto Spaces alpha is a separate, non-sensitive compatibility experiment, not
a replacement production dependency or a claim of native Spaces interoperability.
The [profile compatibility and schema policy](../architecture/federation-profile.md)
pins that distinction and the namespace publication gate.

## Decision

Memory owns the federation domain and an authoritative gateway per space. An
unmodified, digest-pinned upstream PDS supplies private signed AT repositories.
This is a private Memory ATProto profile, not standard public ATProto access
control. The offline spike does not establish production suitability.

Use explicit publication of curated memories with reviewed evidence snapshots.
Later content/state changes synchronize automatically; evidence expansion needs
renewed review. Origin instances own streams; other members submit proposals.
Support offline copies and Web/CLI/TUI parity. Revocation denies future gateway
access but cannot recall previously delivered content.

The production domain needs stable space/instance IDs, publisher distinct from
author, shared versions independent of source versions, separate mutable state
revisions, immutable evidence, relation retraction, proposals and authority order.
Normalize shared confidence to ppm before hashing. Keep runtime, SQL and ATProto
types outside mem-federation. Extend executable Rust/Lean conformance together.

## Trust boundaries

- Infrastructure administrators and enrolled instances can see plaintext. This
  is not end-to-end encryption or protection from a malicious space authority.
- Local users require local federation permissions; enrolled instances also
  require independent gateway space capabilities. Clients cannot claim another
  publisher. Authenticated publisher attribution is distinct from original author.
- Only the gateway may access PDS credentials and write repositories. Member
  instances cannot access raw PDS endpoints, cross-space CAR files or firehoses.
- DID resolution must not allow arbitrary SSRF. Private deployment must work
  without globally disabling upstream SSRF protection or using development mode
as a production workaround.
- Raw captures, sessions, task logs, local absolute paths and unselected related
  memory content stay local. Reviewed snapshot bytes must equal published bytes.
- Imported content is data, not executable instructions. External model forwarding
  requires explicit space policy approval. Ordinary retention cannot erase pending
  delivery or accepted federation history.

## Ordered delivery

The source outbox commits with source mutations. Gateway authorization, reducer
application, event sequence and outbound intent commit together. PDS publication
is an idempotent subsequent step. Receivers consume only the contiguous published
prefix and atomically commit inbox, projections and cursors. Queued, accepted,
published and replicated are distinct states. Database sequences alone do not
establish commit order. Never use repository key order as application order.

## Mandatory first gate

Before product integration, verify an isolated upstream PDS supports private
did:web account provisioning/activation, publication, signed repository
verification, key rotation and recovery with no public exposure. If a PDS fork
or broad security bypass is required, stop and report rather than silently
substitute a different design. Downstream work remains unimplemented until the
gate passes or the user explicitly approves a revised contract.

The [disposable PDS gate](../../../tests/federation-pds/README.md) now includes a
complete encrypted cold-volume restore, tested locally on 2026-09-07 after
removing its source volume. This passes the bounded PDS prerequisite, not the
Memory product gate. [C/D implementation evidence](../../reviews/federation-first/2026-09-09-d-completion.md)
now covers controlled multi-database integration, gateway isolation, durable
receipts/cursors and explicit key/writer fencing. Real independently administered
user workflows, complete deployment recovery and paid-pilot release remain gated.

## Delivery and exclusions

Implement domain/proofs, transactional persistence, gateway/synchronization,
publication/retrieval/proposals, user-interface parity, then production packaging
and release gates, in small commits. Federation is disabled by default. One
active writer per space is fenced by the database. No public Bluesky integration,
general-purpose PDS implementation, peer consensus, multi-region automatic
failover, Neuralese or Aeneas. Existing infrastructure and Notion are unchanged;
live deployment and DNS publication require separate targets and authorization.
