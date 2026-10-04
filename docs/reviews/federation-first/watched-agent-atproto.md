# Watched-agent ATProto expertise exchange

Status: `2.1.0-rc.4` software implemented on `codex/federation-v1`; fleet
activation, operator grants and deployment remain pending. This document grants no
question or onward-disclosure permission by itself.

The watcher monitors Codex and Claude repositories. A bundled skill will let those
agents discover reviewed capabilities and ask task-relevant questions when local
evidence is insufficient. Submission uses a standing operator policy, not an
agent's claim that Olivier approved a particular question. Codex/OpenAI and
Claude/Anthropic receive separate, exact-token output-destination bindings.

Capability advertisements continue in the existing schema-v3 publication stream.
Agent questions and answers will use separate, addressed ATProto records in the
private gateway-controlled PDS. The existing human mailbox workflow and v2/v3
publication bytes remain compatible. The gateway may keep durable metadata,
leases and outbox intents, but a response is deliverable only after exact PDS
read-back and signed-repository verification. Addressed records must not enter
the space-wide publication pull, retrieval corpus or general exports. An outage
leaves work queued; there is no plaintext-mailbox downgrade for agent questions.

The requester and responder alone may obtain addressed bodies through the
gateway. This is gateway-enforced access, **not end-to-end encryption**: gateway
and PDS administrators can read plaintext, and revocation cannot recall bytes
already delivered. No public repository access or native Spaces alpha is used.

## Execution checklist

- [x] Add bounded neutral addressed-record contracts and unpublished ATProto
  question/answer Lexicons without changing existing publication bytes.
- [x] Add durable addressed PDS outbox, verified publication/read-back, recipient
  filtering, retention and retry/restart recovery to the gateway.
- [x] Add per-project, exact-token agent ask policies with collection allowlists,
  fixed output destinations, deterministic-only mode and a 20-question UTC-day
  limit. Recheck current authority before every release.
- [x] Add an agent-submit API and MCP tool with standing-policy audit semantics;
  preserve the human exact-review API.
- [x] Bundle the focused skill with the release and install missing copies from
  the local package in watched repositories without overwriting custom skills.
- [x] Show metadata-only exchange status in Web, CLI and TUI.
- [ ] Rehearse four disposable areas, including Workshop and Trust, with both
  Codex and Claude bindings; test isolation, replay, outage, signed read-back,
  citation integrity, expiry and watcher bootstrap.
- [ ] Build and upgrade Monolith, package one CA handoff through the existing
  coordination channel, and demonstrate cited answers from each agent type.

Pi supervisors, delegated tasks, paid synthesis and automatic remote requests
from ordinary local queries remain out of scope.

## Implementation and evidence to date

Migrations 0062–0065 add separate gateway addressed intents, local agent
intents/budgets, fenced PDS deletion and the area-import compatibility ceiling.
The feature is opt-in through an exact schema-v3/schema-v1 fleet inventory; old
publication bytes and the human mailbox remain unchanged. The gateway makes
private bodies readable for at most the stated access lifetime, then retains them only
for a fenced delete retry. A missing PDS read-back is not a deletion receipt:
the exact key must be absent from a newer, pinned-key signed CAR snapshot.
An active space hold pauses physical deletion; failed deletion retains the
private body for operator repair, without making it readable to participants
after the 24-hour payload deadline. This is not deletion from backups or
recipient copies.

Focused tests pass for signed inclusion/absence, compare-and-swap delete,
hold behavior, PostgreSQL leases, exact retries, four enrolled area identities,
Codex/Claude destination labels and recipient isolation. The four-area gateway
test uses synthetic store publication receipts; the separate fake-PDS HTTP
test checks signed CAR verification. Neither is yet the full four-service/PDS
rehearsal or live deployment. The complete Rust workspace passed against an
isolated PostgreSQL 17 database with four test threads and a 300-connection
ceiling; the default 100-connection disposable test cluster produced pool
timeouts when the entire workspace was exercised concurrently. Lean build/test,
Web tests/build, scoped JavaScript proxy tests, formatting, workspace Clippy
(with the pre-existing `double_must_use` macro warning exempted) and boundary
checks pass. Release artifacts and deployment evidence are recorded separately
before the remaining boxes can be checked.
