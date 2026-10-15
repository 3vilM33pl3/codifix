# Authentication And Access

`memory auth` inspects the current principal and administers service tokens and
explicit project memberships. Browser users sign in through Authentik; CLI,
TUI, watcher, relay, and MCP clients use a Memory Layer service token.

## Modes

- `single_user` is the default and preserves existing local installations. The
  configured `service.api_token` remains the installation-wide credential.
  Explicitly issued service tokens also work, limited to their current project
  permissions, expiry and revocation. Local browser session bootstrap is unchanged;
  OIDC is not needed to give an agent a scoped token. Invalid supplied tokens fail
  rather than falling back to the local owner or anonymous reader.
- `multi_user` enables Authentik OIDC browser sessions, scoped service
  principals, project memberships, and role checks. The legacy token is
  rejected unless `auth.multi_user_legacy_token_enabled = true` is set for a
  temporary bootstrap or migration window.

Role names are presets for explicit permission sets: `reader` can
inspect/query, `writer` adds capture, `operator` adds curation and loop runs,
and `admin` receives every permission. Authorization checks the resulting
permissions rather than comparing roles as an ordinal ladder. A global preset
applies to every project; a project membership applies only to its project,
and permissions from applicable grants are combined.

## Client Credential

Set the issued token in the client process:

```bash
export MEMORY_LAYER_CLIENT_TOKEN='mlt_...'
memory auth whoami
memory query --project memory --question 'What changed?'
```

`MEMORY_LAYER_CLIENT_TOKEN` takes precedence over `service.api_token`. Do not
write a multi-user service token into a repository or command history.

## Inspect Identity

```bash
memory auth whoami
memory auth whoami --json
```

The output includes principal ID, kind, role presets, expanded permissions,
and effective project access. A `401` means the credential is missing,
expired, revoked, or invalid. A `403` means the credential is valid but lacks
the required permission or project membership.

## Service Tokens

Only a global admin can create, list, or revoke tokens:

```bash
memory auth token create \
  --name hermes \
  --project memory \
  --role writer \
  --ttl 30d

memory auth token list
memory auth token revoke <token-uuid-or-unique-prefix>
```

Creation makes a service principal and an initial project membership. The raw
`mlt_...` secret is displayed once; Memory Layer stores only its SHA-256 hash.
Store the secret in OpenBao or another secret manager before closing the output.
Use memberships to grant the same service principal access to more projects.

## Internal expertise answerer (installation operator)

Use this only when preparing the optional private federation answering worker.
It requires **global `system_admin`**, not a project administrator. It creates an
internal identity with exactly one project and explicit `memory_read` plus
`bundle_export` permissions. It issues no token or browser session and enables
no worker, space membership, collection consent or model destination.

Select the intended instance's private client configuration, verify its URL and
`auth whoami`, and retain two new UUIDs for the operation and principal. Do not
rely on the development default configuration for installation. First preview:

```sh
memory --config /private/selected-instance/client.toml auth expertise-answerer \
  --request-id <stable-request-uuid> --principal <new-principal-uuid> \
  --project homelab --name 'Homelab deterministic answerer'
```

Inspect the project, name and IDs, then repeat the exact command with `--confirm`
to commit. `--json` returns machine-readable metadata without credentials. After
an ambiguous response, retry only the identical request and IDs. A changed or
revoked identity is rejected; replay cannot silently restore its access. The
principal, membership and protected authentication audit commit together. The
operation creates no project timeline event or background job.

The API is `POST /v1/auth/expertise-answerers` with
`{request_id, principal_id, project, name, confirmed: true}`. Browser calls require
CSRF/Origin checks. Read-only, evaluation and relay services reject it. Existing
global-admin principal/membership inventory shows the result; revoking its local
project membership stops answering. Do not add other project memberships or
change its reviewed identity through ordinary access-administration commands;
those changes require separate review and invalidate provisioning replay.

Next, independently review the local `answer-expertise` space grant, exact
collection/service grant and `expertise_principal` runtime binding as described
in [federation runtime](../federation-runtime.md#f1-private-expertise-opt-in).
Joining a space is not collection consent. Keep deterministic mode and empty
model destinations for the initial internal installation.

## Project Memberships

```bash
memory auth membership grant \
  --principal <principal-uuid> \
  --project another-project \
  --role reader

memory auth membership list
memory auth membership revoke <membership-uuid>
```

Explicit memberships coexist with Authentik group mappings. Revoking an
explicit row does not remove access granted by a configured group rule.

## Browser Login

The bundled web UI redirects unauthenticated users to `/v1/auth/login`. Memory
Layer validates OIDC discovery metadata, signature, issuer, audience, state,
nonce, and PKCE before creating a 12-hour session by default. The provider
tokens are discarded. Browser writes also require a matching Origin and CSRF
value.

General API automation uses a scoped service token, not reused browser cookies.
The unreleased Mac onboarding candidate adds a separate browser-approved device
flow:

```bash
memory auth login --company https://YOUR-COMPANY-MEMORY-ORIGIN --name "My Mac"
memory projects available --company https://YOUR-COMPANY-MEMORY-ORIGIN
memory auth logout --company https://YOUR-COMPANY-MEMORY-ORIGIN
```

The device secret remains in macOS Keychain; it is not a general service-token
replacement and grants only explicitly approved onboarding scope. Separate
Codex/Claude disclosure grants remain required. Production releases without
these commands should retain service-token automation. See [Mac onboarding](../mac-onboarding.md)
for the guided first-run candidate and its outstanding native acceptance.

## OpenBao Pattern

OpenBao distributes the secret but is not a runtime dependency of Memory Layer:

```bash
bao kv put secret/memory-layer/hermes token='mlt_...'
export MEMORY_LAYER_CLIENT_TOKEN="$(bao kv get -field=token secret/memory-layer/hermes)"
memory auth whoami
```

Keep human login in Authentik and machine-secret delivery in OpenBao. Do not
store Authentik client secrets or Memory Layer service tokens in source control.

## See Also

- [MCP Server](mcp.md)
- [Service Commands](service.md)
- [Web UI](../web-ui.md)
- [Authentication Architecture](../../developer/architecture/authentication.md)
