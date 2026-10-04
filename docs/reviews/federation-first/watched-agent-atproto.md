# Watched-agent ATProto expertise exchange

Status: approved implementation contract; not an active transport or permission grant.

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

- [ ] Add bounded neutral addressed-record contracts and unpublished ATProto
  question/answer Lexicons without changing existing publication bytes.
- [ ] Add durable addressed PDS outbox, verified publication/read-back, recipient
  filtering, retention and retry/restart recovery to the gateway.
- [ ] Add per-project, exact-token agent ask policies with collection allowlists,
  fixed output destinations, deterministic-only mode and a 20-question UTC-day
  limit. Recheck current authority before every release.
- [ ] Add an agent-submit API and MCP tool with standing-policy audit semantics;
  preserve the human exact-review API.
- [ ] Bundle the focused skill with the release and install missing copies from
  the local package in watched repositories without overwriting custom skills.
- [ ] Show metadata-only exchange status in Web, CLI and TUI.
- [ ] Rehearse four disposable areas, including Workshop and Trust, with both
  Codex and Claude bindings; test isolation, replay, outage, signed read-back,
  citation integrity, expiry and watcher bootstrap.
- [ ] Build and upgrade Monolith, package one CA handoff through the existing
  coordination channel, and demonstrate cited answers from each agent type.

Pi supervisors, delegated tasks, paid synthesis and automatic remote requests
from ordinary local queries remain out of scope.
