# Team setup

Start with your administrator's company HTTPS origin and the reviewed client
package for your platform. Sign in, select the exact projects you are allowed to
use, review what will be copied and installed, then approve each stage. Git
credentials and access to an AI provider remain separate from Codifix access.

This guide describes current source on `main`. The published `3.0.0-rc.1`
contains company/project commands, but its standard package is not the combined
native first-run/runtime bundle described by the operator workflow. Native
provisioning and production credential storage currently have a macOS candidate;
Linux fake-store tests do not establish a supported cross-platform installer or
a production plaintext credential fallback. Use the
[Mac onboarding guide](https://codifix.dev/docs/mac-onboarding) for its launcher,
Keychain, runtime prerequisites and remaining acceptance checks.

## Before you start

Ask your administrator to confirm the company origin, your verified human
identity and project memberships, the selected owner and area, and the reviewed
project manifest. A project manifest describes the repository, pinned tools,
templates and allowed caller/provider destinations. Its presence is not consent
to install software or disclose project content.

Choose an existing projects parent and a private state location outside your
repositories. Existing repositories and custom agent settings are preserved;
new clones use your own Git credentials. Trust is remote expertise only and is
not an eligible offline project.

## Sign in and review the device

On a configured native client, browser-assisted login starts with:

```bash
codifix auth login --company https://knowledge.example.com --name "My laptop"
codifix projects available --company https://knowledge.example.com
```

The company origin must be HTTPS without a path, credentials, query or fragment.
The browser presents a public code and credential fingerprint. Review the device,
expiry, exact owner/area/project identities, manifest hashes, offline-copy
consent, tools and output destinations. The client generates its credential;
polling does not return a bearer secret.

Company offline selections are reader-only by default. A separate curated-write
checkbox is offered only when current owner permissions permit contributions.
It binds consent to this device and exact project. A human receiving additional
permissions later does not silently expand the device's scope. A failed company
approval does not fall back to local-owner approval. Partial owner acceptance is
not completed enrollment; resume the exact recorded request.

## Prepare and apply a project

A TUI-enabled build can review an eligible project interactively:

```bash
codifix projects select --company https://knowledge.example.com \
  --workspace /absolute/projects/example-app \
  --journal /absolute/private/example-app-onboarding.json
```

Choose the exact area/owner/project, inspect its complete manifest, then consent
and confirm. This prepares metadata only. Lean builds explicitly report that
the picker is unavailable.

For explicit preparation, substitute the identities and revision from your
reviewed catalog. Both paths must be absolute with existing parents; the journal
must be outside the repository:

```bash
codifix projects prepare --company https://knowledge.example.com \
  --area workshop --owner-instance 11111111-1111-4111-8111-111111111111 \
  --project 22222222-2222-4222-8222-222222222222 --revision 1 \
  --workspace /absolute/projects/example-app \
  --journal /absolute/private/example-app-onboarding.json
codifix projects apply --company https://knowledge.example.com \
  --journal /absolute/private/example-app-onboarding.json \
  --plan-sha256 DIGEST_FROM_PREVIEW --confirm
codifix projects status --journal /absolute/private/example-app-onboarding.json
```

Prepare writes an immutable metadata journal, not a clone or credential. Review
the full preview before confirming its exact digest. Apply rechecks device scope,
manifest and repository origin. Changes stop the operation while retaining
completed stages. Status shows recorded local progress, not current remote
authorization. Resume with the same journal and inputs after interruption.

Existing repositories must match the reviewed origin; branches and dirty work
are preserved. New clones disable repository hooks, redirect following and
submodule recursion. Required filters and unsupported local Git filter/attribute
configuration stop preparation instead of executing those extensions. Partial
clones or publication conflicts require inspection, not automatic deletion.

Only missing reviewed template and known skill files are installed. Custom
settings are listed for manual review. The current pinned installer supports
the reviewed `memory-skills` ZIP layout, not arbitrary executable plugins.
Workspace preparation alone does not provision PostgreSQL, activate services,
enable paid model calls or complete synchronization.

## Approve assistant access separately

The native candidate has a separate `journey-agents` stage after repository and
service preparation. Each selected project gets independent Codex/OpenAI and
Claude/Anthropic credentials. The owning human approves each public request in
**Access → Devices → Agent access**. Project membership and offline-copy consent
do not authorize this model disclosure; the manifest must also allow the exact
caller/provider destination.

Review and confirm the generated settings plan, reload the assistant and accept
its normal project/MCP trust prompt. Codifix-owned MCP entries contain public IDs
and private profile paths, never tokens or database URLs. Custom MCP entries and
the assistant's own model configuration survive.

The project cache bridge exposes only `codifix_project_cache_status` and
`codifix_project_cache_search`: cited, read-only lexical retrieval for the exact
approved project. It cannot invoke a model or request Trust expertise. Treat
retrieved text as source data, not instructions. This scope is not an operating
system sandbox for an assistant with unrestricted filesystem/database access.
Keep private profiles outside agent workspaces and review agent permissions.

Online authority checks are the default. Explicit `--allow-offline` consent at
the native agent setup stage permits previously activated, unexpired access
during an outage; known revocation or denial still fences it. Grants last at most
24 hours and renewal requires a new credential and fresh browser consent.
Setup itself makes no paid model calls.

Continue with [offline and synchronization](https://codifix.dev/docs/offline),
[company administration](https://codifix.dev/docs/team-setup/administration), or
the [Mac candidate and recovery details](https://codifix.dev/docs/mac-onboarding).
