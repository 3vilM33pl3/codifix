# ADR-0008: Protocol-neutral federation spike contract

Status: accepted; the bounded spike selected ATProto as an adapter/profile.
Date: 2026-09-04

## Context

Memory Layer is exploring a federation proof that must stay offline, non-production, and bounded. The spike is intentionally ahead of the normal Notion plan order so the team can test the transport and formal model before any productization work is allowed to branch out.

This document defines the spike contract. The completed evidence report is in
[`docs/future-development/federation-protocol-spike.md`](../../future-development/federation-protocol-spike.md).
The standing protocol choice and rationale are recorded in
[ADR 0011: Adopt AT Protocol as a federation adapter](0011-atproto-federation-adapter.md).

## Contract

The spike must produce all of the following, without adding database migrations, service routes, CLI/UI behavior, live PDS integration, gateway behavior, or a production publication path:

- a pure `mem-federation` Rust crate that depends on `mem-record` only for shared record data and stays free of Tokio, SQL, Axum, configuration, and ATProto crates;
- a transport-neutral record model with:
  - `SharedRecordEnvelope`
  - `TransportRecord`
  - `FederationRecord` variants `MemoryVersion`, `CanonicalState`, `Evidence`, `Relation`, `Membership`, and `Revocation`
  - `DedupKey`
  - `ProjectSpaceTransport`
  - `ReplicaState`, `ApplyOutcome`, `RejectReason`, `QuarantineReason`, `apply`, and `replay`
- a projection from current `MemoryEntryResponse` data into memory-version, canonical-state, and evidence records, while preserving tombstones, canonical/version IDs, provenance, relations, authorship, and timestamps;
- a membership model with separate federation capabilities for `Pull`, `Publish`, and `ManageMembership`, without mapping those capabilities onto the current application `PermissionSet`;
- deterministic RFC 8785 JSON plus SHA-256 as the reference encoding, with rejection of non-finite numbers and invalid identifiers;
- an isolated `mem-atproto` adapter that depends on `mem-federation` and the pinned Atrium crates only, retains unknown fields, and keeps the ATProto surface isolated from the neutral core;
- six unpublished spike Lexicons:
  - `dev.memory-layer.unspecced.memoryVersion`
  - `dev.memory-layer.unspecced.canonicalState`
  - `dev.memory-layer.unspecced.evidence`
  - `dev.memory-layer.unspecced.relation`
  - `dev.memory-layer.unspecced.membership`
  - `dev.memory-layer.unspecced.revocation`
- a handwritten Lean model under `formal/memory-federation` that proves the reducer properties against the same conformance traces used by Rust;
- a shared trace set covering happy path, duplicate delivery, equivocation, version gap, missing evidence, tombstone, and post-revocation write scenarios;
- a spike report that recommends exactly one of:
  - ATProto as an adapter/profile;
  - a bespoke external protocol;
  - inconclusive, if and only if an isolated Atrium/tooling defect blocks the proof.

## Boundaries

The neutral federation core must not absorb ATProto types, runtime assumptions, or production concerns.

Reducer rules for the spike are:

- applied records are idempotent under duplicate delivery;
- the same locator with a different digest is rejected as equivocation;
- data publication requires an active publishing membership;
- membership changes require the configured space authority;
- revocation blocks later writes without erasing previously accepted records;
- versions must be monotonic, and missing dependencies or version gaps are quarantined for resync;
- rejected records never alter projections.

## Decision Gate

The final spike report must evaluate the trace and adapter evidence, then pick one outcome only.

Proceed with ATProto as an adapter/profile only if every required field round-trips, unknown fields survive, MST behavior is deterministic, and no AT-specific type or assumption enters the neutral core.

Choose a bespoke external protocol if ATProto causes semantic loss, requires core contamination, or cannot preserve append-only, versioned, provenance-backed Memory behavior.

Choose inconclusive only for an isolated Atrium/tooling defect, and include a minimal reproduction instead of claiming protocol suitability.

## Non-goals

The spike explicitly does not cover:

- private permissioned spaces;
- signatures;
- encryption;
- OAuth;
- unified live cursors;
- gateway operation;
- network liveness;
- live PDS operation;
- service routes;
- CLI or UI publication flows;
- production database changes.

## Consequences

The neutral Rust model remains the source of federation semantics. ATProto is an
external profile only, so the core does not inherit AT identities, repository
layout, wire types, or public-network assumptions. A bespoke external protocol
is deferred unless a later security or live-network spike demonstrates semantic
loss that cannot be isolated in the adapter.

The outcome does not approve private spaces, signing, encryption, OAuth,
identity resolution, unified live cursors, a gateway, network liveness, live PDS
operation, or any production publication path.
