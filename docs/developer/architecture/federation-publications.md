# Origin-owned publication domain

Implementation contract for roadmap B1/B2/B3; not a deployed protocol. The legacy
six-record spike remains a compatibility fixture. `mem_federation::publication`
extends the same neutral crate with a production-shaped schema, reusing canonical
JSON, digests, authorship and capabilities. No alternative transport, new protocol
stack, production Lexicon namespace or automatic legacy migration is introduced.

## Identities and ordering

Instance, space and record identifiers are non-nil UUIDs, independent of names,
URLs, DNS, database keys or signing keys. A stream is `(origin instance, canonical
record ID)` within a space. Envelope publisher must equal the independently
authenticated instance; original author is attribution, never permission.

One configured space authority manages membership. Data needs a current Publish
capability. Receivers apply a contiguous authority-assigned acceptance sequence,
not authored time, UUID/repository key order or an allocated database sequence.
All counters are bounded positive integers; exhaustion fails closed. The domain
models committed order, but does not itself provide the database transaction or
writer fencing needed to establish that order.

The [local permission policy](federation-permissions.md) independently intersects
exact-project permissions, explicit sharing opt-in and current gateway capability.
Neither local role presets nor a delegated ManageMembership capability establish
space authority. This check gates intent; the gateway must authenticate and
recheck current authority within its future acceptance transaction.

Accepted records bind publisher/record ID, opaque transport locator, digest and
acceptance sequence. Exact duplicates are no-ops; changed bindings are rejected.
Gaps and missing dependencies wait for resync. Rejections/quarantine do not advance
the cursor or change projections. Membership revocation preserves prior history.

## Publications

Only the origin appends its stream. Shared version 1 may represent local source
version 27; source versions may skip between publications. Shared versions are
contiguous and independent of local versions. Local source IDs/versions are
explicit provenance, not shared sequence allocation. State revision is another
contiguous counter; it targets the current shared content version. State updates
do not create content versions, and a new content version does not reset state
revision. Until a state update targets the new content version, consumers must
not treat the old state as a state assertion about that new version.

Evidence snapshots include only explicitly selected text and a public source
label, bound to the space and origin stream. No raw captures, sessions, file paths
or complete source objects are automatically projected. A review receipt binds
the canonical snapshot bytes, reviewer and review time; changed bytes or context
invalidate that receipt. Digest agreement is not proof of human review: the future
service must authenticate the reviewer and persist the review decision. Evidence
records are immutable and references are qualified by their publisher.

Recipients submit proposals against an exact origin stream/version. They cannot
overwrite it. Origin approval atomically appends the proposed content as the next
shared version and resolves the proposal; stale approvals fail. Rejecting a stale
proposal is allowed. The origin chooses the local source record/version when
approving; an untrusted proposer cannot assign local provenance. Relations belong
to their source origin and have independent revisions for assertion/retraction.

## Deliberate limits

This domain is a deterministic in-memory reference, not the storage engine.
The separate [durability crate](federation-durability.md) now composes it with
PostgreSQL transactions; service and live transport integration remain open.
No authentication,
signature checking, review UI, local grant persistence, PDS publication, model forwarding,
network recovery or durable cursor is implied by its constructors. Imported strings
remain untrusted data. Offline copies cannot be recalled. Production adapter and
schema compatibility tests must be added before exposing these records externally.
