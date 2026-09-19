# Homelab local preview

On Monolith, open [Homelab](http://127.0.0.2:4261/). Set **Project** to
`homelab`, then select **Load**. Workshop remains separately available at
[Workshop](http://127.0.0.1:4260/?questions=60).

This is an independently authenticated application and database, not a filter
over Workshop. It contains two real historical observability memories: SigNoz
endpoint ownership and Docker blackbox-probe routing. They describe observations
from April and May 2026; they are not a fresh verification of today's homelab.

## Short demonstration

1. Load `homelab` and show **2 memories, 0 captures**.
2. Open either memory and inspect its content, dates and provenance.
3. Explain that the original source records remain in a protected archive.
   Archive references are visible, but source paths and raw sessions are not
   copied into this instance. An unverified source warning is intentional.
4. Open Workshop separately to show its distinct Memory-development collection.
   The credentials and database are different; the previews cannot read one
   another's project data directly.

### Ask across the boundary

The two-instance exchange is now running through the private company gateway and
ATProto profile. Collections remain on their origin instance; only reviewed
capability advertisements are published. Questions and answers use the private
gateway mailbox, not the PDS or the ordinary retrieval corpus.

1. In Workshop (`memory` project), choose **More tabs → federation**.
2. Select **Discover expertise**, then **Select expertise Reviewed Homelab expertise**.
3. Enter `What do we know about SigNoz and blackbox probe routing?` Leave the mode
   on **Deterministic excerpts (no synthesis)**.
4. Select **Prepare remote question**. Check the destination, exact question and
   local-human disclosure policy, tick the approval checkbox, then **Send approved question**.
5. Select **Fetch expertise result** until the status is `answered`. Inspect the
   original author, exact source version, answering instance and content digest.

Preparing does not send. A queued response is not yet an answer. If a response
is lost, retain the request ID and retry the exact question before its two-minute
execution deadline; do not create replacement IDs blindly. An answered request
can be read during its 24-hour payload window, but each read still requires
fresh approval from the answering instance. Previously delivered bytes cannot be
recalled. The completed browser example is request
`cffd69b6-eecf-448a-9d89-545e3095cdf0`; its payload expires on
2026-09-20 at 13:02 Europe/London. It is demonstration evidence, not a permanent link.

For the reverse direction, open Homelab, select **Reviewed Workshop expertise**
and ask about the Memory-development knowledge it advertises. This small initial
collection is historical; it does not yet announce or install new releases.

For the terminal interface:

```sh
memory-homelab
memory-homelab health
memory-homelab query --project homelab --question "What do we know about SigNoz and blackbox probes?"
memory-homelab federation --project homelab --local-human-session capabilities
memory-homelab federation --project homelab --local-human-session tui
memory-workshop federation --project memory --local-human-session tui
```

The first command opens the ordinary local TUI. The focused federation TUI uses
F2/F3 to choose an operation, F5 to read/prepare, and F6 to confirm the exact
prepared action. The explicit local-human session is for an actual human at the
terminal: it is not permission to forward answers to a cloud assistant or MCP.
See the [federation CLI guide](cli/federation.md) for reviewed JSON-file commands.
The CLI
query currently attempts its configured, deliberately unavailable local model
endpoint before returning the deterministic fallback. It does not contact a paid
provider. The API also supports explicit `answer_mode: "deterministic"` without
that attempted connection.

## What is and is not active

Homelab has **no enabled Pi supervisor, external model, background learning or raw
capture ingestion**. No provider credential is installed in its application.
Private deterministic expertise exchange with Workshop is active. The current
two-record preview is not the completed dataset migration or four-area demo.

A restricted internal answering identity has only Homelab read/export permissions,
the local answering capability and access to its exact reviewed collection. It
has no login credential. The collection allows only Workshop as requester and
only local-human output. Workshop's separate collection allows only Homelab.
Membership alone does not permit a third instance to use either collection.

Workshop's existing model permission does not grant Homelab permission to send
data to models. The original shared dataset and its writers are unchanged;
Trust/CA and General are not activated. Neither preview is a LAN/customer
deployment: the UI is restricted to host loopback and uses local-owner access.

The new private `memory.robotmotel.com` names are Docker-network aliases, not
public DNS or LAN endpoints. A scoped preview issuer supplies TLS; no existing CA
service was changed. Its leaf certificates last seven days. PDS publication
session refresh and certificate renewal remain explicit operator work; this is
not an unattended production deployment.

## Local operation

The relay starts with the user session. Inspect only these resources:

```sh
systemctl --user status memory-homelab-preview-relay.service
docker ps --filter name=memory-homelab-preview
memory-homelab health
```

If the app process alone needs restarting:

```sh
docker restart memory-homelab-preview-app-20260919
```

Do not use `memory service restart-all`. Do not run the original saved binary
against this prepared runtime role: it assumes migration-owner privileges. The
current app uses [prepared-database verification](prepared-database.md) at schema
`0060`, with a non-owner database role. If the database container is recreated,
its private address must be revalidated and the fixed relay target updated; a
restart of the same container is different from recreation.

Old containers named `memory-*-preview-app-before-company-f1` are stopped rollback
artifacts, not spare active instances. Never start both writers. Do not run
Compose orphan-removal operations on `memory-company-20260919`: its dedicated
database has a separate Compose file. Gateway, raw PDS, member and operator
networks must remain separate.

## Backups and recovery boundaries

Private configuration, import/restore receipts, exact artifacts and helper
sources live in
`~/.local/state/memory-homelab-preview-20260919`.
Encrypted copies are on the NAS under
`/media/bunnychow/backups/memory-four-area-20260917/homelab-preview-20260919`.
Those bundles contain this instance's database credentials and must remain
private; they contain no provider key.

Before activation, a database restore matched all 114 logical tables. The fresh
`homelab-runtime-verified-local.tar.gpg` additionally preserves the running binary,
Web assets, dump, private configuration and relay/operator helpers. That full
bundle contains the initial prepared runtime. The later
`before-answerer-setup.tar.gpg` preserves a fresh stopped-app backup before the
restricted-identity upgrade. `before-company-f1.tar.gpg` preserves the stopped-app
state before joining the new company gateway. These full bundles have not been restore-rehearsed,
and ciphertext hash verification does not
prove decryption-key recovery. Never start an active clone or replay the original
import into the running target. Recovery requires an isolated, non-autonomous
target and verification before switching any client.

See the [evidence and remaining exchange work](../reviews/four-area-network/homelab-exchange.md).
