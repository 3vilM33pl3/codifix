# Project-selective synchronization implementation boundary

This is the source-backed boundary for selective synchronization. The gateway
feed, signed-page verification and separate read-only projection are implemented;
enrolled-device delivery and automatic laptop execution are still in progress.
It is not a declaration of a working Mac journey. The approved [contract](README.md)
remains authoritative.

## Existing mechanisms to reuse

- `mem-federation-runtime/src/security.rs` verifies enrolled receipt keys and
  publication signatures. Individual receipt verification is distinct from
  complete history verification.
- `mem-federation-store/src/audit.rs::pull_audited` returns an **instance/space**
  stream. It cannot be exposed as a laptop subscription and filtered afterward.
- `mem-federation-runtime/src/context.rs` deliberately excludes imported content
  from ordinary local search. A searchable laptop projection therefore needs an
  explicit, separate implementation rather than changing that default globally.
- The publication `StreamId` binds origin instance and canonical UUID. It does not
  carry a project UUID. `SourceVersionRef` is also not a project authorization
  claim. Source-side sharing storage already has project ownership; that binding
  must be authenticated before it becomes a scoped delivery policy.

## Scope and authorization

Identify a subscription by company, area, owning instance, stable project UUID
and space. Slugs are display information, never a cross-instance identity.
The company console requires explicit authority over each area; creating a local
catalog entry does not grant membership in a separate area's database.

The server checks the current human identity, approved device/project scope,
catalog revision, offline-copy policy and source sharing eligibility **before**
selecting any content. Trust has no offline subscription. Area unavailability is
an error, not fallback into General. Disabling a human or revoking a device stops
new transfers; an old approval is not sufficient authority.

Select only explicitly approved, verified published versions and their reviewed
evidence. Do not serialize raw local memory responses: those may include paths,
captures or provenance fields never reviewed for disclosure. Do not follow an
arbitrary causation, relation or proposal reference into another project. A
necessary dependency outside the selected scope blocks the page or requires a
bounded authenticated representation; it is not permission to disclose it.

## Attested pages and local projection

The implemented gateway registration API is a **patch**, not a replacement.
Owner adapters retain the prior registered set and explicitly disable removed or
ineligible streams. A current review covers only its exact published keys; neither
catalog membership nor an old review permits a union of historical content.

Scoped pages need a distinct signature purpose and bind the device audience,
subscription, exact manifest/policy revision, request nonce, cursor interval,
content digest and expiry. Use the enrolled gateway receipt trust; never learn a
trust key from an untrusted page. Preserve existing publication bytes, receipt
ordering and schema-v2/v3 interpretation unchanged.

A filtered page is not a contiguous publication history. Do not run the complete
space reducer over it, claim a complete repository proof, or download a CAR to
fill its intentionally omitted records. Verify the scoped attestation and each
included content digest/receipt, and state the verification boundary explicitly.

Use a subscription-specific commit-safe cursor rather than assuming a filtered
global sequence proves completeness. Snapshot pagination binds one consistent
snapshot; delta pages include withdrawals and tombstones. A missing cursor
history produces explicit resynchronization, not an empty successful page.

Persist the received page, read-only remote projections and cursor in one
transaction. Exact replay is harmless; changed bytes under the same page identity
fail closed. Remote keys include owner identity so identical project/canonical
IDs at different owners cannot collide. Local capture/proposals remain separate
from remote authoritative history.

Search and citations label the remote owner, exact source/shared version, author,
reviewed evidence and digest. Local mutation routes cannot edit the projection.
External model forwarding still needs the existing explicit destination policy;
offline-copy permission is not model consent.

## Focused acceptance evidence required

- A denied project sentinel is absent from the serialized server response, not
  merely absent from the laptop's display.
- Same-named projects at separate owners remain distinct.
- Catalog changes, source withdrawal, device revocation and disabled humans are
  checked before delivery.
- Interrupted snapshot/delta application rolls back projections and cursor;
  duplicate pages and altered retries behave correctly.
- A laptop performs cited read-only search offline, reconnects and receives an
  authorized update without gaining another project's bytes.

No test of the existing whole-space pull substitutes for these checks. Clean-Mac
login, Keychain, local database provisioning and services remain native acceptance
work, even after the scoped-feed software passes Linux tests.
