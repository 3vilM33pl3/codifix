# Agents

Use Codifix to answer a question from a previous session and leave enough
evidence for the next developer or coding agent to check the answer.

## How agents connect

### CLI

A shell-capable agent can query a project and record a verified result with
the installed `codifix` CLI. Check the service, area and project from the
initialized repository before reading or writing. See
[Areas and projects](https://codifix.dev/docs/areas).

### MCP

MCP exposes callable tools. For example, an agent can read a briefing with
`codifix_resume`, ask a question with `codifix_query`, and inspect a cited
record with `codifix_get_memory`. For Codex Desktop, the plugin registers
the local CLI's stdio server. Enable one Codifix connection in the client.
See [Codex Desktop plugin](https://codifix.dev/docs/codex-plugin) and
[MCP setup and permissions](https://codifix.dev/docs/mcp).

### Repo-local instructions

A skill gives the agent instructions for a workflow, such as when to query,
what evidence to inspect, and how to save completed work. Preview generated
project files with `codifix wizard --dry-run`; the initializer can install
repo-local skills under `.agents/skills/`. The desktop plugin also supplies
a user-level Codex skill.

An enrolled specialist can **perform an action** using the permissions its
operator configured. For example, an approved Linear draft can become a
tracked issue. A normal project query stays in its selected area. Asking
another area for expertise requires its own destination and disclosure
approval. See [Specialists and operator workflows](https://codifix.dev/docs/specialists)
and [Private expertise](https://codifix.dev/docs/federation/expertise).

## Agent prompts

“Resume this project, find the decision behind the plugin connection, and
show me the source that still supports it.” The following walk-through uses
the Codifix 3.1.0 checkout as an example. Substitute the project slug that
your installation assigned to that checkout.

### Install Codifix

For a new machine, follow [Install](https://codifix.dev/docs/install).
For an existing team installation, first obtain the approved service address
and project access through [Team setup](https://codifix.dev/docs/team-setup).

### Configure a project

Run `codifix wizard --dry-run` in the checkout, inspect the proposed files,
then run `codifix wizard` when ready. From the same directory:

```bash
project=codifix
codifix status --project "$project" --json
codifix resume --project "$project"
codifix query --project "$project" \
  --question "How does the Codex plugin connect to Codifix?" --json
```

## Briefings

Follow the answer's citation number to its ranked result. Inspect the result's
source path and excerpt. With MCP, pass the cited record's UUID as
`memory_id` to `codifix_get_memory`. If the project has no matching decision,
report the gap and check the repository directly.

For this example, open `plugins/codifix/.mcp.json` in the checkout. It declares
the executable, arguments and forwarded token variable. Check the plugin's
configuration and guide against the repository's contract:

```bash
python3 scripts/check-codex-plugin.py
```

After that check passes in your checkout, preview the record below. Review
the destination and evidence, then repeat the command without `--dry-run`
to save the result. If the check fails, record the actual failure instead.

```bash
codifix remember --project "$project" --dry-run \
  --title "Checked the Codex plugin connection" \
  --summary "The plugin starts codifix mcp run over stdio." \
  --note "Verified in plugins/codifix/.mcp.json at $(git rev-parse --short HEAD)." \
  --test-passed "python3 scripts/check-codex-plugin.py"
```

Leave the next session one open question. A checkpoint on the same machine
appears in the next resume briefing:

```bash
codifix checkpoint save --project "$project" \
  --note "Plugin configuration checked. Next: confirm the first project query in the intended client."
codifix resume --project "$project"
```

## Recommended behavior

Keep the verified finding, source revision, check result, and next action
together. Omit credentials and unrelated conversation history. If a query
returns conflicting decisions, check their dates and source files before
choosing one.

## Next

See [Daily workflow](https://codifix.dev/docs/daily-workflow) for background
jobs and recovery, or [Agent interoperability](https://codifix.dev/docs/how-it-works/agent-interoperability)
for protocol versions and validation gaps.
