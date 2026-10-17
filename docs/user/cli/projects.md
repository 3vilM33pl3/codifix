# Project onboarding and synchronization

`codifix projects` manages company-authorized laptop projects. Use the full native
package for synchronization; lean builds report the missing capability explicitly.
The complete native Mac installation still requires real-Mac acceptance.

First enroll with `codifix auth login --company https://company.example --name laptop`.
Browser approval and the OS credential store (macOS Keychain or Windows
Credential Manager) are separate from Git credentials. The Windows candidate
still requires its native onboarding acceptance; a compiled credential adapter
alone does not establish the complete installation journey.

| Command | Purpose |
| --- | --- |
| `available` | List projects currently authorized for the enrolled device. |
| `select` | Guided selection and review of one project. |
| `prepare`, `apply`, `status` | Preview, confirm and resume the workspace setup journal. |
| `subscription` | Review a scoped download subscription and prepare metadata. |
| `sync --profile PROFILE` | Synchronize one native subscription; add `--watch` for continuous operation. |
| `sync-status --profile PROFILE` | Inspect committed freshness, pending uploads and sanitized failures. |
| `offline-search --profile PROFILE --question QUESTION` | Search the selected local projection. |
| `propose-correction --subscription UUID --request correction.json` | Preview a downloaded-source correction; `--confirm` stages it for owner review. |
| `agents prepare/status/configure/mcp` | Separate, explicitly approved Codex or Claude cache access. |

Use each command's `--help` for its required project, company and journal arguments.

The Windows port accepts local drive paths, checks physical identities rather
than trusting case-sensitive path spelling, and rejects junctions/reparse points,
reserved device names and alternate data streams. Its setup journals and locks
use protected Windows file handles. These paths have cross-compilation evidence;
the native onboarding journey and project agent-settings merge remain separate
acceptance items, not a completed Windows installation claim.
The native setup operator supplies profiles; do not place credentials in JSON files.

`shared_publications_v1` downloads approved publications only. The separately
authorized `curated_project_v1` mode downloads curated project history and supports
curated contributions when the device has explicit write consent. Captures and
session bodies stay local. Pending uploads are not accepted owner versions, and
corrections never overwrite another origin's history directly.

Inspect downloaded history with
`codifix history CANONICAL_UUID --subscription SUBSCRIPTION_UUID`.
Company enrollment commands use their explicit company origin; local correction
staging and history use the selected local area/configuration.

See [Mac onboarding](../mac-onboarding.md) for setup, disclosure, recovery and
current delivery limitations. Git commit, merge and push remain manual Git work.
