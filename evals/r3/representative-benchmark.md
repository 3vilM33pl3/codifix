# Representative R3 benchmark — draft replacement design

Olivier approved this direction: keep the existing small cases as smoke tests;
make the official cases representative of engineering continuity. This is not
approval of labels, spending or official execution. Original dataset sizes and
quality gates remain unchanged. Do not pad counts by renaming the same problem.

## What changes

Knowledge questions should require joining evidence from an incident timeline,
current runbook, historical decision and configuration excerpt. Include useful
distractors and an explicitly superseded instruction. No document should be a
question-shaped answer key. All informed conditions get the same source facts;
the maintained handover must be a genuinely useful curated alternative.

Coding tasks should operate on small but coherent multi-file Rust applications,
not isolated functions. Each repository has public tests, a realistic bug and
documented operational constraints. Three steps share the working tree: diagnose
and repair, accommodate a changed requirement, then demonstrate regression safety.
Private graders run separately and cannot be read by the coding agent.

## First knowledge workflow: supplier incident handover

Fictional organizations Northbank Operations and Copper Relay exchange approved
incident knowledge. A payment acknowledgement timeout is followed by a retry and
a duplicated settlement. This is a synthetic reasoning task, not a live payment
system or a new federation implementation.

Source pack:

- Incident timeline: first settlement committed at 10:03:12; acknowledgement
  timed out at 10:03:14; retry used a different request ID at 10:03:17.
- Current runbook: deduplicate by settlement identity, not transient request ID;
  quarantine ambiguous outcomes for reconciliation rather than blind replay.
- Superseded runbook: retry every timeout with a fresh request ID.
- Configuration: deduplication retention 72 hours; supplier replay horizon five
  days. These values are intentionally inconsistent, not independent trivia.
- Reviewed correction: the origin owner approved the new deduplication rule;
  the recipient may propose changes but cannot overwrite that publication.

Four task types: explain the failure using timeline and policy; identify why the
retention settings still permit duplication; propose a bounded recovery plan
with ownership/verification; refuse to invent the financial loss or exact affected
customer count. Rubrics require correct evidence references, recognition of the
obsolete instruction and explicit separation of observed facts from inference.

## Second knowledge workflow: recovery after staff turnover

An on-call engineer inherits a service after its maintainer leaves. A backup
restore succeeds but synchronization stalls. Evidence is spread over a restore
log, identity-rotation decision, deployment configuration and an old runbook.
The restored data belongs to writer generation 8; the gateway has fenced it and
expects generation 9. A stale instruction recommends reusing the old credential.

Tasks: distinguish data recovery from writer authorization; identify what must
be verified before synchronization resumes; explain which historical knowledge
remains usable offline; decline to claim a recovery-time objective from a single
unmeasured exercise. Do not reveal credentials or require real infrastructure.

## First coding workflow: durable event importer

Repository modules: `input.rs` parses deliveries; `store.rs` implements a
deterministic test store; `replay.rs` applies batches; `cursor.rs` records accepted
positions. Public integration tests exercise ordinary delivery. A fault injector
simulates failure between applying a record and advancing a cursor.

1. Reproduce and fix the partial-commit bug across store/replay/cursor boundaries.
   Graders verify atomicity and unchanged state after injected failure.
2. Support reordered duplicate delivery using stable operation IDs; preserve
   accepted sequence order rather than sorting by transport record key.
3. Add restart/catch-up behavior without replaying accepted effects. Graders
   inject failure before commit and after commit-before-ack and check convergence.

The source pack records the authority-ordering contract and an obsolete proposal
to use arrival order. Tests assert outcomes, not particular source-code strings.
Each step has both positive and negative controls; a stub that simply ignores all
input must fail. Reference implementations must pass all three stages.

## Second coding workflow: evidence publication review

Repository modules: `capture.rs`, `review.rs`, `publish.rs`, and `policy.rs`.
Inputs are fixture bytes, a review decision and a deterministic fake transport.

1. Fix a time-of-check/time-of-use bug: publication currently rereads a mutable
   file instead of sending the immutable reviewed snapshot.
2. Add version-scoped correction proposals, preserving original authorship and
   authenticated publisher identity as distinct values across modules.
3. Recheck revocation before publication and handle a timeout without producing
   a second logical publication. Earlier accepted versions remain available.

Hidden tests change the file after review, revoke authority between stages,
repeat a request and present a stale proposal. A blanket refusal must fail
authorized positive cases. No signatures, real PDS or paid provider is required.

## Expansion and validation contract

The dependency-build family is assigned to development: transitive build order,
dependency-aware cache invalidation and atomic cache replacement. Do not reuse
this failure family in the held-out split. Compilation is represented by a
deterministic artifact-generation counter, not an actual incremental compiler.

The configuration-rollout family is assigned to development before its fixture
is evaluated: overlay precedence, explicit deletion and revision-safe atomic
reload. It must not also appear as a renamed held-out family. Its runtime and
failure injection are deterministic test doubles, not a real deployment.

These four examples define the difficulty and evidence structure, not the final
held-out allocation. Expand to 10 development/25 held-out knowledge scenarios and
4 development/12 held-out repository sequences using genuinely distinct failure
families. Assign families to splits before implementation; no near-duplicate
repositories or shared hidden fixtures across splits. Keep counts at scenario/
whole-sequence level, not at assertion or step level.

For each case, implement sources first, then questions and private rubrics. Check
that a competent reviewer can answer from the sources and that simple baselines
are not artificially restricted. Measure outcomes, citation accuracy, refusal,
cost and latency; do not reward Memory merely for using its own terminology.

Before asking Olivier to freeze labels: materialize complete fixtures, run
reference and deliberately broken implementations, verify hidden-test isolation,
and provide a short plain-language review sheet with expected outcomes. No need
for Olivier to inspect raw JSON. Keep official execution disabled until the full
isolation driver and per-request budget mediator are independently verified.
