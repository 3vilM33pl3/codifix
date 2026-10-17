# Native Windows onboarding coordinator

`onboard.ps1` supplies explicit native stages for **already provisioned** local
databases, user configuration, Memory services and curated sync profiles. It
does not install dependencies, generate profiles, provision services or complete
the first-run installer. It is not macOS/Windows acceptance evidence.

Run in the signed-in user's interactive PowerShell session, not SYSTEM, a
service token, S4U or an elevated installer session. Device credentials remain
in Windows Credential Manager; never put passwords/tokens in command arguments.
The executable must live in an immutable reviewed release directory. Configuration
and profile files need protected, dedicated directories without mutable logs or
repository data. Existing protected-release guards retain reviewed identities.

## Stages

Every invocation requires exact `-Memory`, `-MemorySha256`, `-ReleaseRoot`,
`-Config`, `-ConfigSha256` and canonical HTTPS `-Company` inputs. First omit
`-Execute` to print a metadata-only plan and `review_sha256`. Repeat with
`-Execute -ApprovalSha256 <that exact digest>` to perform that stage.

1. `-Stage Login -DeviceName <public-name>` uses existing browser-assisted login.
2. `-Stage Catalog` lists currently authorized projects.
3. `-Stage Prepare` takes exact `-Area`, `-Owner`, `-Project`, `-Revision`,
   `-Workspace` and private `-Journal`. Trust is remote-only. The existing CLI
   validates the authorized manifest and prepares its resumable workspace plan.
4. Review that CLI plan. `-Stage Apply -Journal ... -WorkspacePlanSha256 ...`
   confirms its exact digest and reuses existing clone/pinned-tool installation.
   Existing repositories/settings are preserved. Login is not tool/model consent.
5. `-Stage SubscriptionReview` takes the same composite project identity and
   `-SubscriptionOutput`. After reviewing disclosure, `SubscriptionConfirm`
   additionally requires its exact `-BootstrapSha256`. These create metadata,
   **not** a database-bearing runtime profile.
6. The installed-stack operator must separately prepare the private curated
   profile, its loopback database and local project binding. This coordinator
   deliberately does not invent a password, database or binding.
7. `SyncOnce`, `SyncStatus` and `EnableSync` require exact `-Profile`,
   `-ProfileSha256`, `-Area`, `-Owner`, and `-Project`. The reviewed profile is
   authoritative for the local database; `--config` does **not** override the
   project's sync database. Plans show only its non-secret loopback destination
   and full subscription scope. The shared Rust CLI validates the complete
   private profile and performs authorized transfers.
8. `EnableSync` validates local sync status, then registers an independently named
   current-user Interactive/Limited task. It starts immediately and at logon,
   running the existing `projects sync --watch` startup/reconnect/five-minute
   loop. It needs no password, service identity or administrator task. Existing
   task action, principal, trigger and complete settings must match; a foreign
   or modified task is never overwritten. IgnoreNew prevents concurrent copies.

The task checks exact executable/config/profile hashes on each launch. This
relies on the protected immutable release and current-user private profile
boundary, not atomic same-handle process execution. Changes require a fresh
review and explicit operator resolution of the prior registration. A task start
request is not proof that synchronization succeeded: inspect `SyncStatus` and
the native CLI's retained status/journals. Interrupted clone/apply/subscription
stages resume through the existing CLI journals; effects are never claimed to
have rolled back merely because the coordinator failed.

No paid provider, autonomous supervisor or automatic Git push is enabled.
No existing service or watcher is stopped or replaced. Repositories and watcher
routing remain responsibilities of the provisioned stack and existing routing
operators. A real Windows login→selection→clone→offline search→contribution→reboot
rehearsal is still required.

Portable policy checks (no login, network or service effects):

```powershell
pwsh -NoProfile -File packaging/windows/test-onboarding.ps1
```
