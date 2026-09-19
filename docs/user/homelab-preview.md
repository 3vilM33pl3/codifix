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

For the terminal interface:

```sh
memory-homelab
memory-homelab health
memory-homelab query --project homelab --question "What do we know about SigNoz and blackbox probes?"
```

The first command opens the TUI; use its memory browser and detail view. The CLI
query currently attempts its configured, deliberately unavailable local model
endpoint before returning the deterministic fallback. It does not contact a paid
provider. The API also supports explicit `answer_mode: "deterministic"` without
that attempted connection.

## What is and is not active

Homelab has **no enabled Pi supervisor, external model, federation, background
learning or raw capture ingestion**. No provider credential is installed in its
application. It cannot yet ask Workshop remotely. The current two-record preview
is not the completed dataset migration or four-area demo.

A restricted internal answering identity is prepared with only Homelab read and
export permissions. It has no login credential and is not yet granted remote or
collection access; its presence does not enable sharing.

Workshop's existing model permission does not grant Homelab permission to send
data to models. The original shared dataset and its writers are unchanged;
Trust/CA and General are not activated. Neither preview is a LAN/customer
deployment: the UI is restricted to host loopback and uses local-owner access.

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
restricted-identity upgrade. These full bundles have not been restore-rehearsed,
and ciphertext hash verification does not
prove decryption-key recovery. Never start an active clone or replay the original
import into the running target. Recovery requires an isolated, non-autonomous
target and verification before switching any client.

See the [evidence and remaining exchange work](../reviews/four-area-network/homelab-exchange.md).
