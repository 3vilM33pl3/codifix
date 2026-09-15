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

The remaining development knowledge families are allocated before materialization:

| Family | Distinct engineering decision |
| --- | --- |
| schema-transition | Expand/backfill/contract compatibility and rollback limits |
| capacity-pressure | Pool saturation versus database capacity and admission control |
| timezone-window | Local business windows across daylight-saving transitions |
| artifact-origin | Build identity, digest verification and unsigned release notes |
| retention-hold | Data expiry versus active legal holds and retained backups |
| quota-accounting | Reserved, settled and uncertain provider usage |
| dns-cutover | Split-horizon records, resolver caches and observed propagation |

Together with the three existing development scenarios these fill ten families.
Do not reuse these mechanisms as renamed held-out scenarios. This allocation is
not label approval; sources precede private questions and scoring rubrics.

### Held-out allocation (before implementation, labels still draft)

The held-out knowledge mechanisms are assigned independently of development:

| Family | Primary evidence-joining problem |
| --- | --- |
| telemetry-units | Gauge units, conversion location and mislabeled dashboards |
| cursor-snapshot | Tied sort values and stable paginated read snapshots |
| cancellation-scope | Caller cancellation versus already-started remote effects |
| archive-expansion | Small compressed inputs and bounded expanded output |
| circuit-probes | Dependency circuit state and controlled half-open probes |
| alert-denominator | Sampled observations versus population alert rates |
| cohort-assignment | Stable experiment buckets and changing assignment inputs |
| unicode-identity | Display-equivalent names versus normalized identity rules |
| quantization-error | Fixed-point precision and accumulating rounding error |
| watcher-rename | Atomic editor replacement versus watching an old file inode |
| vector-compatibility | Embedding dimensions versus semantic model compatibility |
| certificate-chain | Missing intermediates versus expired leaf certificates |
| token-audience | Valid tokens intended for a different service audience |
| stream-framing | Transport chunks versus complete application messages |
| traffic-drain | Endpoint removal versus active long-lived stream completion |
| cgroup-pressure | Process heap measurements versus container memory charges |
| reproducible-order | Random seeds versus nondeterministic parallel reduction |
| lock-inversion | Conflicting acquisition order across otherwise valid operations |
| numeric-overflow | Individually valid lengths and overflowing aggregate bounds |
| rename-durability | Atomic namespace replacement versus crash durability |
| null-filter | SQL null semantics and unexpectedly omitted anti-join results |
| token-boundary | Search field/token boundaries versus substring expectations |
| trace-link | Trace propagation versus sampled log absence |
| color-profile | Pixel values versus display color-space interpretation |
| distribution-notice | Recorded dependency origin versus distribution notice obligations |

Twelve held-out coding repository mechanisms are also allocated: `framed-stream`,
`interval-calendar`, `path-policy`, `decimal-register`, `identifier-catalog`,
`snapshot-pages`, `notice-bundle`, `sensor-matrix`, `circuit-window`,
`route-precedence`, `bounded-decoder` and `structured-redaction`.
These cover stream framing, half-open interval operations, component-based path
authorization, exact arithmetic, explicit normalized keys, stable page cursors,
notice composition, sparse sensor transforms, clocked dependency protection,
route specificity, bounded decompression and schema-aware disclosure respectively.
They must not reuse the development reference implementations or hidden probes.
Related knowledge/coding mechanisms stay wholly in held-out, never development.

This is an allocation, not a claim that any held-out fixture exists or is ready.
Materialize source contracts before tests/questions. Keep all held-out material
out of development calibration packs and do not inspect model outcomes to tune
its sources, labels or thresholds. All labels require Olivier's later approval.

The cache-partition knowledge family is assigned to development before fixture
implementation: distinguishing request authorization and signed provenance from
the scope of a reused answer, with bounded incident evidence. Do not reuse that
failure family in held-out material.

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
