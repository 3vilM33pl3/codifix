# Agent interoperability

**Assessment for Codifix 3.1.0, 27 November 2026.** Use the
[Agents guide](https://codifix.dev/docs/agents) for the end-to-end workflow.
The following checks distinguish three questions a client integrator needs to
answer before rolling out a new version.

## Can the client and server exchange tools?

The 3.1.0 adapter declares MCP `2025-11-25`. A release check launched
`codifix mcp run` over stdio, sent `initialize` with that revision,
received `2025-11-25`, sent `notifications/initialized`, then requested
`tools/list`. The reply advertised `codifix_resume`, `codifix_query` and
`codifix_get_memory`. This exercise covered negotiation and tool discovery.
To verify retrieval with a real client, query an authorized test project and
open a returned citation. See the [tagged MCP implementation](https://github.com/3vilM33pl3/codifix/blob/v3.1.0/crates/codifix-mcp/src/lib.rs)
and [connection guide](https://codifix.dev/docs/mcp).

The current MCP revision is `2026-07-28`. It adds per-request protocol
selection and `server/discover`. Before advertising compatibility, exercise a
client using that revision over both stdio and Streamable HTTP. Check version
discovery, handling of an unsupported revision, authenticated tool calls,
and the existing initialization sequence with a 2025 client. Include a call
which the project's token must reject. Record both client and server versions
with the outcome. See [MCP versioning and backward compatibility](https://modelcontextprotocol.io/specification/versioning).

## Can another client run the same skill?

The plugin manifest points to the bundled `codifix-codex` skill and to an MCP
configuration that launches the CLI. The checked-in contract test covers the
manifest, version, documented commands and token forwarding. The Agent Skills
format describes a portable instruction file. Plugin discovery, permission
prompts, command execution and credential forwarding still depend on the host.
See the [3.1.0 plugin](https://github.com/3vilM33pl3/codifix/tree/v3.1.0/plugins/codifix)
and the [Agent Skills specification](https://agentskills.io/specification).

For each additional client, use a disposable project and a scoped token.
Verify that the host discovers the skill, selects it for a resume request,
retrieves the intended project, and prompts or fails safely when a required
capability is unavailable. Record the operating system, client version and
integration used. Treat CLI access, MCP access and skill activation as
separate results.

## Does the next session recover the right decision?

Codifix exposes cited query results, version history and resume briefings.
To evaluate what a fresh session actually recovers, seed a test project with
one decision, its evidence, and a correction that supersedes it. Have one
session record a checked outcome and one unresolved question. Start a fresh
session with the same project access and ask for the current decision, its
source revision, and the next step.

Compare the answer with the recorded correction and source. Count unsupported
claims, stale decisions, correct citations and recovered next steps. Repeat
with an empty project to test how the client handles missing evidence.
For any quality comparison, publish the fixture revision, client/model
versions, and results with and without the handoff. Existing evaluations
have their own suites and baselines. See [Codifix evaluations](https://codifix.dev/docs/evals)
and Anthropic's discussion of [context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).
