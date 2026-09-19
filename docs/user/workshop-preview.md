# Workshop local preview

Available on Monolith at [Workshop](http://127.0.0.1:4260/?questions=60).
Reload this address to load the question-enabled Web assets. This is a separate installation
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
4. In Supervisor, click **Load saved proposals**, then **Open proposal** for
   `e711c518-a60c-467a-9c3e-55f4d7ae91b2`. Read the original question, Pi's answer
   and all three exact source-version citations. Pi explained ATProto's role and
   proposed a human check before cross-area sharing; it did not execute a change.
5. **Approve** or **Reject** opens a confirmation showing the result digest and
   request ID. Confirm only your actual decision: it is permanently recorded.
   For a demonstration without a decision, close the confirmation. Approval
   records human review only; it does not execute a suggestion, change a memory,
   install software or make another model call.

The Supervisor feed deliberately contains metadata, not private proposal text.
Private results have their own project-scoped access check and are not part of
that feed. The original `first-live-result.md` is also retained as an export.
One earlier setup failure is retained in history; it occurred before any model
call and was reconciled. Do not describe the failed action as another charge.

## Ask Workshop a new question

1. In **More → Supervisor**, enter an exact question such as
   `What should we verify before demonstrating federation?` and click
   **Prepare question**. Preparation is free: it neither contacts OpenAI nor
   queues work.
2. Review the displayed question, OpenAI destination, `gpt-5.6-sol`, three
   approved source records, profile/request hashes and daily limits. Only the
   question and this frozen collection are sent—not conversation history, files
   or ordinary-query results.
3. Click **Confirm paid question** only when those terms are correct. The
   separate worker processes explicitly confirmed questions under the existing
   **$2.50 / 20-generation daily ceiling**. Queued does not mean answered.
4. Use **My questions**, inspect the request and **Refresh question status**.
   When available, click **Read cited answer**. Approval/rejection is still a
   separate human review; neither executes the proposal.

The current standing disclosure grant expires on **19 September 2026 at
14:46:41 London time** (`2026-09-19T13:46:41.758666190Z`). Execution and delivery
of answers from the new question queue fail closed after expiry. Renewing that
grant requires an explicit operator review; restarting the worker does not
extend it. Changed/withdrawn source versions and revoked submitting credentials
also block disclosure. Already delivered bytes cannot be recalled.

Keep the request UUID and confirmation hashes after an ambiguous response.
Inspect status or retry the **same submission**, not a newly generated question.
An `uncertain` provider attempt is not automatically repeated; starting a new
question may incur another charge. Requests have a ten-minute execution deadline
and there may be at most eight outstanding questions. The grant can expire
sooner. Question status is requester-private; saved answers remain project-private
and can be read by other authorized project readers while the original
requester's disclosure authority remains valid.

For the terminal interface:

```sh
memory-workshop
memory-workshop supervisor
memory-workshop supervisor proposals --project memory
memory-workshop supervisor result --project memory --action e711c518-a60c-467a-9c3e-55f4d7ae91b2
memory-workshop supervisor questions --project memory
memory-workshop query --project memory --question ATProto
memory-workshop health
```

The local wrapper is `/home/olivier/.local/bin/memory-workshop`. It always selects
this installation and intentionally disallows shared service/watcher management.
In the TUI, select **Supervisor**, press `r` for metadata or `v` for private
proposals, then Enter to open the selected result. Use `j`/`k` to scroll, `a`/`x`
to prepare approval/rejection and Enter to confirm. `c` closes the confirmation;
Escape returns to metadata. For scripted review, see the
[CLI instructions](cli/supervisor.md#private-proposals-and-human-review).
For a new TUI question, enter the private pane with `v`, then press `q`, `n`,
type the question and Enter to prepare; read the terms before Enter confirms a
potentially paid call. Use `l` to list questions, Enter for status and Enter on an
available answer to read it. See the exact [CLI confirmation commands and TUI
keys](cli/supervisor.md#explicit-private-questions); use `memory-workshop` instead
of `memory` to target this installation.

## What stays running

The private app and database have `unless-stopped` restart policies. The
`memory-workshop-preview-relay.service` user unit is enabled for the user session.
It listens only on host loopback. The app itself listens only on its container's
loopback; its network has no internet default route. Browser sessions keep normal
loopback/CSRF checks; no forwarded-address header grants owner access.
The relay permits at most 32 Web connections so browser and TUI reads can coexist;
the five-minute idle timeout and the app's 128-PID/thread limit remain unchanged.

Deterministic health observations run every five minutes. They inspect counts,
not knowledge contents, and do not contact a model. The separate
`memory-workshop-question-worker.service` user unit is enabled and running for
this preview. It polls every two seconds and processes **explicitly confirmed
queued questions only**, one at a time. It does not generate questions,
continuously call a model, or autonomously change knowledge or install releases.
The completed original one-work manifest cannot trigger another generation by
being restarted.

The question worker has `Restart=no`: a credential, configuration or storage
failure requires inspection rather than an automatic restart loop. Check or
pause it independently from the app:

```sh
systemctl --user status memory-workshop-question-worker.service
systemctl --user stop memory-workshop-question-worker.service
# After inspecting the failure and confirming current grant/policy authority:
systemctl --user start memory-workshop-question-worker.service
```

Stopping during a provider request does not prove it was unbilled. Its uncertain
state remains recoverable and is not permission to regenerate the answer.

Inspect or restart only these preview components when needed:

```sh
docker ps --filter name=memory-workshop-preview
systemctl --user status memory-workshop-preview-relay.service
docker restart memory-workshop-preview-app-20260918
systemctl --user restart memory-workshop-preview-relay.service
memory-workshop health
```

Restarting the relay also stops its dependent question worker. After the relay
and app are healthy, inspect current authority and explicitly start the question
worker again if queued execution should resume. It does not auto-restart itself.

Do not use `memory service restart-all` for this preview. Do not start the retained
`memory-workshop-preview-app-initial-transport-20260918` or
`memory-workshop-preview-app-before-interaction-59` or
`memory-workshop-preview-app-before-questions-60` containers. They are stopped
historical artifacts, not supported writers. They predate the current schema;
starting any of them against the upgraded database is not a rollback procedure.
After recreating—not simply
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
The review UI upgrade has a separate encrypted `before-interaction-59.tar.gpg`
backup, copied to the same NAS location and checksum-verified before migration
0059. It preserves the pre-upgrade database and private configuration, without
the provider key. It has not had a restore rehearsal.
The question upgrade has its own encrypted **`before-questions-60.tar.gpg`**,
copied to the same NAS location and checksum-verified before migration `0060`.
It preserves the pre-upgrade database/configuration without the provider key;
it has **not** been restore-rehearsed. The current schema is migration `0060`.
Current app/worker binaries, Web assets and unchanged hash-bound work manifest
are under `questions-60`; `artifact.json` also binds the actual private
`app.toml` configuration. The `memory-workshop` wrapper selects this build.

Only the selected curated text can reach Pi/OpenAI through the frozen grant.
Raw captures, sessions, source-file bytes and other areas were excluded. Imported
source references are archive-only; an unverified source warning is honest, not
permission to scan the old host paths. Provenance references are not evidence
that source files exist inside this container.

This preview does **not** yet provide the four-area company demonstration:
Homelab/General/CA enrollment and migration, remote expertise, autonomous release
notifications/installations and messaging remain separate work. Explicit local
questions are now supported, but multi-turn supervisor chat and autonomous
knowledge upkeep are not. Do not describe the queue worker or a stored enabled
policy as a fully autonomous Pi supervisor.
See [execution evidence and remaining work](../reviews/four-area-network/workshop-live.md).
