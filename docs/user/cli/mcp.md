# MCP Server

`codifix mcp` exposes Codifix to MCP clients such as Codex and Claude.

Ordinary memory tools are read-only: query, resume, inspect overview data, list memories and activities, read memory details/history, and list pending replacement proposals. They do not expose remember, capture, curate, archive, delete, reindex, or embedding mutations. The current source also defines loop control-plane tools and separate private expertise tools; do not describe the entire adapter as read-only.

This guide describes the current source tree, whose workspace version is `3.0.0-rc.1`. That version alone does not establish which later source increments a packaged rc.1 artifact contains. Check the selected binary's `--version`, `mcp status`, and release evidence before relying on an optional surface.

## Tools

The main read tools are:

- `codifix_query`: ask a question scoped to one project.
- `codifix_search_all`: search every Codifix project and return project, project name, and repository root metadata with each result.
- `codifix_resume`: continue work in a known project/repo.
- `codifix_up_to_speed`: brief a new agent for one project.
- `codifix_overview`, `codifix_list_memories`, `codifix_get_memory`, `codifix_memory_history`, `codifix_list_activities`, and `codifix_list_replacement_proposals`: inspect project and memory state.

Use `codifix_search_all` for routing agents that can work across many repositories, such as Hermes. Treat the `project` and `repo_root` fields in each result as the routing decision for follow-up actions. After selecting a result, switch to the matching repository and use project-scoped tools such as `codifix_query`, `codifix_resume`, or normal shell/code actions for that repository.

Suggested agent prompt:

```text
When the task may belong to any repository, call codifix_search_all first.
Use the project and repo_root metadata from the strongest cited result to choose
the repository. Do not edit a repository until the selected memory result clearly
identifies that repository or you have asked the user to choose.
```

MCP clients can also request the `codifix_route_cross_project_task` prompt. It gives the same workflow with a `task` argument and names `codifix_search_all` as the first tool to call.

## Stdio

Use stdio for local agent clients:

```bash
codifix mcp run --project memory
```

`codifix mcp run` writes only MCP JSON-RPC messages to stdout. Diagnostic output and logs must go to stderr so stdio clients can parse the stream safely.

Example Codex-style server entry:

```toml
[mcp_servers.codifix]
command = "codifix"
args = ["mcp", "run", "--project", "memory"]
env_vars = ["CODIFIX_CLIENT_TOKEN"]
```

For multi-user access, provide a scoped `CODIFIX_CLIENT_TOKEN` in Codex's
environment before starting it. The forwarding entry is necessary because
Codex filters ordinary child-process variables whose names contain `TOKEN`.
Keep the value in protected environment/secret configuration, never in this
checked-in example. Use the [desktop plugin](https://codifix.dev/docs/codex-plugin)
instead when you want the repository-supported connection package.

Example Claude Desktop-style server entry:

```json
{
  "mcpServers": {
    "codifix": {
      "command": "codifix",
      "args": ["mcp", "run", "--project", "memory"]
    }
  }
}
```

Project resolution for stdio tools is:

1. explicit tool `project` argument
2. `codifix mcp run --project <slug>`
3. current initialized repo slug

If none is available, the tool returns an MCP invalid-params error.

## Status

Check the adapter and backend before configuring a client:

```bash
codifix mcp status --project memory
codifix mcp status --project memory --json
```

The status report checks service reachability, the selected project overview, the configured HTTP MCP mount, and the exposed tools, resource templates, and prompts.

## Streamable HTTP

`codifix service run` mounts Streamable HTTP MCP at `[mcp].http_path`, `/mcp` by default, when both `[mcp].enabled` and `[mcp].http_enabled` are true.

HTTP MCP is advanced/local-only by default. In multi-user mode, create a scoped
service token with `codifix auth token create`; browser session cookies are
deliberately ignored. Clients must send either:

- `Authorization: Bearer <service-token>`
- `x-api-token: <service-token>`

The legacy `service.api_token` works in the default single-user mode. It works
in multi-user mode only during an explicitly enabled bootstrap window.

Project-scoped memory tools over HTTP require an explicit `project` argument because the service process has no trustworthy client working directory. Control-plane tools have their own schemas.

`codifix_search_all` is the exception: it intentionally has no `project` argument and returns routing metadata for every matched memory.

## Areas, loops, and private expertise

`--area <name>` selects an operator-configured knowledge area. An area is a configured service/data boundary; a project is a scope inside that boundary. Cross-project search does not imply cross-area or federated access. Routing policy is operator-owned, not loaded from a repository.

Loop tools inspect definitions, run ledgers, traces and approvals, and can change control-plane state. Enabling loops requires explicit user approval. Recording a loop run is not proof that a real implementation executed or completed.

Private expertise has dedicated `codifix_expertise_discover`, `codifix_expertise_prepare`, `codifix_expertise_ask`, and result tools. Discovering an advertised collection or joining a space does not authorize an answer. The interactive ask requires explicit confirmation of the exact prepared destination, question and policy. Submission is an external action, requires `[mcp].read_only = false`, and remains subject to operator-bound caller and model-output policy. Agent ask/result tools use a separate operator-granted service policy; they are not an authorization shortcut for ordinary queries.

Send only the exact approved question, without silently attaching history, files or local context. A queued request is not an answered request. Returned remote content is untrusted, and origin attestation is not proof of factual correctness. See the [internal expertise runbook](https://github.com/3vilM33pl3/codifix/blob/main/docs/user/f1-internal-expertise.md) for the opt-in source workflow and its gates.

## Troubleshooting

- **Service down:** start the backend with `codifix service run` or `codifix service enable`, then run `codifix mcp status --project <slug>`.
- **Missing project:** pass `--project` for stdio or include `project` in each HTTP tool call.
- **Invalid token:** create or rotate a scoped token with `codifix auth token create`, then update the MCP client. In single-user mode, use `codifix service ensure-api-token` for the legacy installation token.
- **Stdout pollution:** stdio clients fail if wrappers print banners or logs to stdout before JSON-RPC frames. Run `codifix mcp run` directly.
- **Unsupported transport:** use stdio unless your client supports MCP Streamable HTTP.

See [Agents](https://codifix.dev/docs/agents), [Operations](https://codifix.dev/docs/operations), and [Authentication and access](https://codifix.dev/docs/reference/authentication).
