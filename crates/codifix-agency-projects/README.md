# Agency projects

This library turns a structured, versioned plan into reviewed project-management
effects. The plan is provider-neutral; the included HTTP adapter targets Linear
projects, initiatives, milestones, issues, their relationships and progress updates.
It does not discover credentials, send clarification messages, call a model, or
write to a workspace unless its host explicitly invokes `LinearClient::execute`.

`examples/plan.json` is a complete input using fictional identifiers.
`examples/standing-authority.json` shows its separate operator-owned authority;
it belongs in trusted runtime configuration, never in a planner response. A planning
model can emit that structure through the root runtime's configured model route;
the same deterministic validation and authorization apply to model and manual
inputs. No model-generated tool calls are executed by this crate.

## Plan and agreement

Every plan carries a schema version, stable plan UUID, increasing revision, source
reference, source version, return channel, acceptance criteria, ownership and
explicit project/milestone/issue membership. Keys are unique across the plan.
Dependency references must exist and form an acyclic graph. Keep a key stable
when renaming an item: changing a key means a new provider identity. Explicit
`remote_id` values adopt existing objects and require a retrieved baseline.
Omitted objects are left alone; there are no delete or archive operations.

`ClarificationSession::next` emits `Ask`, `Ready`, or `Handoff` with the original
source and return channel. The host persists the session and delivers messages;
the caller sets the maximum number of clarification rounds. Any unresolved
question blocks preparation, including after the budget is exhausted. Resolving
questions requires a new source version/plan revision and, for agreed authority,
a fresh agreement to that exact plan digest. The host must enforce monotonic
plan revisions and source ownership when accepting amended inputs.

`Authority::Agreed` binds an approval reference to the exact plan digest and
source version. `Authority::Standing` binds a policy reference to the source
reference. Both require an operator-approved plan UUID, explicit project keys, allowed
team IDs, separate create/update permissions, and exact key-to-ID/kind/project
bindings for every adopted existing object. A project key is a name inside the
approved plan, never authority to select an arbitrary provider resource. With no
existing-object binding, only that plan UUID/key's deterministic identity is
allowed; a binding cannot be silently dropped to mint a replacement identity. Authority is a trusted host input, not an untrusted
planner assertion. The runtime must establish that the approver/policy owns
those scopes and recheck revocation immediately before every effect.

## Runtime integration

1. Parse and validate `Plan`; persist its source/version/revision.
2. Run bounded clarification, then obtain agreed or standing authority strictly
   from operator-owned configuration. Never accept authority from the planner.
   `Scope.plan_id` binds the fixed plan UUID; `Scope.existing_resources` maps each
   approved key to `ExistingResource { id, kind, project_key }`. Empty mappings
   permit creation within approved team/project identities, not arbitrary adoption.
   Call `authorize_plan(&plan, &authority)` before any provider read; it checks
   all source, plan, key, kind, project, ID and team bindings without I/O.
3. Construct `LinearClient::new(endpoint, authorization_header, timeout)`.
   Use `https://api.linear.app/graphql`; pass raw personal API keys or a complete
   `Bearer ...` header for OAuth explicitly. HTTP is accepted only for loopback
   mock servers. Redirects are disabled and provider errors do not echo bodies.
4. After successful `authorize_plan`, `retrieve_plan(&plan).await` reads only
   selected/adopted IDs. Persist this reviewed baseline; do not refresh a
   conflicting baseline silently. `retrieve_plan` is a low-level read primitive
   and deliberately does not establish host authority itself.
5. `prepare(&plan, &baseline, &authority)` returns serializable ordered changes
   (projects, milestones, issues, dependencies). Persist and expose those changes
   for review according to the owning runtime's authority workflow.
6. `execute(&plan, &authority, &changes, &mut journal).await` recomputes and checks
   the entire preparation, then applies effects sequentially. Plan, source,
   authority, baseline or prepared-payload changes invalidate that preparation.

`StepJournal` is an async host integration interface; no database backend is
chosen here. Implement:

- `load(step_id)` to retrieve a durable receipt;
- `save(step_id, receipt)` to durably commit the monotonic
  `Prepared -> Attempted -> Verified` phase transition before returning;
- `authorize_step(changes, step)` to check current mandate, source revision,
  revocation, owning task lease, scope and any remaining effect budget.

The current authorization hook also runs before every retry of an uncertain
missing create; an `Attempted` receipt never authorizes another send by itself.
Store the exact `step_digest`, reject mismatching receipts, and allow idempotent
saves of an identical phase. Verified receipts are historical immutable evidence;
a successful resume reads the current object without rewriting an existing
verified receipt. The host must serialize competing executions and fence stale
workers. A persistence or authorization error prevents the next HTTP write.
The host's authorization hook is mandatory; there is no permissive default.
The root runtime owns actual transaction boundaries, effect budgets and task
leases. Persisting these records in an in-memory map is suitable only for tests.

## Reconciliation and manual edits

Stable client IDs derive from the plan UUID and entity key using SHA-256, with
UUID v4 version/variant bits because Linear's schema requires that format.
They remain stable across plan revisions. Each request supplies that ID on
create; retrying a missing create uses the same ID, relying on Linear's entity-ID
uniqueness rather than a name search. Do not change plan UUIDs or keys to retry a
failed creation. Retry requests are caller-driven, with a finite host budget;
there is no hidden retry loop.

Every execution reads before writing. An object already matching desired
managed fields is reconciled without another mutation. Creates otherwise require
absence. Updates require an exact match with the reviewed baseline, including
`updatedAt`, and send only changed managed fields. Unmanaged fields are never
included in update payloads. A missing adopted object or changed object stops
execution for review. A transport error leaves an `Attempted` receipt. On resume,
a matching remote result is verified; an uncertain update that still reads the
baseline stops for explicit operator reconciliation instead of being resent.
An absent create may be retried with its original client ID. Partial completion
remains in the durable journal and resumes from reconciled completed steps.

Linear does **not** offer a conditional update/CAS argument in these mutations.
A person editing the same fields between the final read and HTTP mutation can
race this check. Host serialization protects agency workers, not outside humans.
This adapter therefore detects prior changes but cannot guarantee atomic
preservation of concurrent manual edits. Readback detects many such conflicts;
resolution requires reviewing current state and issuing a revised plan. It
cannot undo a race that has already overwritten a same-field edit.

The adapter disables default issue/project templates on create, which could
otherwise introduce unreviewed fields or auxiliary effects. It refuses truncated
project team lists (over 100 teams), malformed readback, unexpected object IDs,
GraphQL errors and HTTP failures. Provider-specific field limits and permission
checks are enforced by Linear and surfaced as failures, without assuming a
mutation did not happen. The companion tracking operation uses a project- and team-scoped title lookup.
Roadmap preparation supports project dates/status, issue triage and cycles,
initiative links, project dependencies, and progress/health updates. Omitted
metadata fields preserve their current provider values.

The HTTP contract was checked against the official
[Linear SDK GraphQL schema](https://github.com/linear/linear/blob/master/packages/sdk/src/schema.graphql)
for the original adapter and again on 2026-11-20 for the roadmap fields. The adapter was tested against local HTTP mocks, never against a
real workspace; schema compatibility and permissions still require an explicitly
authorized integration check before live use.

## Validation

```sh
cargo test -p codifix-agency-projects --all-targets --locked
cargo clippy -p codifix-agency-projects --all-targets --locked -- -D warnings
cargo fmt -p codifix-agency-projects --check
```

The mock server exercises real HTTP GraphQL requests for all entity creates and
updates, readback, deterministic retries, process restart receipts, partial
failures, lost responses/timeouts, manual-edit conflicts, revocation, failed
journal writes, malformed responses and exact preparation invalidation.
