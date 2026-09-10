# ADR 0011: Adopt AT Protocol as a federation adapter

Status: accepted; protocol selection does not establish production readiness.
Date: 2026-09-10

## Context

Memory Layer needs to exchange selected, curated knowledge between independently
administered instances while preserving separate databases, provenance, immutable
publication history and explicit sharing policy. Federation must support local
work and retained offline copies when another instance is unavailable.

[ADR 0008](0008-protocol-neutral-federation-spike.md) defined an offline experiment
to test whether AT Protocol (ATProto) could represent Memory's federation
semantics without semantic loss or protocol dependencies in the domain. The
[completed spike](../../future-development/federation-protocol-spike.md) selected
ATProto as an adapter/profile: required fields and unknown extensions
round-tripped, repository construction was deterministic, and the neutral
reducer retained its invariants.

The [federation-first roadmap](../../reviews/federation-first/README.md)
subsequently made federation part of the first paid-pilot scope.
[ADR 0009](0009-private-federation-v1.md) defines the private-space authority,
publication and trust contract. This ADR records the standing protocol choice
and its rationale. It complements both records without superseding their
boundaries or release gates.

## Decision

Adopt ATProto as the external record and repository profile for Memory
federation, behind a transport-neutral Memory domain.

1. **Memory owns application semantics.** Keep the Rust `mem-federation` core
   responsible for record meaning, publication lifecycle, membership capabilities,
   authority rules and deterministic state transitions. ATProto types, network
   clients, configuration and persistence dependencies remain outside it.
   Extend the Rust reducer and its scoped Lean conformance model together.
2. **Adapters own protocol representation.** Use Memory-specific Lexicons and
   Rust Atrium adapters to map validated neutral records to ATProto identifiers,
   XRPC requests and repository records. Keep neutral semantic digests distinct
   from transport content identifiers (CIDs). Signature verification and live
   I/O remain separate from pure encoding and domain logic.
3. **Memory owns ordered delivery and durable history.** Preserve accepted
   application order, transactional outboxes/inboxes, immutable publications,
   reviewed evidence and receiver cursors independently of repository state.
   Repository keys, revisions and firehose positions do not define Memory's
   authority order or replace its audit history.
4. **Start with the private gateway profile.** Use the authoritative Memory
   gateway per space and unmodified upstream Personal Data Server (PDS) from
   ADR 0009. Only the gateway accesses PDS credentials and endpoints. Local
   permissions and independent gateway capabilities control publication and
   access; ordinary ATProto repositories do not provide that Memory policy.
5. **Keep compatibility explicit.** Follow the
   [profile and schema policy](../architecture/federation-profile.md), including
   pinned upstream evidence and the namespace publication gate. Preserve unknown
   extensions at the adapter boundary while rejecting unsupported semantic
   versions and record kinds. Selecting ATProto does not publish the current
   experimental Lexicons as a stable external contract.

Native ATProto Spaces remains a separate compatibility candidate. Adopting
ATProto does not select its evolving private-data mechanisms or establish native
Spaces interoperability. MCP remains the local tool interface; remote agent
task interoperability, including any A2A integration, is a separate concern.

## Rationale

ATProto provides reusable primitives at the boundary Memory needs:

- [DIDs](https://atproto.com/specs/did) identify accounts and associate them with
  service locations and verification keys. Memory still binds those identities
  to enrolled instances and its own authorization policy.
- [Lexicons](https://atproto.com/specs/lexicon) describe application-specific
  records and APIs, while [XRPC](https://atproto.com/specs/xrpc) supplies their
  HTTP conventions. Memory can define its knowledge records without adopting
  Bluesky's social application model.
- [Signed, content-addressed repositories](https://atproto.com/specs/repository)
  and [repository synchronization](https://atproto.com/specs/sync) provide
  verifiable record transfer, exports and change-stream primitives. These are
  reusable protocol mechanisms; the private profile still needs its own
  authorized gateway delivery path.

Reusing these mechanisms reduces the external protocol machinery Memory must
design and maintain. The decisive project evidence is the spike's demonstrated
semantic fit and isolated adapter boundary. It supports this selection without
assuming that an existing ecosystem supplies Memory's full product contract.

The separation also matters where semantics differ. Ordinary ATProto repositories
support record updates and deletion without retaining historical tombstones;
they are not Memory's immutable event archive. Memory therefore retains its own
publication history, retention rules and projections, consistent with the
[immutable-version decision](0007-immutable-versions-and-event-log.md).
See the [repository specification](https://atproto.com/specs/repository).

## Alternatives considered

- **Build a bespoke external protocol now.** Deferred because the spike found
  no semantic loss requiring one. It would add responsibility for identity
  conventions, schemas, repository verification and synchronization. Reconsider
  if verified incompatibilities cannot be isolated in an adapter while
  preserving Memory's invariants.
- **Make ATProto the internal domain model.** Rejected because wire identities,
  repository layout and protocol lifecycle would spread into authorization,
  persistence and reducer semantics. Keeping the core neutral allows protocol
  evolution without redefining the knowledge model.
- **Publish private knowledge through ordinary public ATProto repositories.**
  Rejected because public repository and sync interfaces do not supply the
  required access boundary. In particular, ordinary repository export is
  unauthenticated; sensitive records must stay behind the isolated gateway.
  See the [sync specification](https://atproto.com/specs/sync).
- **Depend on native Spaces for the initial private implementation.** Deferred
  pending separate compatibility and operating gates in the profile policy.
  Replacing the selected gateway profile requires a subsequent decision.

## Consequences

Memory gains an existing external protocol foundation and a bounded adapter
surface. It still owns authorization, reviewed publication, ordered delivery,
key trust, recovery, retention and deployment isolation. The gateway is a trusted
authority and an availability dependency for sharing; adopting ATProto does not
provide peer consensus or automatic failover.

Verified signatures establish authenticity and integrity within their verified
scope. They do not establish factual truth, increase a memory's confidence,
prove human approval or provide end-to-end encryption. Original authorship stays
distinct from the authenticated publisher. Admitted recipients and infrastructure
administrators can read plaintext, and revocation cannot recall delivered bytes.

The offline selection evidence, subsequent controlled integration evidence and
production release gates remain separate. Track current implementation and
remaining gates in the [execution roadmap](../../reviews/federation-first/README.md)
and its linked evidence. Changes to upstream pins, schemas or the private profile
must preserve the adapter boundary and pass the relevant compatibility,
Rust/Lean conformance, security and recovery checks.
