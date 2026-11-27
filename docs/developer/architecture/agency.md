# Autonomous specialist runtime

Codifix agency is a bounded execution layer with explicit identities and grants.
It runs as a companion service; the existing supervisor's stateless model path,
private expertise exchange and ordinary background workers retain their contracts.
The implementation does not infer permission from a public advertisement, model
output, task text, reputation or a shared procedure.

## Ownership

| Component | Responsibility |
| --- | --- |
| `codifix-agency` | Pure manifests, mandate attenuation, task states, A2A data contracts, public projection, measured routing, improvement gates and matched outcome accounting |
| `codifix-agency-runtime` | Authenticated HTTP/CLI, persistent admission, scheduling, private conversations, provider execution, operator controls and evidence |
| `codifix-agency-projects` | Provider-neutral plans and reviewed changes; Linear CRUD subset, readback and conflict detection |
| `codifix-agency-openbao` | Independent deterministic controller, custodians, native attestor and protected local replay state |
| Existing store/record | Specialist job kind, leases, physical effects and execution ownership |

Migrations 0087 onward are additive. A specialist job has a separate claim filter;
ordinary service workers cannot accidentally execute it. The service starts four
bounded workers plus independent schedule/cancellation maintenance. Existing
per-project job serialization remains in force. Give unrelated work separate
project scopes where concurrency is appropriate.

## Identity and conversations

Public profiles are an explicit allowlist projection of separately reviewed
public text, using `dev.codifix.agency.v1.capability`. Publication requires a
separate public PDS DID, refuses configured private repository DIDs, checks the
session DID and reads the published record back. It never republishes a private
federation repository. Remote discovery is untrusted information; it is not a
full CAR/commit identity attestation and grants nothing.

Private task transport implements the non-streaming A2A 1.0 HTTP+JSON subset:
SendMessage, GetTask, ListTasks and CancelTask. Agent Cards declare bearer auth,
JSON content, no streaming and no push notifications. Errors use problem JSON.
The task data part contains Codifix's typed local contract; arbitrary text
is not interpreted as authority. The server assigns task/context IDs, deduplicates
message IDs against exact payloads, checks ownership on every private route and
uses signed caller/filter-bound list cursors. Default sends wait up to 60 seconds;
a timeout includes task/context IDs for polling. History is omitted under the
server's zero-history limit. This is not a claim of certification of every A2A
binding, streaming, file or push capability.

Peer enrollment is operator-owned configuration: instance identity, distinct
bearer credential, exact permitted mandates, optional HTTPS endpoint and separate
outbound credential. A2A is terminated behind an operator-controlled TLS proxy.
Public AT Protocol discovery need not be available for enrolled peers to work.

Codifix-to-Codifix delegation additionally carries an authenticated delegation
trace, destination restrictions and a renewable execution lease of at most 60
seconds. This is a Codifix extension, not a generic A2A guarantee. The remote
operator's own grant must also authorize the operation. Task budget/deadline/depth
are attenuated; source destination restrictions are intersected and survive
clarification and further delegation. Cycles are rejected. A dead coordinator
stops renewing; a remote worker cannot admit a new effect after lease expiry.
An already admitted network effect may finish. Cancellation is journaled and
retried independently of worker progress; absent remote IDs require explicit
reconciliation against the original message ID.

## Durable execution and cost

Admission uses a transaction-wide lock around current task/mandate/ancestor
checks, remaining budgets and physical effect intent. Child mandates and tasks
reserve portions of parent budgets. Cancellation, expiry, revocation and expired
execution leases prevent subsequent admission. Every task activation is charged
once. Clarification spends a turn. A worker restart cannot elect a second sender
for an uncertain model request or private message.

Project writes use persisted preparations, plan heads, a per-plan execution lock,
step receipts and unique physical attempt records. Every provider read and write
is metered and checks current authorization. A completed step is reconciled from
remote state; a lost update response is not blindly resent. An absent create can
retry only its original stable ID. Linear has no conditional update argument, so
a human changing the same fields in the narrow interval between the final read
and mutation remains an external race; readback/conflict review is required.

Routing chooses the cheapest eligible measured profile, using current locally
accepted evaluations for the exact model revision, task quality/latency/context
requirements and both mandate and task disclosure policy. Deterministic work
uses no model. Configured rates bound token spend; the runtime conservatively
retains the upper-bound debit and records actual token usage, estimated cost and
latency. Local rates must include compute rather than treating local inference
as free. Unknown provider outcomes retain their reservation. Profiles and prices
are operator-reviewed inputs, not measurements invented by the router.

## Improvement and evidence

Candidates bind immutable artifact and baseline revisions/digests. Supported
runtime artifacts modify drafting prompts, drafting procedures or the choice
among already configured/evaluated models. They cannot add credentials, tools,
grants, budgets, code or new model destinations. Retrieval training, weights and
self-deployment are outside this release.

The lifecycle is propose, held-out evaluation, locally accepted 10% canary,
independently assessed rollout, promotion or revert. A deterministic bucket uses
server-assigned task IDs; task deployments are recorded. Merely producing JSON
is operational evidence, not semantic quality. Promotion requires distinct
persisted task/evidence assessments by an operator or trusted evaluation service,
with no observed failures. Reverting affects future admissions and cannot undo
an already completed external operation. Shared artifacts still need local
acceptance.

`codifix-agency compare` validates matched deterministic/single-agent/network
receipts, including dataset, case, verifier, model and procedure revisions. It
reports success, cost, latency and human interventions. Synthetic fixtures stay
labeled synthetic. Neither integration tests nor arithmetic over supplied
receipts establish that a specialist network is scientifically superior.

## Validation and deployment boundary

The suite includes real pgvector-backed persistence and real HTTP to local
Linear, A2A and model fixtures; a separate opt-in test uses an actual OpenBao
binary and independent custodian processes with synthetic shares. These tests
exercise restart, revocation, uncertainty, budgeting, wrong identities and
manual-edit conflicts. No production grant, public profile, real Linear ticket,
CA enrollment or live trading action is installed by building this branch.

Production rollout requires the actual CA seal/threshold and native enrollment,
independent custody failure domains, offline trust/bootstrap paths, a selected
Linear test project and reviewed credentials/authority, TLS/backup/monitoring
configuration, and live acceptance checks. Jira needs its own provider hierarchy
mapping and remains a later adapter.

Protocol references: [A2A 1.0](https://a2a-protocol.org/v1.0.0/specification/),
[AT repositories](https://atproto.com/specs/repository),
[OpenBao seal](https://openbao.org/docs/concepts/seal/), and the provider-specific
[Linear adapter notes](../../../crates/codifix-agency-projects/README.md).
