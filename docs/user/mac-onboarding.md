# Mac project onboarding

This feature is under implementation. Existing macOS packages still use the
setup wizard; they do not yet provide sign-in/select/clone/offline-sync onboarding.
See the [execution contract](../reviews/mac-onboarding/README.md) for completion
checks. No Mac installation or existing area has been changed by this work.

## Identity and project access

The company console uses Authentik for human accounts. Memory project roles,
Git repository access, federation membership and model-disclosure consent are
separate permissions. A project assignment is not a Git credential or a grant to
send project contents to an assistant.

Workshop, Homelab and General are the eligible offline areas. Trust remains
CA-local: its selected expertise may be queried, but its database and sensitive
sources are not copied onto a laptop. Revocation stops new transfers; it cannot
recall bytes already delivered to an offline device.

## Coding and advisory models

This repository's `.codex/config.toml` selects GPT-6.1 Sol with high reasoning for
the main coding session, Sol/medium for ordinary subagents, and at most two active
subagents. `.codex/agents/astra_adviser.toml` selects GPT-6 Astra/high for bounded
planning and independent reviews. Ask Codex to use `astra_adviser` for a named
design or review question, then implement the result with Sol. Start a new session
to use changed project defaults; a live session's explicitly selected model may
take precedence.

The read-only agent default is complemented by instructions forbidding writes;
live permission overrides can still affect the agent's sandbox. It is not a
separate security boundary from the parent session. Claude retains its own model
selection and credentials.

Configuration follows the [official Codex subagent documentation](https://learn.chatgpt.com/docs/agent-configuration/subagents).
No paid model runtime is enabled by these files. A separately API-driven workflow
would need explicit per-agent models, provider credentials, disclosure policy and
budgets. The unidentified “Decision API” is not an onboarding dependency.

## Implementation environment

The approved plan was checkpointed in an isolated development service on Monolith
at `http://127.0.0.1:4250`, backed by the disposable
`memory-onboarding-dev-pg-20261005` database container. The development
configuration is outside the repository, under
`/home/olivier/.local/state/memory-mac-onboarding-dev/`. Provider calls, background
work, federation and watchers are not enabled there.

This development service is not the company console deployment and contains no
customer data. The pre-existing dev overlay that points at the retired shared
database was deliberately left untouched.
