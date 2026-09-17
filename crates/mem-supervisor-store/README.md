# Supervisor persistence

Trusted, in-process PostgreSQL storage for the contracts in `mem-supervisor`.
No HTTP, provider credentials, workers, schedulers, service authorization, human
approval UI, Pi execution or live migration is implemented by this crate.

## Authority and actions

`SupervisorStore::new(PgPool)` never migrates or enables anything. Migration 0053
adds only dedicated `supervisor_*` tables, indexes, constraints and guard
functions. It seeds four authorities with **no active policy**; it does not
modify existing source knowledge, provision logins, grant privileges or start
workers. Apply it only through the authorized instance migration workflow.

`install_policy(area, revision)` installs a disabled policy. Repeating the same
revision returns its existing state, including permanent revocation. Rotation
revokes the prior revision and cannot resurrect a previously used revision.
`set_enabled`, `revoke_policy`, and `reduce_limits` are trusted operator APIs.
Reductions cannot be reversed within a revision; a new revision never resets
the area's existing daily spending. The model is exactly `gpt-5.6-sol`.

`submit(ActionSubmission)` uniquely binds action UUID, area, source-event UUID,
kind, revision and SHA-256 evidence. An exact duplicate returns the existing
action; conflicting terms fail. The pure `ActionKey` is reused, with its stable
event/revision IDs constrained to non-nil UUIDs so they cannot carry prose.
The supported lifecycle is observed → proposed → authorized → queued → running
→ verified/failed, plus denial, cancellation, expiry and uncertain recovery.
`advance` checks common area policy but **does not establish action-specific
authorization**. The embedding service must separately validate requester,
collection, disclosure, standing-question, maintenance and human approvals.

`claim` exclusively claims a queued action with a database-clock lease of 1–300
seconds. Opaque `LeaseClaim` values are not deserializable. `renew` replaces the
fence and rejects unresolved provider attempts; callers must not renew across
a send. `check_current` is a point-in-time common-policy/lease preflight, not a
disclosure grant or send permit. Every `begin` and `publish` rechecks persisted
policy, exact claim and database time. A revoked policy cannot accept a new result.

`stop` and `reap_expired` fence running work and mark its outstanding reservations
uncertain. Such work is never automatically requeued. `reconcile` requires all
provider attempts to be settled/released; verified reconciliation still needs
current policy, while a trusted reconciler may record failure after revocation.
`publish` records only an immutable outcome and evidence hash: the caller must
durably preserve and validate the actual private result before calling it.
Use `publish_proposal` below for atomic typed private proposal persistence.
Generic verified publication/reconciliation rejects any action with provider-
attempt history unless its matching private proposal is already persisted in
the same transaction. Hash-only completion cannot bypass private persistence.

## Local instance and model-input binding

Additive migration 0055 adds `supervisor_instance_binding` and
`supervisor_model_contexts`. Both are immutable, initially empty, and contain
only UUIDs, bounded hashes and database timestamps. No policy is enabled, no
existing attempt is backfilled, and migrations 0053/0054 remain unchanged.

`bind_instance(instance_id, area)` is explicit trusted bootstrap: exactly one
non-nil instance UUID and one area may be bound in a database. Exact retries are
read-only; different identities or areas fail. `instance_binding()` returns the
stored identity, never infers it from hostnames or project names. This is **local
database uniqueness only**: a clone copies its binding, and another independent
database can bind the same area. Deployment still must provide one authoritative
ledger per area and fence duplicate instances/clones. This is not fleet identity
enrollment or distributed uniqueness enforcement.

Before any token-counting disclosure, call
`bind_model_work(claim, instance_id, project_id, request_digest, evidence_digests)`.
It fixes the exact instance, area, action, policy revision, existing project UUID,
wire-request SHA-256 and ordered list of at most eight evidence hashes. It checks
current policy and lease, locks the current project `FOR KEY SHARE`, and commits
the immutable context with a metadata-only `model_context_bound` journal event.
An exact repeat returns the same opaque `ModelWorkContext`; changing any input
fails. Contexts cannot be added retroactively to actions with provider attempts.
Health checks and installation actions cannot bind model work.

`check_model_work(claim, context)` revalidates the persisted context, local
binding, current project existence, current policy and exact lease using the
database clock. Call it before and after token counting and immediately before
generation; `check_current` alone is insufficient for model work. Valid lease
renewal preserves the input context but invalidates the old claim. `reserve`,
`begin` and first proposal publication independently require the same context;
publication cannot relabel a result's project or reorder/substitute evidence.
Journal contention cannot commit a context whose lease expired while waiting.

These checks establish scope, **not permission to disclose project data**.
The trusted service caller must independently authorize the exact project and
source subset, retain the relevant permission locks across its dispatch boundary,
and recheck current source eligibility. This crate has no service principal or
permission-lock API and performs no network request. Its point-in-time checks
cannot recall already dispatched traffic or prevent a subsequent permission
change. Real service configuration/authorization integration remains required.

The retained project UUID has no foreign key, preserving deletion behavior.
Deletion prevents further model preflight, reservation, begin or first result
publication; it does not erase evidence or prevent settlement/recovery. Historical
0054 proposals remain readable without a new context. Where a context exists,
private reads also verify its immutable project/request/evidence binding. No
legacy action gains new dispatch authority just because 0055 was applied.

## Private model proposals

Additive migration 0054 introduces `supervisor_proposals` and an immutable-body
guard; it seeds/enables nothing and does not change migration 0053. The retained
project UUID deliberately has no `projects` foreign key, preserving existing
project deletion behavior. A **new** publication checks that the project exists
under `FOR KEY SHARE` in the publication transaction. Historical retention never
confers current project access or source eligibility.

`publish_proposal(claim, &attempt_id, project_id, &Proposal, &evidence_digests)`
validates the shared typed shape: nonempty summary of at most 4 KiB, at most eight
evidence digests and bounded unique reference indices. Health checks and automatic
installation actions cannot publish model proposals. The selected attempt must
be settled and match the action, area, policy revision and exact lease fence.
Current policy and live lease are required for every new write, including a
recheck after any wait for the journal lock.

The private row, immutable verified outcome and metadata-only journal entry
commit in the **same transaction**. Any error rolls all three back. A domain-
separated SHA-256 binds the original immutable request digest, area, action,
attempt, policy, fence, project, complete typed proposal and ordered evidence
digests. The proposal is still only a suggestion, never an approval, source
mutation, installation or executable instruction.
Here `verified` means the required result was validated and durably recorded,
not that the model's factual claims or suggested actions were approved.

`private_proposal(area, project_id, action_id)` retrieves only an exact scope and
revalidates the private digest against the verified action/outcome and settled
attempt. It has no cross-project listing or fallback. `PrivateProposal` and the
neutral `Proposal` have no `Debug` implementation. The service must authorize
every read and revalidate current source eligibility itself; these methods are
trusted in-process APIs, not new service authorization. Bodies never appear in
action/attempt/journal metadata reads, and private storage errors discard all
SQL/JSON diagnostic context that could contain a failing private row.

Restart reads recover the complete typed body. An exact publication retry can
return the immutable historical row after lease expiry, revocation or project
deletion, but it never changes authority, emits a new event, creates a send or
authorizes access. Changed project, body, kind, references, evidence digests or
attempt bindings are rejected; corruption is not returned as a valid proposal.

## Physical provider attempt protocol

1. Establish the exact input, output/token caps and conservative micro-USD cost
   bound outside this crate. Bind the exact wire request SHA-256 and fixed model.
2. Bind the local identity and model context above, then use `check_model_work`
   before any external token counting. `reserve(claim, ProviderRequest,
   request_digest)` atomically checks matching context, current project, policy,
   lease, the area's current UTC day and remaining limits. A duplicate returns
   `Reservation::Existing`, never permission for another send.
3. `begin(claim, attempt_id)` commits the in-flight record before returning a
   non-cloneable/non-deserializable `SendPermit`. The sender consumes it once,
   checks its digest/model and bounds dispatch by its lease deadline. Duplicate
   begins fail. No permit is reissued after a crash or uncertain commit.
4. `settle(attempt_id, actual_cost, evidence_digest)` records authoritative usage
   even after expiry, revocation or day rollover. Exact repeats are idempotent;
   conflicting amounts or evidence are rejected. An over-reservation charge is
   retained in full and atomically revokes the area's **current** policy. This
   blocks further admission and begins, including under a rotated policy.
5. Transport ambiguity calls `mark_uncertain`. Uncertain attempts retain their
   entire reservation. `release_before_send` requires the original live claim
   and reserved state; it cannot release in-flight or uncertain money. A released
   ID also cannot be reused. Additional physical sends require new admitted IDs.

Admission reserves at most $2.50 and 20 calls per area per **PostgreSQL UTC day**,
not per revision. Four isolated area limits allocate at most $10/80 calls for
the fleet. No area borrowing, client-supplied clock or day reset is available.
This fleet bound requires exactly one authoritative ledger per area and a single
area binding across installations. This crate does not establish distributed
uniqueness across databases or clones; admitting the same area in two independent
databases would duplicate its budget. Deployment must fence replicas/restores
and bind every physical send to the selected authoritative area ledger.
Requests use integer micro-USD. Settlement uses exact `numeric(20,0)` to preserve
the pure contract's entire `u64` range; aggregates use `u128`. Charges above an
admitted bound are incident evidence, not proof that the provider was capped.
Actual network budget enforcement additionally requires every physical send to
use this protocol and the external adapter to enforce its proven pricing/token
bound, approved endpoint and absence of implicit retries. Already dispatched
traffic cannot be recalled by subsequent revocation.

## Atomicity, privacy and recovery

Common operations share-lock the area authority, then lock action and area/day
budget rows. Policy mutation/over-bound incidents exclusively lock that area's
authority. Unrelated areas have no shared lock. Budget and attempt transitions
are committed with their journal entries. Attempts' request terms, settlements,
published outcome evidence and journal rows are append-only; triggers reject
mutation/deletion and invalid accounting transitions. Action identity and policy
revocation/limit reductions also have database guards. Database owners remain
trusted; this is not a security boundary against an administrator bypassing SQL.

Every journal writer locks a per-area journal-head row **before allocating its
sequence and through commit**. `journal(area, after, limit)` (maximum 500) therefore
has a commit-safe area-specific cursor; callers must not reuse one area's cursor
for another area or treat the global identity as a global commit watermark.
Only fixed event labels, UUIDs, hashes and database timestamps are journaled.
There are no free-form errors, source content, prompts, provider bodies, secrets,
URLs or arbitrary JSON fields. Reads still require authorization in the service.

On uncertain database commit, read the durable action/attempt by its stable ID;
never assume an absent response means a send may be retried. Recreate the store
after restart, inspect retained attempts, reap expired actions and reconcile
from authoritative evidence. No API force-releases unknown usage.

## Validation

Tests use `mem-test-support::IsolatedDatabase` and only an explicitly configured
disposable pgvector database. Set `MEMORY_LAYER_TEST_REQUIRE_DB=1` to make missing
database coverage fail. The harness applies migrations to its configured test
database and creates/drops isolated synthetic fixture databases; never point it
at a live or restored source database.

```sh
CARGO_TARGET_DIR=/data/tmp/memory-four-area-target \
  cargo test -p mem-supervisor-store --locked
CARGO_TARGET_DIR=/data/tmp/memory-four-area-target \
  cargo clippy -p mem-supervisor-store --all-targets --locked -- -D warnings
```

Integration coverage includes exclusive concurrent claims, hard call/cost caps,
cross-area independence, stable duplicates and digest conflicts, renewal fences,
reductions/rotation/revocation, begin-before-send replay denial, expiry and restart
recovery, immutable full-width charges, over-bound automatic revocation, a
commit-order journal barrier, delayed-begin expiry and UTC day rollover. Private
proposal tests cover atomic publication and journal-failure rollback, restart
reads, exact scope, immutable bodies, revocation/expiry, altered retries, matching
attempt fences, bounded references, corruption and sanitized database errors.
Context tests additionally cover missing/default-deny binding, concurrent local
identity assignment, exact input retries and mismatches, renewal, current project
deletion, retained usage, rejected retroactive binding, corruption and atomic
journal rollback (including lease expiry after waiting).
