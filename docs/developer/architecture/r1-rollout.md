# R1 ordering cutover and enrichment repair

R1 is an additive engineering change, not authorization to deploy it. Use an
operator-controlled maintenance window and an explicitly selected environment.
The verification used disposable PostgreSQL and fake providers only.

## Coordinated cutover

1. Identify every database writer: services, watchers, embedded services,
   direct-library/maintenance callers, federation source processing and workers.
   Record their versions, owners and restart configurations; close new intake.
2. Finish in-flight work where feasible and drain existing offline stores before
   stopping their writers. Preserve undrained stores with their profiles and
   attribution limitations. Do not delete uncertain items.
3. Stop **all** writers and verify their database sessions/transactions ended.
   A maintenance page or stopped HTTP process alone is insufficient. Save an
   encrypted backup of the database, evidence, offline stores, configuration,
   identity/credential material and local journals. Verify restore readability
   in isolation before authorizing the migration.
4. Apply this release's normal additive SQLx migration path once, while other
   writers remain stopped. Migrations 0039–0047 are new; never edit older applied
   migrations/checksums. 0039 exclusively locks the timeline, preserves old
   sequence values and initializes its transactional counter at their maximum.
   If migration fails, diagnose under maintenance before restarting writers.
5. Verify migration checksums, counter, retention watermarks and job/ownership
   tables. Deploy compatible binaries and a common concurrency configuration
   to every writer. Start services, verify database health and worker polls,
   then submit a small synthetic capture and inspect its committed event and
   four stages. Only then re-enable watchers, clients and federation processing.
6. Observe ready age, blocked/failed stages, uncertain attempts/effects and offline
   catch-up. Curation success means mutations/events/intents committed, not
   completed embedding or synthesis. Recover timelines through bounded cursors;
   a retention gap requires snapshot recovery, not silent cursor advancement.

Mixed old/new writers are unsupported during this cutover. Rolling back after
new writes requires another writer stop and an explicitly chosen compatible
roll-forward or restore strategy. Restoring an older backup can discard accepted
work; it is not a harmless binary downgrade. This document does not authorize
that action or assert that customer backup recovery has been demonstrated.

## Runtime settings

```toml
[background_work]
enabled = true
internal_automation = true
concurrent_projects = 2
poll_seconds = 2
lease_seconds = 120
renewal_seconds = 30
max_attempts = 5
initial_delay_seconds = 10
max_delay_seconds = 600
```

The project cap is shared through PostgreSQL admission; each project has at most
one live stage across processes. Use the same limit on all writers; lowering it
does not cancel running work. Validated bounds: concurrency 1–16, polling 1–60
seconds, leases 4–3600 seconds, attempts 1–20, maximum delay at most one day.
Renewal must be positive and less than half a lease. Defaults above are the
starting point. Shutdown/reload stops owned tasks; unfinished leases recover.

Foreground loop/validation ownership requires direct or session-pooled
PostgreSQL, **not transaction pooling**. Budget one extra session per active
foreground execution. Provider calls do not hold result transactions open.
Test-runner parallelism is bounded to eight because tests create independent
databases/owner sessions; explicit concurrency tests still compete normally.

## Explicit pre-cutover enrichment repair

Migrations never invent historic events, curation runs or jobs. For vectors only,
preview `memory embeddings reembed --project PROJECT --dry-run`, review its scope
and cost, then explicitly run without dry-run. Do not recapture old content to
pretend its enrichment was scheduled historically.

For the complete recipe, the bounded maintenance example requests **new** work.
This is a privileged database-administrator tool, not a public authorization API.
Use an approved protected connection and an explicitly chosen active principal
with the required project execution permissions. Workers still check current
principal authority before work and every result.

1. Inspect the exact project and 1–200 current active non-tombstone version UUIDs.
   Save that selection and choose one fresh operation UUID. Never modify existing
   job inputs/statuses directly to force a retry.
2. Supply `MEMORY_LAYER_REPAIR_DATABASE_URL` through the approved secret mechanism
   without exposing it in shell history, then explicitly run:

```sh
cargo run -p mem-store --example repair-post-curation --locked -- \
  --apply PROJECT_UUID NEW_OPERATION_UUID PRINCIPAL_UUID VERSION_UUID...
```

3. The transaction creates a present-time repair-request event and four stable,
   version-pinned stages. It does not change memory text, original authorship,
   source timestamps or historical curation rows. Event failure rolls back all
   intents. Repeating identical inputs returns the same stages/event; a changed
   actor or selection conflicts. After an uncertain commit, reuse the operation
   UUID rather than create another billable request.
4. Inspect the returned IDs in Web/CLI/TUI Jobs. Disabled stages skip; current
   authority, source versions, quota, utility-floor and provider policy still
   apply. Address failures before explicit retry. This does not bypass revoked
   authority or stale sources. Inspect derived proposals before approval.

Repair can incur provider costs; exactly-once billing is not promised. Provider
caches stay private operational data, not timeline or export material.

## Interrupted external work

See [external-effect journals](external-effects.md). Match an exact operation ID,
current file/provider state and receipt before choosing a new action. Never
blindly rerun commands or restore configuration from an old intent. Trigger
intake without a linked run is not completed execution: inspect traces and
explicitly request missing work under current policy. Old actors remain unknown.

This runbook does not demonstrate customer DR, deployment readiness, retrieval
or evaluation quality, or eligibility to start F.
