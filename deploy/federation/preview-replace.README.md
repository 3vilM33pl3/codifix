# Exact private preview replacement

`preview-replace.mjs` operates only on its four named existing preview targets.
It does not pull images, create new areas, change credentials, migrate databases
itself, expose ports, or verify application health. The caller owns the fresh
backup, writer fence, release verification, compatible migration plan, and
post-start health/federation checks.

The private contract binds the exact old container ID, backup receipt, reviewed
artifact hashes, preserved mounts, and recovery directory. `review` binds its
configuration and network snapshot; `replace` requires that review digest.

`retained_name` accepts exactly either `<name>-before-dependable` (historical
behavior) or `<name>-before-rc2`. No arbitrary suffixes or targets are accepted.
The rc.2 namespace preserves earlier retained containers and uses
`<name>-unconfirmed-rc2` for an interrupted pre-start candidate. A changed
retained name needs a new review, not reuse of an old approval digest.

For rc.2, a durable `start-intent.json` is written before Docker can start the
candidate. From then on, `restore-old` refuses because startup can apply database
migrations or publish accepted work. Recover forward using the exact reviewed
candidate and the journal, even when the start acknowledgement was lost. Before
that point, explicit `restore-old` can restore the retained container. No phase
automatically rolls back, deletes a container, or claims to reverse remote work.

Validation: `node --test deploy/federation/preview-replace.test.mjs` uses synthetic
containers and includes exact identity boundaries, interrupted operations,
retained-name isolation, and the rc.2 no-downgrade boundary.
