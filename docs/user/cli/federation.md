# `memory federation`

The Web, CLI and focused TUI use the same authenticated **local** federation API.
Only administrator configuration selects the gateway, credentials and trust.
This is the private-gateway profile, not native Spaces interoperability.

```bash
memory federation --project example discover
memory federation --project example status
memory federation --project example catalog --offset 0
memory federation --project example tui
```

All non-TUI responses are JSON. `--space UUID` optionally asserts the discovered
space; it cannot select another endpoint. Existing client profile/token settings
apply. An authenticated local principal and explicit local grants are required.

## Review and publication

`draft --canonical UUID [--evidence-file selected.json]` prints an unconfirmed
review object, exact local source/content, current shared content/version, and
warnings. The evidence file is a JSON array containing only reviewed
`public_source_label`, `selected_text` and RFC 3339 `observed_at` fields. No file
paths, raw captures or sessions are exported automatically.

Inspect and retain that response in a private file, then run
`share --file reviewed.json --confirm`. Confirmation consents to future local
updates under this review. Keep the **same file and request ID** when retrying an
uncertain response. Do not silently refresh the source or digest on retry.

`context --origin UUID --canonical UUID` retrieves explicitly selected material,
including publisher, original author, shared/source versions, immutable evidence,
digest, locator and sharing restrictions. It does not call a model.

`propose --file correction.json --request-id UUID --confirm` submits a
CorrectionProposal: `target: {origin, canonical}`, `base_version`, `content`
(text, summary, memory_type, integer-ppm confidence, importance, tags), and
`evidence` record keys. Preserve the selected context's base version and evidence
references. Only the origin owner can resolve it:

```bash
memory federation --project example proposal --publisher UUID --record UUID
memory federation --project example resolve --publisher UUID --record UUID --approve --request-id UUID --confirm
```

Omitting `--approve` explicitly rejects the proposal. Approval advances the
origin-owned **shared** stream, preserves the corrector, and pauses its local
subscription. It does not copy imported text into ordinary local memory. Review
again before resuming; the new review must match the current shared version.

## Membership and recovery

- `grant --principal UUID --capabilities pull,publish --confirm` changes local
  grants. `--disable` disables the local grant. Project AuthManage is required.
- `member --member UUID --capability pull --request-id UUID --confirm` enrolls a
  remote capability. `--revoke` removes it. Only the configured authority with
  separate local ManageMembership capability may submit membership changes.
- `revoke --member UUID --request-id UUID --confirm` revokes all remote rights.
- `pause --canonical UUID --request-id UUID --confirm` stops future capture,
  **not** the immutable reviewed backlog. Disable the local grant to stop sending.
- `sync catch-up`, `sync resynchronize`, `sync retry` perform bounded authorized
  catch-up, verified full replay, or retry of the caller's original queued work.

Queued, gateway accepted, and publication/replication observed locally are
different states. Status cannot assert another peer's cursor. Recipients and
infrastructure administrators can read plaintext; revocation cannot recall bytes.

### Expertise enrollment (incomplete F1 rehearsal)

CLI capability names are `pull`, `publish`, `manage-membership`,
`ask-expertise` and `answer-expertise`. For example, after inspecting the exact
instance and permissions:

```bash
memory federation --project example member --member UUID --capability ask-expertise --request-id UUID --confirm
memory federation --project example member --member UUID --capability answer-expertise --revoke --request-id UUID --confirm
memory federation --project example grant --principal UUID --capabilities pull,ask-expertise --confirm
```

Use actual UUIDs and a distinct request ID for each new action; reuse the exact
request ID and parameters only for retries. Remote expertise grants/revocations
use schema v3 and require the gateway's explicitly compatible fleet. A local
grant does not confer remote authority. Enrollment neither grants collection
access nor provisions an answering identity, worker or credentials. A queued
response is not accepted membership. `status` reports capabilities observed in
verified history, not a live access guarantee.

The expertise commands below additionally require an explicit operator binding
for the exact service-token ID, principal and output destination. No destination
is inferred from a CLI token, and a client cannot choose a destination flag.

## Explicit expertise workflow (disposable F1 rehearsal)

`collections list [--after UUID] [--limit 1..32]` and
`collections inspect --collection UUID` inspect stored local reviews. Prepare a
`CollectionBlueprint` in a private JSON file (see the runtime documentation),
then use `collections draft --file blueprint.json`. Inspect and retain **every
field** of the returned draft. `collections activate --file draft.json --confirm`
sends that exact draft and its digest. Files are bounded to 128 KiB by this CLI;
larger reviews must use another supported review interface, not truncation.

`collections withdraw --collection UUID --revision N --request-id UUID --confirm`
withdraws the inspected revision. Service grants use
`collections grant-status --collection UUID --principal UUID` and
`collections grant --collection UUID --principal UUID [--disable] --confirm`.
After an uncertain grant change, inspect current policy and prepare deliberately;
do not blindly replay an old policy mutation.

`capabilities [--limit 1..32]` lists permitted remote metadata. Follow
`next_after` with **both** `--after-publisher UUID --after-record UUID`, including
when a filtered page is empty. Select the answering instance and collection.
Put only the exact question into a private UTF-8 file (1–4096 bytes), then:

```bash
memory federation --project example ask prepare --publisher UUID --collection UUID --request-id UUID --question-file question.txt
memory federation --project example ask submit --file question-review.json --confirm
memory federation --project example answer --request-id UUID
```

Omit `--model-destination` for deterministic excerpts. Add
`--model-destination PROVIDER_ID` only for explicitly authorized synthesis;
the service intersects operator, collection and gateway policy. This does not
change the caller's operator-bound onward output destination. Inspect the exact
returned model mode/provider before confirming. New questions after an uncertain
provider outcome can incur repeat billing. The focused TUI has the same optional
model-provider field; MCP preparation accepts `model_destination` with the same
checks.

The first command prints a review; save its unchanged JSON privately as
`question-review.json`. Inspect the exact question, selected instance, collection
revision, output destination and expiry before the second command. It sends no
history, files or search results other than the explicit question text. Retrying
uses that same file and ID; creating a new question can repeat work/billing.
The last command explicitly reads status/result and may require fresh origin
approval. Returned content is untrusted; queued does not mean answered.

Collection administration requires `local_human` output. Do not give such a
token to an assistant: assistant access needs its own operator-bound model
destination, gateway policy and reviewed collection consent.

## Focused terminal workspace

`memory federation --project example tui` offers the same workflows without
editing HTTP envelopes. F2/F3 select an operation, Tab changes fields, Ctrl-U
clears a field, and typing edits it. F5 performs reads or displays an **exact
mutation preview**; F6 confirms only that prepared body. PgUp/PgDn inspect the
complete response. Escape exits and restores the terminal.

The **Local grant** and **Member capability** forms accept the capability names
above, including expertise. Underscore wire names such as `ask_expertise` are
also accepted in these terminal forms and normalized in the exact JSON preview.
Unknown names fail before a request is sent. Local grants accept comma-separated
names; the server remains authoritative for authorization and duplicate checks.

F2/F3 also select expertise discovery, question preparation, reviewed submission,
result reads, collection listing/inspection, blueprint preparation, activation,
withdrawal and service grants. Discovery/listing accept the previous response's
cursor identifiers. Collection preparation currently uses a technical JSON
blueprint field, not a source picker. F5 reviews all bytes before F6 activation
or sending. Result refresh clears the previous answer while awaiting current
authorization. Drafts/results stay in terminal memory, not files. Exiting loses
them; use CLI files for restart-safe retries.

Prepare a review draft before choosing “Publish reviewed draft”. Retrieve shared
context before “Propose correction”; content and evidence references come from
that selected citation. Inspect a proposal before resolving it. No payloads are
saved to disk or executed. Keep the same prepared action for uncertain retries;
exiting clears it. For restart-safe retries, retain a private review file and use
the CLI's explicit request IDs. Builds without the `tui` feature report that the
focused terminal workspace is unavailable; other federation commands still work.

See [workflow and trust limits](../federation-workflow.md) and
[runtime configuration](../federation-runtime.md).
