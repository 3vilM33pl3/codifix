# Transactional local persistence

`codifix-store` owns local event/job SQL primitives, not authorization, providers,
configuration, HTTP, or federation. `codifix-record` holds safe job wire types.

Timeline callers own the transaction: mutate, enqueue required work, append
events last, commit, then broadcast. An append error must roll back the whole
transaction. Migration 0039 allocates positions with a commit-held database row
lock, including for direct SQL inserts. Returned positions are provisional
until commit. Existing events retain their positions. The cursor reader uses a
consistent snapshot and deletion watermarks; it never assumes global sequence
gaps imply missing project records. It is a storage primitive, not an authorized
HTTP endpoint.

Jobs use a project slot -> job -> attempts/receipts -> timeline lock order.
Provider calls run without database transactions. To apply output, acquire
`begin_result`, write exclusively through its borrowed transaction, then call
`finish_result`; stale/expired attempts roll back their outputs. Dropping the
guard also rolls back. Keep result transactions short. Callers must reauthorize
and revalidate input before output writes. A lease does not grant permission.

`enqueue` uses an immutable project work key and compares duplicate inputs and
dependencies. Dependencies point to already-created jobs in the same project;
the API never modifies existing edges. A retry keeps immutable inputs and
completed stages, increments a generation, and journals the request/actor.
Only failed/blocked jobs accept manual retry. The API assumes the caller already
checked current permissions; it is not directly safe to expose to clients.

Successful outputs and acknowledgement commit together. Lease loss is recorded
as uncertain: a provider could have accepted a request even if no output was
committed. This is at-least-once scheduling, not exactly-once external billing.

## Cutover

Stop **all** writers, take/verify a backup, apply additive migrations, then
restart the new version. Mixed old/new writers are not a supported rollout.
Do not edit 0039/0040 after application. These migrations cannot reconstruct
past missing events or lost in-memory jobs. No automatic historical backfill
or provider work is triggered by the primitives alone.

Implementation coverage is tracked in
`docs/reviews/federation-first/r1-coverage.md`. The worker, caller conversions,
operator UI, and full R1 acceptance are separate work; these primitives alone
do not close R1.
