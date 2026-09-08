# Private federation profile: compatibility and schema governance

Status: production contract, not a released federation service. Read with
[ADR 0009](../adr/0009-private-federation-v1.md) and the
[execution checklist](../../reviews/federation-first/README.md).

## Reproducible evidence

The machine-readable [upstream lock](../../../fixtures/federation/upstream-lock.json)
pins the tested PDS image/source, Atrium versions, interoperability fixtures and
the SHA-256 of the assessed Spaces proposal at commit
`35a2d378297d9178561a5cc643c74a1262122266`. Updating this file is an explicit
compatibility review, not a floating dependency upgrade.

The signed-repository adapter and PDS cold-restore tests cover the private
gateway candidate. Native Spaces **has not been run** here. Its August 20 alpha
announcement explicitly excludes production and sensitive-data use. There is no
alpha image dependency in the production candidate or PDS recovery test.

## Two different profiles

| Concern | Memory private-gateway candidate | Native Spaces proposal at the pinned revision |
| --- | --- | --- |
| Authority | One Memory authority per space; origin instance owns its stream | Space authority DID; separate per-user, per-space repositories |
| Address | Neutral identifiers mapped to ordinary AT URIs | AT URI with a `space` path segment |
| Repository | Signed MST/CAR transport behind an isolated gateway | LtHash set digest; different commit construction |
| Authenticity | Repository signer plus independently authenticated publisher attribution | Sync authenticity; leaked commits do not prove contents to third parties |
| Read boundary | Enrolled instance capabilities checked by Memory gateway | Whole-space credential bound to the client with DPoP |
| Sync | Ordered Memory publication prefix and durable receiver cursors | Direct repo sync, revisions, notifications and periodic reconciliation |
| Revocation | Deny future gateway access; cannot recall delivered bytes | Space authority stops issuing credentials |
| Deletion | Explicit Memory content-retention and audit policy required | Space deletion expects syncers to delete their copies |
| Audit | Memory receipts separate from PDS state | Repository synchronization is not an archival audit contract |

Source: [pinned permissioned-data proposal](https://github.com/bluesky-social/proposals/blob/35a2d378297d9178561a5cc643c74a1262122266/0016-permissioned-data/README.md).
This comparison does not claim interoperability. In particular, public-MST
determinism does not test Spaces commits. Both profiles leave plaintext visible
to admitted infrastructure; neither is claimed as end-to-end encryption.

## Pinned SDK coverage and gaps

`cargo test -p mem-atproto --all-targets --locked` executes in-memory requests for
record create/apply/list, binary repository export/import, refresh credentials,
bodyless activation and authentication-failure propagation. Native Spaces URIs
are rejected by the existing adapter, not interpreted as public AT record paths.

The generated API supplies these ordinary PDS endpoint contracts without
default features. The custom `HttpClient` remains responsible for endpoint
allow-listing, redirects, TLS, deadlines, response limits and HTTP framing
(including import `Content-Length`). No automatic session refresh, credential
storage, signing-key verification or safe DID resolution is established by the
request tests. Those belong to the runtime/security adapter, not mem-federation.
`atrium-repo` covers the MST proof, not native Spaces repository verification.
The pinned generated API has no `com.atproto.space` namespace.

Before a native Spaces implementation can be selected, run separate test-data
conformance for credentials, addressing, commits, synchronization, revocation,
deletion and audit-policy compatibility. Record the exact runtime image digest
and SDK commit then; do not claim a runtime pin or passed test that does not exist.

## Authenticated publisher versus original author

`mem-federation` now offers `AuthenticatedRecord`, `apply_authenticated` and
`replay_authenticated`. Membership, publication and revocation checks use the
resolved publisher, while memory projections preserve the original author. An
accepted source/locator cannot subsequently be attributed to another publisher.
Replay inputs must retain authenticated attribution; replaying just payloads
would lose that distinction. The wrapper is not deserializable from wire JSON.

The caller must authenticate the transport and bind the source repository to an
enrolled instance before constructing this input. A constructor is not proof of
identity. The old `apply`/`replay` methods remain offline-spike conveniences that
assume author equals publisher; they must not authorize production ingestion.
There is no production ingestion route yet. The existing `AuthorIdentity` type
still represents principals in this legacy spike interface.

Nine Rust/Lean traces cover the modeled reducer, including independent authorship
and rejected forged authorship. This is not proof of runtime authentication,
durable ordering or two-instance product operation.

The new [`publication` domain](federation-publications.md) uses distinct non-nil
UUID instance/space identities, origin-qualified streams, shared/local counters,
state revisions, reviewed evidence, proposals and relation retractions. Its
in-memory reducer requires a separately authenticated publisher and contiguous
authority acceptance order. Authentication and establishing that committed order
remain caller duties, not properties supplied by deserialization or a counter.
Five publication Rust/Lean traces now cover every variant, including evidence,
proposals, relation retraction, causation and independent capabilities. Scoped
proofs accompany them; validated UUIDs, review digests and encoding remain
prerequisites, not proofs of transport authentication or human approval.

This versioned neutral-domain extension now has its own schema-v2 adapter and
eight-variant round-trip/unknown-field tests in `mem-atproto::publication`. The
six spike Lexicons and MST fixtures are unchanged. The additional unpublished
`dev.memory-layer.unspecced.publication` wrapper carries a validated neutral
delivery; its permissive Lexicon `unknown` field is not a semantic validator.
Rust validates schema, identifiers, known fields and digests before acceptance.
Unknown extensions survive at the adapter boundary; unknown semantic kinds or
versions fail closed. Real PDS create/readback, independently calculated CID and
signed CAR verification in the disposable harness are separate evidence from MST
determinism. See [Stage C runtime](federation-durability.md).
The [local permission policy](federation-permissions.md) is implemented as a pure
intersection of exact-project permissions, explicit local sharing grants and
independent gateway capabilities, with no implicit admin/role bypass. Production
local grants are persisted and independently checked again before transmission.
The opt-in runtime pins administrator-configured origins and tokens, disallows
redirects/proxies and bounds response sizes/time. Automatic session refresh,
live DID/signing-key verification, TLS deployment, private network enforcement
and production namespace publication remain D/E/deployment gates.

## Namespace ownership and publication gate

- `dev.memory-layer.unspecced.*` is an unpublished fixture namespace, never a
  stable external contract. Existing spike records remain readable in tests.
- Production publication requires recorded control of the reverse-DNS namespace
  and explicit approval of its owner. Ownership is **not verified by this file**;
  do not publish production Lexicons or register DNS until that evidence exists.
- The registry must identify the owner, immutable schema content/hash, protocol
  version, data classification, compatibility policy and deprecation status for
  every allowed record type. No arbitrary client-supplied collection is accepted.
- Memory validates each selected schema before acceptance/outbox insertion.
  Upstream `validate:false` is only a mechanism for unpublished custom records,
  never a replacement for Memory validation.
- Additive optional fields may be forwarded without understanding them. Retain
  unknown fields at the adapter boundary, but never interpret them as permissions,
  executable instructions, author identity, or trusted evidence.
- Unknown record kinds, capability values and unsupported semantic versions fail
  closed. A breaking change requires a new explicit protocol/schema version,
  migration/export strategy and conformance evidence; never reinterpret old bytes.
- Keep immutable publication versions separate from mutable-state revisions.
  Normalize confidence to integer ppm before producing the committed shared
  representation. Track neutral digests and transport CIDs separately.
- Test old-writer/new-reader and new-writer/old-reader behavior. Canonical encoding,
  unknown-field preservation, review-byte equality and deterministic reducer
  conformance are release checks for schema changes.

## Release and operating limits

The PDS prerequisite now demonstrates cold restoration of account data, runtime
credentials, signing keys, repositories and blobs into a fresh volume. It does
not prove a Memory gateway, two-instance synchronization or fencing of restored
writers. Those remain product release gates.

Keep raw data local by default. Original authorship and authenticated publisher
identity are distinct. A signed claim is not automatically true. Revocation,
content deletion and audit retention are distinct operations; offline-copy
policy and legally permitted retention must be explicit in deployment agreements.
Do not advertise native Spaces compatibility until its deletion and receipt
semantics have been reconciled without weakening that contract.
