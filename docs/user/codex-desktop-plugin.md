# Codex Desktop Plugin

The Codifix Codex Desktop plugin packages the supported stdio MCP connection and a desktop-oriented workflow skill. It does not run another service, store a credential value, or replace the repository-local `.agents/skills/` bundle.

## Before installing

- Install the `codifix` CLI and make sure its local service is healthy.
- Initialize the repository with `codifix wizard` or `codifix init`.
- In multi-user mode, provide Codex with a scoped service token through its process environment. The plugin refers only to the token variable name; it never contains the token value.
- Check the active project first:

  ```bash
  codifix mcp status --project <project-slug>
  ```

## Install from this repository

From the Codifix repository root:

```bash
codex plugin marketplace add "$PWD"
codex plugin add codifix@codifix
```

Start a new Codex task after installation so it loads the plugin skill and MCP configuration. The MCP server uses the active Codex workspace unless an individual tool call supplies another project.

Use exactly one Codifix MCP connection. Disable any hand-authored `mcp_servers.codifix` configuration before enabling the plugin so `codifix_*` tools are not registered twice.

## How it fits with project setup

The included `codifix-codex` skill helps Codex retrieve or resume project context through MCP and use the CLI for plan checkpoints and completed-work capture. It complements, rather than overwrites, the generated `.agents/skills/` bundle.

Run `codifix init` or `codifix wizard` whenever the repo-local bundle needs installing or refreshing. The desktop plugin is a user-level integration; it does not write project instructions for you.

## Source and dev mode

When developing Codifix from source, run the isolated backend with:

```bash
cargo run --bin codifix -- service run
```

The source profile uses port `4250` and separate runtime state. Do not point a plugin task at it by accident when you intend to use the packaged service on `4040`; verify with `codifix mcp status --project <project-slug>` before relying on results.

## Versioning and maintainer validation

The checked-in plugin manifest and workspace both declare `3.0.0-rc.1`. This identifies the current source contract, not proof that every installed rc.1 package contains subsequent source changes or has passed desktop validation. A change to the MCP tool surface or the bundled workflow-skill contract needs a compatibility review and a plugin version update.

Validate the repository-owned contract with:

```bash
python3 scripts/check-codex-plugin.py
```

Maintainers should also run Plugin Creator's `validate_plugin.py` against `plugins/codifix` from their installed Plugin Creator skill. This checks the desktop-plugin package in addition to the normal repository tests.

## Verify and troubleshoot

```bash
codifix mcp status --project <project-slug>
codifix auth whoami
python3 scripts/check-codex-plugin.py
```

- **`codifix` not found:** install Codifix so the Codex process can find it on `PATH`.
- **Service unavailable:** start or repair the local service, then rerun MCP status.
- **Authentication failed:** refresh the scoped token, restart Codex, and check `codifix auth whoami`. The plugin explicitly passes `CODIFIX_CLIENT_TOKEN` to the MCP process because Codex filters ordinary child-process variables whose names contain `TOKEN`.
- **Wrong project:** open an initialized repository or pass the intended project to the relevant tool call.

See also: [Agents](https://codifix.dev/docs/agents), [MCP](https://codifix.dev/docs/mcp), and [daily workflow](https://codifix.dev/docs/daily-workflow).
