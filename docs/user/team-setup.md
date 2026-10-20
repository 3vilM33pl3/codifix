# Team setup

Use this path when your organization already operates Codifix. Your administrator
approves which projects your device can use; you review the repository, tools,
knowledge copy and assistant access before applying the setup. For your own
standalone service instead, start with [Install](https://codifix.dev/docs/install).

These instructions follow current `main`. The published `3.0.0` includes
company/project commands; its standard package does not include the optional
combined native first-run/runtime bundle. Native guided provisioning is currently
a macOS candidate. Check the [Mac guide](https://codifix.dev/docs/mac-onboarding)
and [platform limits](https://codifix.dev/docs/help/known-limitations) before
choosing a package.

## Before you start

Have these ready:

- Your administrator's exact company HTTPS origin and the approved client package.
- Your human sign-in and project assignment.
- Your own Git credentials for the selected repository.
- An existing projects parent and a private state directory outside repositories.

A company project is identified by its area, owner instance and project UUID.
The project name alone is not enough. See
[Areas and projects](https://codifix.dev/docs/areas) if the destination is unclear.

## 1. Sign in and approve your device

```bash
codifix auth login --company https://knowledge.example.com --name "My laptop"
codifix projects available --company https://knowledge.example.com
```

Replace the example company origin with the one your administrator supplied.
The browser shows a public code and credential fingerprint. Check the device,
project and expiry before approving. Credentials stay in the supported OS
credential store; do not paste a token into a repository or onboarding journal.

Permission to read a project, keep an offline copy, contribute changes and send
content to a model are separate approvals. Reader devices do not gain write
access automatically when your human permissions change.

## 2. Select and review a project

On a TUI-enabled client, choose a project and review its setup:

```bash
codifix projects select --company https://knowledge.example.com \
  --workspace /absolute/projects/example-app \
  --journal /absolute/private/example-app-onboarding.json
```

Use absolute paths with existing parent directories. Keep the journal outside
the repository. Select the exact area/owner/project and inspect the reviewed
manifest: repository origin, pinned tools, templates and allowed destinations.

Selection prepares metadata; it does not clone the repository or start services.
To apply, use the digest from the reviewed preview:

```bash
codifix projects apply --company https://knowledge.example.com \
  --journal /absolute/private/example-app-onboarding.json \
  --plan-sha256 DIGEST_FROM_PREVIEW --confirm
codifix projects status --journal /absolute/private/example-app-onboarding.json
```

Existing repositories must match the approved origin. Branches, dirty work and
custom settings are preserved; conflicts stop for review. After interruption,
resume with the same journal and inputs. Do not delete an incomplete workspace
or start a fresh request merely because a response was lost.

For scripted preparation, see [Projects reference](https://codifix.dev/docs/reference/cli/projects).
For the guided native launcher, runtime prerequisites and installation recovery,
use [Mac onboarding](https://codifix.dev/docs/mac-onboarding).

## 3. Verify the knowledge you received

Use the private sync profile produced by the reviewed native setup:

```bash
codifix projects sync-status --profile /absolute/private/profiles/example-app.json
codifix projects offline-search --profile /absolute/private/profiles/example-app.json \
  --question "What should I know before working on this project?"
```

A local status records progress, not current remote permission. Downloads are
read-only owner copies; pending contributions are not yet owner acceptance.
The [offline guide](https://codifix.dev/docs/offline) explains freshness,
refreshing a subscription, upload receipts and corrections.

## 4. Connect an assistant separately

The native candidate has a separate assistant-access stage. Review the exact
project and Codex/OpenAI or Claude/Anthropic destination in the human browser
approval, then review the generated settings and reload the assistant.

Project assignment and offline-copy consent do not authorize model disclosure.
The project-cache MCP bridge provides only cited read-only retrieval for the
approved project. Custom MCP entries and the assistant's own model settings are
preserved. Setup itself makes no paid model calls.

Use [Agents](https://codifix.dev/docs/agents) and the
[Codex plugin](https://codifix.dev/docs/codex-plugin) for ordinary local integration.
Administrators should follow [Company administration](https://codifix.dev/docs/team-setup/administration);
private questions to another instance use the separate
[expertise workflow](https://codifix.dev/docs/federation/expertise).
