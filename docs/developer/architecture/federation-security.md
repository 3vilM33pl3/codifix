# Private-profile trust, use and retention

Stage D implements this boundary after authenticated local review and neutral
domain reduction. It does not change the Rust/Lean reducer or prove the service,
cryptography, operating system, human reviewer, content truth or customer recovery.
The private profile is not native Spaces interoperability.

## Publication and receiver proofs

The fixed gateway verifies each immutable PDS record's exact JSON, DAG-CBOR CID
and signed full-repository inclusion under its explicitly enrolled repository
DID/key. Only then does it sign an independent, domain-separated SHA-256/ECDSA
attestation with a **different** audit key. Canonical JSON claims bind:

- purpose, space, authority and committed application sequence;
- hash of the complete neutral delivery (publisher, author, locator, digest,
  evidence and source/shared versions included);
- previous receipt hash and the bounded audit-key generation;
- frozen publication policy and verified repository/record/commit metadata.

`federation_audit`, publication acknowledgement and the published prefix commit
together. Unsigned acknowledgement cannot pass after explicit security enrollment.
No PDS repository/CAR or unrelated space content is sent to participants.

Each pull response additionally signs a fresh client nonce, exact page, high-water
mark and current policy using the latest enrolled audit key. The client validates
nonce, scope, signatures, key sequence windows, receipt chain, contiguous order
and wire bindings before storage. Receipts/inbox/projection/cursor commit atomically.
`verified_snapshot` reads records and receipts under the same replica lock and
verifies them again on restart/offline use. Unsigned reference-store APIs cannot
update an enrolled replica. Storage remains a trusted in-process API, not a public
cryptographic verifier or arbitrary SQL-access boundary.

Existing unsigned published prefixes are refused, not retroactively certified.
Explicit reconstruction/resync must retain the known complete prefix, receipts
and trust schedule; truncation, missing proofs and changed histories fail closed.
The entire reference history is replayed, so current 16 MiB/32-record page limits
and O(history) costs remain sizing constraints.

## Enrollment, rotation and recovery

There is no TOFU, dynamic DID discovery or fallback to arbitrary old keys. Public
compressed `did:key` encodings are validated locally; no identity or record URL
is fetched. Operators establish repository DID/key, space UUID, authority UUID,
receiver identity, gateway origin/CA, audit-key schedule and scoped bearer tokens
through a separate trusted administration channel.

For an audit-key rollover, obtain the last published sequence H, append exactly
one generation with `first_sequence = H + 1`, and distribute the appended trust
schedule to receivers explicitly. Configure the matching new private key and
replacement instance credentials. Startup serializes enrollment, key/policy change
and a new writer epoch in one transaction. Old gateway readers/writers fail that
epoch check; old credentials fail on the new process. Receivers reject key rollback,
changed historical windows and old-key signatures for new sequences. Old receipt
signatures remain valid only within their original sequence window. Stop/retry if
H changed while preparing rollover; never guess an allocated database sequence.
A pre-publication empty space has no nonempty old-key window: recreate the empty
enrollment only through an explicit administrator procedure, not automatic rollover.

PDS signing-key replacement remains a separate upstream migration: stop outbound
work, verify the new DID/repository binding through the approved channel, replace
the pinned key/token and retry the exact blocked record. Existing receipts retain
the old transport-key attribution; the gateway never silently discovers a key.

Before restoring a gateway, isolate/stop old writers and ingress, restore the
database, audit/key schedule, PDS account/keys/records and credentials consistently,
verify receipts and the retained prefix, rotate audit key/credentials with an
explicit sequence cutover, then re-enable approved clients. Never run a restored
database and an old independently writable copy as authorities simultaneously.
Epoch fencing protects processes sharing a database, **not** split-brain clones
with independent databases. A complete offsite recovery drill and stale-writer
containment on the actual deployment remain mandatory release gates.

## Explicit external-model use

Imported records stay in federation storage, not local embeddings, automatic
curation, reinforcement or global search. Boundary checks guard those implicit
pipelines. `retained_context` requires a request-scoped current local read
permission and explicit local space grant; it selects current non-archived,
non-tombstoned streams with reviewed evidence/proposal dependencies. Causal IDs
are preserved but arbitrary causal links do not expand the selected content.
Space, origin, original author, versions, locator/digest and receipts remain in
the untrusted-data context. No signature changes confidence or establishes truth.

`InstanceSync::answer_shared` is the guarded tool-free provider interface for later
E integration, not a configured commercial model or remote-expertise feature.
It requires all of: current local read/export permission and enabled space grant,
an explicitly configured local provider ID, immutable policy for **every selected
publication/dependency**, and a fresh signed gateway policy/current Pull access.
Empty provider sets deny. Publication policy freezes during acceptance, not a
later retry. Tightening current policy blocks new calls; broadening it cannot
loosen frozen restrictions. Offline, revoked, stale selection or invalid proofs
fail before provider invocation. Provider IDs are restricted labels, never URLs.

Trusted adapters must resolve current application permissions for each request,
keep instructions separate from quoted JSON, disable tools and never execute a
model's returned instructions. Prompt separation reduces exposure; it is not a
claim that language models are immune to prompt injection. No existing general
model provider is silently connected to this interface. E supplies the user
approval/retrieval workflow and concrete configured adapter.

## Retention, revocation and legal holds

Only an authenticated space authority may POST a bounded reason and `hold`,
`release_hold` or `request_erasure` to `/v1/retention`. Decisions are serialized and
append-only. A request records intent, not approval of destruction. Releasing a
hold never authorizes physical erasure. There is deliberately no ordinary purge
endpoint, timer or cascading cleanup for accepted/pending federation state.
Database guards retain accepted records, receipts, inbox history, reviewed
snapshots and publication/source intents independently of mutable PDS history.

Tombstones/archival remove material from current context selection but preserve
accepted content and provenance. Revocation denies subsequent gateway operations
immediately at acceptance, even before the revocation reaches PDS. Previously
accepted queued writes may finish; operations already authorized in flight may
complete. Previously delivered offline copies remain locally readable while local
authorization permits. They cannot be recalled by revocation or deleting a PDS
record. New external-model egress needs live current gateway approval.

Infrastructure administrators and authorized recipients can read shared plaintext;
encryption in transit/at rest is not end-to-end secrecy from those operators.
Recipients can copy data outside this software's controls. Audit receipts provide
attribution and integrity evidence, not non-repudiable legal conclusions or proof
of a human review. Retention periods, residency, recipient agreements, data-subject
requests and legal-hold approval require owners' explicit legal/operational policy.
Whole-space decommission and physical erasure need a separately reviewed procedure
covering all authorized copies, pending work, backups, keys and legally required
evidence; do not disable immutable guards as a routine workaround. No claim of
native Spaces deletion, privacy or audit compatibility is made.
