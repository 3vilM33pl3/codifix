# CA agent: complete the Trust rc.2 upgrade

Olivier authorized upgrading the existing Memory installations and completing
the four-area setup on 29 September 2026. Your goal is a working **Trust-local
Memory Layer 2.1.0-rc.2 installation**, with ordinary CLI/TUI/watchers routed to
Trust and policy-controlled expertise exchange with Workshop, Homelab and
General. You own the CA-side implementation and report; Olivier is not a log or
command relay. Use the existing Notion CURRENT ACTION and RESULT INBOX bodies.

This document is the procedure, not evidence that an archive has been delivered.
The accompanying delivery index must provide exact URLs, release and encrypted
partition hashes, the frozen source revision, and the current cutover state.
Never substitute a similarly named old rc.1 archive or an unverified `latest`.

## Preserve these boundaries

- Keep Trust instance `cc1bff80-d0e4-4687-9e62-9b8a5fe4bb16`, its existing
  database, enrollment, receipt-key trust, accepted history, cursors, collection
  grants and usage journals. Reconcile into it; do not replace it with a fresh DB.
- Keep PostgreSQL 17/trust and the application CA-local. Preserve the existing
  `memory-layer@trust` sandbox drop-in. Do not change Step CA, OpenBao, the OS,
  other PostgreSQL clusters, DNS, CA signing keys or the enrolled PDS identity.
- Use the existing schema-v3 private gateway profile, with v2 history preserved.
  No upstream protocol, PDS/Atrium or Pi upgrade; no paid models or delegation.
- The portable partition contains only reviewed Trust knowledge and opaque
  provenance references. Other areas' archives, raw captures and sessions must
  not be transferred here. Archive CA-only/offline material before interpreting
  it; preserve operation IDs and do not replay into a guessed destination.
- The package is authorized work within this scope. Resolve routine reversible
  packaging, path and service issues yourself. Report only a concrete failure
  that cannot be resolved without new authority, data loss or a changed design.

## 1. Prepare while Trust remains available

Inventory active and dormant Memory commands, symlinks, desktop launchers,
services, manager/watchers, MCP/plugin configurations and Memory-owned skills.
Record executable hashes and actual database destinations, not version labels
alone. Identify every legacy shared-Monolith writer, including resident clients
and any offline queue. Never print tokens, connection strings or raw bodies.

Fetch the exact signed release and encrypted Trust-only bundle named by the
delivery index. Verify its outer ciphertext hash before decryption using the
existing CA recipient. Use the already enrolled release signing subkey
`4EB55288032C60EF65C73A0352EE81AD6D98F325`; a key supplied beside the download
is not a replacement trust anchor. Authenticate both prior and candidate release
preparations. Verify ARM64, Debian-12/glibc compatibility and schema 60→61 using
the packaged tools. No password or private key belongs in Notion.

Prepare protected backups of Trust data and sequence states, configuration,
effective unit/drop-in, identities, receipt state, budgets and CA-only offline
work. Restore into a disposable isolated target and record the result. Keep
existing backups and rc.1 artifacts. A successful archive checksum is not a
restore test.

Report `READY_FOR_CUTOVER` with the inventory, exact candidate/bundle hashes,
restore evidence and CA-only pending items. Do not stop the live installation
until the delivery index/Notion action says `CUTOVER_READY` for those same bytes.
This is host coordination, not a request for Olivier to approve the same plan
again. If the index changes, report which binding changed and obtain the final
bytes directly from Monolith.

## 2. Perform the coordinated cutover

1. Stop the identified legacy shared-database writers, manager/watchers and
   offline replay. Preserve their configuration and queued work. Disable/mask
   obsolete singleton launch paths so reboot cannot resume shared-source writes.
   Confirm this to Monolith before its final source snapshot.
2. Receive and authenticate the **final-delta** Trust bundle. The earlier online
   rehearsal bundle is not final cutover data. Preserve all destination-only
   accepted records and runtime authority.
3. Run the packaged journaled `upgrade.py` for the existing named Trust instance,
   supplying the exact prior/candidate authenticated preparations, current
   configuration digest and observed schema. It stops only that unit and
   preserves the effective sandbox. Verify rc.2 health and all 61 migrations.
4. Stop Trust again for its data reconciliation. Use `memory-area-import
   reconcile-portable` with the independently authenticated bundle hash and a
   protected loopback/Unix target descriptor bound to the existing instance.
   Generate the preview against this stopped target, inspect conflicts and bind
   the execution to that exact preview. Identical records deduplicate; compatible
   history appends. Record explicit `keep_destination` resolutions for differing
   non-version state when appropriate. Never overwrite conflicting immutable
   memory versions or silently discard destination-only records.
5. Apply the same operation ID once, retain its durable receipt, then demonstrate
   harmless exact replay. Any drift requires a new inspected preview; never edit
   hashes merely to silence a mismatch. Retain archive references as references,
   not invented published evidence URLs.
6. Install normal CLI/TUI, packaged operators, Web assets and owned skills from
   this exact release bundle. Configure one operator-owned area-routing policy:
   CA-local work and the default route use Trust, with the existing Trust UUID
   and client profile. Do not map remote Workshop/Homelab/General writes into
   Trust. Those areas remain explicit gateway expertise destinations.
7. Start Trust, verify its own database, then start only area-bound watchers and
   the area-aware manager. Leave old unscoped outboxes preserved until their
   ownership is resolved. Restart resident Memory clients without restarting
   unrelated desktop applications or security services.

## 3. Finish scoped expertise and agent access

Retain company space `46511664-ac1a-4a68-b28e-f6ee13036e32`. Preserve the current
Workshop and Homelab requesters. General's new identity is
`ab756e53-1f26-466e-9eee-02bcf9aa89a7`; require its observed current membership
and explicit collection permission, not membership alone.

Use the delivery index's exact reviewed output-destination IDs and policy
revision. Prepare a new immutable revision of the small existing Trust
collection after reviewing every disclosed field, source version and digest.
Allow only the designated requesters and explicitly approved OpenAI destinations
alongside local-human access. Do not treat old `local_human` consent as model
consent, and do not enlarge the knowledge selection just because more Trust
records were reconciled.

Provision dedicated scoped local identities/tokens for the approved MCP callers,
bind their **principal ID and token ID** to the onward destination, and grant
only the required Trust project permissions/capabilities. Keep secret token
values in protected local configuration. Never bind the broad administrator's
token to an assistant or let callers authorize themselves by naming a provider.
Remote answers remain deterministic, cited and explicitly requested; ordinary
queries remain local. Test both an approved caller and an unbound caller.

## 4. Prove the result and report once

Verify local search, a clearly labelled operational capture, its history and
citations. Check the default CLI/TUI destination and a watcher-produced event.
Verify restart/catch-up and a cited exchange with Workshop; coordinate the
reverse question through Notion until the supported gateway question itself
works. Answer attribution must retain author, answering instance, exact version
and collection revision. Verify unauthorized requester/output policy denial.

Replace only the CA result block in RESULT INBOX and fetch it back. Report:

- Instruction/delivery ID, UTC observation time and status
  (`DONE`, `READY_FOR_CUTOVER` or a specific failure).
- Frozen source revision; release manifest/signature verification; installed
  executable and Web/operator/skill hashes; native architecture/ABI result.
- Trust instance, sanitized database identity/schema, service endpoints,
  effective sandbox preservation and default CLI/TUI route.
- Identified legacy writers and evidence they are stopped/disabled; manager,
  watcher and resident-client routing/restart results.
- Backup/restore receipt hashes; final Trust bundle hash; reconciliation
  operation/preview/receipt IDs; inserted, deduplicated and retained counts;
  conflict resolutions; CA-only/offline pending items.
- Federation sequence/receipt tip, capability/collection revision, allowlists
  and output destinations; cited question IDs and allow/deny test outcomes.
- Protected services unchanged, paid model calls zero, remaining limitations
  and the one next action if something cannot be completed.

Do not paste private content, prompts, source paths, credentials, provider
responses or unrestricted logs. Metadata and evidence hashes are sufficient.

## Recovery

Retain the old executable and database backups, but never restore an old database
as authority after new accepted writes/publications. The named-instance upgrade
journal permits rollback only before its durable startup boundary. After that,
recover forward using its `finish` operation or a corrected compatible release.
For interrupted reconciliation, inspect/reuse the same operation and immutable
receipt; do not start a fresh destructive import. Report uncertain state exactly.
