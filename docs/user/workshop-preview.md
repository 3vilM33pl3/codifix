# Workshop local preview

Available on Monolith at **http://127.0.0.1:4260**. This is a separate installation
and database, not the existing dev or production service. Only three reviewed
Memory-development records are loaded. The original dataset is unchanged.

## Two-minute demonstration

1. Open the Web address. Leave the project as `memory` and click **Memories**.
   Show the three real records and their original version numbers.
2. Open **Query**, enter `ATProto`, and click **Query**. This is local lexical
   retrieval with citations. `llm is not configured` in its fallback explanation
   is intentional: ordinary questions do not invoke a paid model.
3. Select **More → Supervisor**, then **Refresh supervisor**. Show Workshop's
   identity, the $2.50 / 20-call daily ceiling and the recorded completed action.
   The first live run used one generation and cost $0.008608.
4. Read the private saved result at
   `/home/olivier/.local/state/memory-workshop-preview-20260918/first-live-result.md`.
   Pi explained ATProto's role and proposed a human check before cross-area
   sharing. It cited all three supplied records; it did not execute a change.

The Supervisor feed deliberately contains metadata, not private proposal text.
The result file is a genuine saved-output export, not an interactive chat screen.
One earlier setup failure is retained in history; it occurred before any model
call and was reconciled. Do not describe the failed action as another charge.

For the terminal interface:

```sh
memory-workshop
memory-workshop supervisor
memory-workshop query --project memory --question ATProto
memory-workshop health
```

The local wrapper is `/home/olivier/.local/bin/memory-workshop`. It always selects
this installation and intentionally disallows shared service/watcher management.
In the TUI, select **Supervisor**, press `r`, and use `j`/`k` to scroll.

## What stays running

The private app and database have `unless-stopped` restart policies. The
`memory-workshop-preview-relay.service` user unit is enabled for the user session.
It listens only on host loopback. The app itself listens only on its container's
loopback; its network has no internet default route. Browser sessions keep normal
loopback/CSRF checks; no forwarded-address header grants owner access.

Deterministic health observations run every five minutes. They inspect counts,
not knowledge contents, and do not contact a model. There is **no recurring paid
Pi schedule**. The completed one-work manifest cannot trigger another generation
by being restarted. A new proposal requires deliberately prepared new work.

Inspect or restart only these preview components when needed:

```sh
docker ps --filter name=memory-workshop-preview
systemctl --user status memory-workshop-preview-relay.service
docker restart memory-workshop-preview-app-20260918
systemctl --user restart memory-workshop-preview-relay.service
memory-workshop health
```

Do not use `memory service restart-all` for this preview. Do not start the retained
`memory-workshop-preview-app-initial-transport-20260918` container. It is stopped
diagnostic evidence, not another supported writer. After recreating—not simply
restarting—the database container, the operator must revalidate its private IP
and update the relay's fixed database destination.

The main application has no Docker socket or provider key. The separate trusted
operator worker has Docker authority and fixed OpenAI transport. This is not an
OS-wide egress firewall on the operator account or a cap on other applications
using the existing shared key. The deployment is local-user access, not LAN
publication, OIDC or customer readiness.

## Recovery and limits

Private configuration, exact work identities, artifact hashes, import/restore
receipts and saved result are in
`/home/olivier/.local/state/memory-workshop-preview-20260918`.
Encrypted backups are also copied under
`/media/bunnychow/backups/memory-four-area-20260917/workshop-preview-20260918`.
The pre-activation database restore matched all three memory rows and key counts;
its test database remains unable to accept connections. Never enable provider
execution on a restored clone while the original authority is live.
The newer `live-preview-recovery.tar.gpg` includes the completed proposal,
current database/configuration and operator helper sources, but not the OpenAI
key or binary artifacts. Rebuild/verify artifacts against the retained hashes;
that newer recovery bundle has not itself been restore-rehearsed.

Only the selected curated text can reach Pi/OpenAI through the frozen grant.
Raw captures, sessions, source-file bytes and other areas were excluded. Imported
source references are archive-only; an unverified source warning is honest, not
permission to scan the old host paths. Provenance references are not evidence
that source files exist inside this container.

This preview does **not** yet provide the four-area company demonstration:
Homelab/General/CA enrollment and migration, remote expertise, autonomous release
notifications/installations, messaging and supervisor chat remain separate work.
Do not describe a stored enabled policy as an always-running autonomous Pi agent.
See [execution evidence and remaining work](../reviews/four-area-network/workshop-live.md).
