# MCP Server

`codifix mcp` exposes Codifix to MCP clients such as Codex and Claude.

The built-in server is read-only in this release. It can query, resume, inspect project overview data, list memories and activities, read memory details/history, and list pending replacement proposals. It does not expose remember, capture, curate, archive, delete, reindex, or embedding mutation tools.

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
command = "memory"
args = ["mcp", "run", "--project", "memory"]
```

Example Claude Desktop-style server entry:

```json
{
  "mcpServers": {
    "memory": {
      "command": "memory",
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

HTTP tools always require an explicit `project` argument because the service process has no trustworthy client working directory.

`codifix_search_all` is the exception: it intentionally has no `project` argument and returns routing metadata for every matched memory.

## Troubleshooting

- **Service down:** start the backend with `codifix service run` or `codifix service enable`, then run `codifix mcp status --project <slug>`.
- **Missing project:** pass `--project` for stdio or include `project` in each HTTP tool call.
- **Invalid token:** create or rotate a scoped token with `codifix auth token create`, then update the MCP client. In single-user mode, use `codifix service ensure-api-token` for the legacy installation token.
- **Stdout pollution:** stdio clients fail if wrappers print banners or logs to stdout before JSON-RPC frames. Run `codifix mcp run` directly.
- **Unsupported transport:** use stdio unless your client supports MCP Streamable HTTP.
