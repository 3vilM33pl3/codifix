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
policy, exact claim and database time. A revoked policy cannot accept a result.

`stop` and `reap_expired` fence running work and mark its outstanding reservations
uncertain. Such work is never automatically requeued. `reconcile` requires all
provider attempts to be settled/released; verified reconciliation still needs
current policy, while a trusted reconciler may record failure after revocation.
`publish` records only an immutable outcome and evidence hash: the caller must
durably preserve and validate the actual private result before calling it.
This crate does not store proposal bodies or establish their atomic durability.

## Physical provider attempt protocol

1. Establish the exact input, output/token caps and conservative micro-USD cost
   bound outside this crate. Bind the exact wire request SHA-256 and fixed model.
2. `reserve(claim, ProviderRequest, request_digest)` atomically checks policy,
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
commit-order journal barrier, delayed-begin expiry and UTC day rollover.
