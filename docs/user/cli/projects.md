# Project onboarding and synchronization

`memory projects` manages company-authorized laptop projects. Use the full native
package for synchronization; lean builds report the missing capability explicitly.
The complete native Mac installation still requires real-Mac acceptance.

First enroll with `memory auth login --company https://company.example --name laptop`.
Browser approval and a macOS Keychain credential are separate from Git credentials.

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
The native setup operator supplies profiles; do not place credentials in JSON files.

`shared_publications_v1` downloads approved publications only. The separately
authorized `curated_project_v1` mode downloads curated project history and supports
curated contributions when the device has explicit write consent. Captures and
session bodies stay local. Pending uploads are not accepted owner versions, and
corrections never overwrite another origin's history directly.

Inspect downloaded history with
`memory history CANONICAL_UUID --subscription SUBSCRIPTION_UUID`.
Company enrollment commands use their explicit company origin; local correction
staging and history use the selected local area/configuration.

See [Mac onboarding](../mac-onboarding.md) for setup, disclosure, recovery and
current delivery limitations. Git commit, merge and push remain manual Git work.
