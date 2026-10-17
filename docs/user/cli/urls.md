# `codifix urls`

List configured Memory entry points with descriptions. Web interfaces appear
first, followed by API/agent interfaces and known federation gateways.

```bash
codifix urls
codifix urls --json
codifix urls --area workshop
codifix urls --config /path/to/config.toml
codifix urls --federation-config /path/to/participant.json
```

The command reads operator-owned area routing plus existing explicit, global,
repository and repository-development configurations. Use `--area-routing` to
select a routing file. `--area` filters local configurations; areas without a
local configuration, such as Trust on another host, appear as remote-only notices.
The optional participant configuration can also be selected through
`CODIFIX_FEDERATION_CONFIG`; only its gateway URL is displayed, not PDS
internals or credentials.

Web entries explain navigation, including **More → Access** for administration.
Tabs are not advertised as invented deep links. Multi-user identity-provider
login appears only when configured; disabled HTTP MCP is omitted. A configured
public URL takes precedence over the bind address; wildcard binds become
loopback links. Public URLs may include a reverse-proxy path prefix.

Output is a **configuration inventory**, not an access grant or health check.
The command makes no network requests, opens no browser, starts no services and
needs no database connection. Individual unreadable configurations produce
sanitized notices without preventing other entries from being listed. Tokens,
passwords, token-bearing/query URLs and database connections are never printed.

JSON output has `schema_version: 1`, `configured_only: true`, an `entries` array
(category, installations, URL, description and access requirements), and `notices`.
Identical entry points retain all installation labels. For the complete REST
endpoint list, use the listed OpenAPI YAML URL.
