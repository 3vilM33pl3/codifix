# Offline knowledge and synchronization

Once a reviewed project has been provisioned, you can search its downloaded owner
knowledge without contacting the company. Check freshness and source versions
before relying on a copy. Queued local work is not owner acceptance, and a cached
grant is not evidence of current remote authority.

This guide describes current source on `main`. The published `3.0.0-rc.1` package
contains project-sync commands; its standard package does not include the combined
native runtime bundle. Native provisioning is a
macOS candidate with remaining acceptance checks; start with
[team setup](https://codifix.dev/docs/team-setup) and the
[Mac candidate guide](https://codifix.dev/docs/mac-onboarding). Full/embedded
PostgreSQL packages include `project-sync`; lean binaries report it unavailable.

## Inspect the copy you have

In the Web UI, **More → Offline** shows downloaded owner knowledge, citations,
versions, freshness and upload status. The Web **Memories** local count is not
the number of downloaded owner versions. The updated TUI **Memories** combines
local and synchronized records while marking owner copies read-only; **Offline**
shows detailed subscriptions and cache history.

With the private profile emitted by the reviewed native operator:

```bash
codifix projects sync-status --profile /absolute/private/profiles/example-app.json
codifix projects offline-search --profile /absolute/private/profiles/example-app.json \
  --question "What is the approved procedure?" --limit 8
```

These commands inspect existing projections without creating subscriptions or
contacting the company. Status reports observation age and explicitly unknown
current remote authority. Search returns read-only source/shared versions,
original authors, evidence, locators and digests. It does not authorize forwarding
the results to a model. Uninitialized or stale caches do not appear fresh.

## Refresh an approved subscription

After setup and explicit offline-copy consent, synchronize using the same
protected profile:

```bash
codifix projects sync --profile /absolute/private/profiles/example-app.json
codifix projects sync --profile /absolute/private/profiles/example-app.json --watch
```

`--watch` syncs immediately and every five minutes, retries transient outages and
stops on Ctrl-C without losing committed progress. The native background worker
also checks queued work promptly and refreshes downloads every five minutes
while online. Failed or revoked access is visible; previously delivered bytes
are not falsely described as recalled.

Profiles are protected, owner-only JSON files in owner-only directories. They
contain pinned company/device/project/area/owner/catalog identities, a public
receipt-key schedule and a separately generated local loopback PostgreSQL
connection. Device credentials remain in Keychain. Do not handcraft a profile
against an existing server database. This path does not migrate a database or
fall back to a remote connection. Changed device, manifest review or receipt keys
require explicit profile renewal.

Each page binds its audience, scope, cursor and nonce and verifies its signature
and record receipts before committing the separate projection and cursor. The
company uses its dedicated owner-delivery credentials; it does not forward the
laptop bearer token. Filtered pages are not complete repository proofs. No
whole-space stream, raw capture, session body or unreviewed dependency is copied.

## Publication copies and curated project copies

The existing publication-only mode remains read-only. The native candidate's
explicit `--curated-sync` mode adds downloads of curated histories, canonical
state and asserted same-project relations, including knowledge not published to
other federation participants. The heuristic graph remains owner-local; private
source paths, raw captures and session bodies are excluded. Unavailable source
references remain marked.

Curated contributions need separate browser approval for each exact project and
device, within current owner permissions. Existing reader devices do not acquire
write access automatically. Curated additions queue transactionally and receive
owner receipts. Existing owner-stream changes become correction proposals;
conflicting versions are visible rather than overwritten. Downloads stay in a
separate read-only projection and never enter the upload trigger. Git commit,
merge and push remain your normal Git workflow. Setup enables no paid curation.

## Review owner history and propose a correction

Use **View owner history** in Web query/Offline views, or `h` and then `n` in TUI
synchronized knowledge. CLI history uses the exact canonical and subscription IDs:

```bash
codifix --area workshop history 33333333-3333-4333-8333-333333333333 \
  --subscription 44444444-4444-4444-8444-444444444444 --json
```

Use `--after-version N --limit 16` for later pages. These are owner copies; private
source links are not followed. Inspect the downloaded version and digest before
using the correction form or preparing a local request:

```bash
codifix --area workshop projects propose-correction \
  --subscription 44444444-4444-4444-8444-444444444444 --request correction.json
codifix --area workshop projects propose-correction \
  --subscription 44444444-4444-4444-8444-444444444444 --request correction.json --confirm
```

The first command previews. The request contains a stable `operation_id`,
`source_memory_id`, bare SHA-256 `expected_digest`, and `content` with
`canonical_text`, `summary`, `memory_type`, `importance` (1–5) and `confidence`
(0–1). Reuse the exact file and operation ID after interruption. Confirmation
queues upload for owner review; it does not approve or overwrite remote content.
A changed or withdrawn source requires fresh review. No model is invoked.

`sync-status` includes queued, accepted, proposed, conflict and blocked counts.
Mutable-state, relation and deletion uploads are not an owner-write protocol.
Assistant access is a separate consent path: see
[team setup](https://codifix.dev/docs/team-setup#approve-assistant-access-separately).
