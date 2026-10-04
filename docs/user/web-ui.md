# Web UI

**More → jobs** provides filtered, bounded background-work inspection, dependency
and attempt history, queue statistics and explicit retry confirmation. See
[Background jobs](cli/jobs.md). Enrichment is separate from curation success.

Activity recovery polls bounded sequence-based pages every five seconds, even
when live notifications are unavailable. The WebSocket reconnects automatically.
Sequence numbers are global, so gaps within one project do not imply data loss;
the server explicitly reports history removed by retention. Recovery does not
reconstruct deleted history, and late responses from a previously selected
project are discarded.

The **More → federation** workspace exposes the configured private space, local
grants, authority enrollment, reviewed publication, shared citations, correction
proposals, approval/rejection, subscriptions and sync controls. Mutations display
their exact request and require confirmation. Drafts are cleared on project
changes; imported text is escaped and never forwarded to a model from this view.
See [Federation workflow](federation-workflow.md) for consent and offline-copy limits.

On a single-user loopback installation, opening Federation establishes the local
owner's revocable browser session before discovery, including read-only requests.
It does not require a prior mutation in another tab. Existing or rejected sessions
are not replaced automatically after an authorization failure. Multi-user
installations continue to use their normal OIDC sign-in; current permissions,
explicit federation grants and CSRF protection still apply.

### Private expertise questions (disposable F1 rehearsal)

The **Watched agent exchanges** panel lists the latest twenty project-scoped
Codex/Claude questions by request ID, destination, time and observed transport
state. It deliberately omits question and answer bodies. Refresh it to see
queued/published progress; `unknown` means status could not be verified. The
existing **Ask another instance** panel remains the separate, human-reviewed
question workflow.

Authority administrators can select **Ask expertise** or **Answer expertise** in
**Local grants and authority enrollment → Remote capability**, then review and
confirm enrollment or capability revocation. These actions require schema-v3
gateway/participant compatibility; a queued action is not accepted enrollment.
Local capabilities use the separate `ask_expertise` and `answer_expertise` names.
Neither enrollment nor a local grant creates collection consent, credentials or
an answering worker. **Refresh local status** includes both capabilities when
observed in verified history, not as a live authorization guarantee.

In **More → federation → Ask another instance**, choose **Discover expertise**,
select a permitted collection, enter a question and choose **Prepare remote
question**. Review the exact destination, collection revision, question, identity,
deadline and digest before checking the consent box and sending. Preparation does
not send a question. No history, files or local search results are attached;
ordinary queries remain local. The operator must enable the experimental browser
policy and explicit grants described in [Federation runtime](federation-runtime.md).

Only deterministic excerpts and `local_human` output are supported here. Remote
content is displayed as untrusted text with its citation/attribution data, never
as executable HTML. Model forwarding is not authorized. Queued status is not an
answer or evidence of replication.

Use **Fetch expertise result** to explicitly request current status or an answer.
Completed answer reads require fresh origin approval. The previous displayed
answer is cleared before a refresh, including a denied/offline refresh. Save the
request ID if you need to return later: this panel does not persist questions or
answers in browser storage. Switching projects or local identity clears the panel
and aborts its requests.

After an uncertain submission, **Retry exact question** reuses the same reviewed
bytes and request ID. Closing the review does not cancel delivery. A new question
is a new request and may repeat work; expired reviews require fresh preparation.
Read-only mode and missing export permission disable the panel; the server still
enforces all current permissions. CLI/TUI/MCP
parity and the complete live rehearsal remain unfinished.

### Expertise collection management

The Federation workspace also has **Approved expertise collections**. Choose
**Browse collections**, then inspect an existing collection to see the stored
review, including withdrawn consent. This is not a current eligibility or
publication check. Pagination uses collection UUIDs, not gateway ordering.

To prepare a collection, provide its reviewed name, description, topics, allowed
requesting instance UUIDs and canonical memory UUIDs. Leave the collection UUID
blank for a new collection, or supply an existing UUID for a new revision. The
server selects exact current versions for review. Omitted entries and evidence
are not carried forward into a new revision.

No evidence is attached by default. The advanced evidence field accepts an object
mapping selected canonical UUIDs to `EvidenceSelection` arrays, each containing
`snapshot` and `snapshot_digest`. The backend checks evidence validity; the UI
rejects keys for unselected memories. This technical editor is not an automatic
source picker. Raw captures, paths and files are never attached automatically.

**Prepare collection review** returns all exact snapshot and advertisement bytes.
Read every field before checking the consent box and confirming activation.
Activation grants standing consent for deterministic local-human answers; it does
not enroll an instance or grant the answering service identity access. To withdraw,
inspect the stored collection and choose **Review collection withdrawal**; confirm
the collection UUID and revision. Withdrawal cannot recall previously delivered
or already authorized in-flight bytes.

The UI preserves a failed/uncertain action for exact retry and reports successful
submission as queued, not published. Closing a review does not roll back a
committed action. Project/identity/permission changes clear local state and abort
pending requests; review contents are not saved in browser storage. Server-side
permissions remain authoritative. A real-browser HTTP/SQL rehearsal covers
collection management and queued questions; CLI/TUI/MCP parity and the complete
independent-instance answering rehearsal remain open.

After inspecting a collection, administrators can enter an **Answering service
principal UUID**, inspect its stored grant and review a grant or revocation.
This does not create an identity or credentials, start a worker, enroll a peer,
or confer local/remote `AnswerExpertise` capabilities. Enabling requires an active
internal identity with no global/group authority and current read/export access
to the project. Revocation remains possible after the target identity is disabled.

Grant changes update local policy rather than queueing a publication. After an
uncertain response, the confirmation is discarded: inspect current grant state
and deliberately review a new change. Do not replay an old grant over newer
policy. Stored enabled state alone is not current execution authorization.

The web UI is the web companion to the TUI. It is served by `mem-service` and uses the same backend APIs for memories, query, code graph inspection, activities, watchers, embeddings, resume briefings, and curation review.

Open it while the service is running:

```text
http://127.0.0.1:4040/
```

When building from source, run:

```bash
npm --prefix web ci
npm --prefix web run build
cargo run --bin memory -- service run
```

## Tabs

**More → Supervisor** shows this instance's supervisor policy, exact budget
amounts and bounded action history. It requires global `memory_read` and is not
scoped by the project selector. This build explicitly reports execution **not
integrated** for full autonomous agency, even if a policy is enabled. Inspecting
this operational feed never starts work or exposes private bodies. The separate
project-private interaction panel below has its own permissions and explicit
consent; see [Supervisor inspection](cli/supervisor.md).

### Workshop questions and saved proposals

Select the Workshop project and open **More → Supervisor**. Under **Questions
and proposals**, **Load saved proposals** and **Open proposal** display a saved
question, plain-text response and exact source-version/digest citations. Approve
or reject first opens confirmation. A recorded review is permanent and does not
execute a suggestion, install software, edit a memory or permit a model call.
The original fixed-proposal result remains supported.

For a new request, enter an **Exact question** and choose **Prepare question**.
The limit is 4096 UTF-8 bytes. Read the confirmation's exact question, OpenAI
destination, `gpt-5.6-sol`, approved source count, daily ceiling, profile/request
hashes and request UUID. Preparation makes no model call. **Confirm paid
question** permits one bounded answer attempt using only that question and the
approved collection; no conversation history, files, ordinary searches or other
areas are attached. A model answer is untrusted text, not an instruction to run.

**My questions** lists only the authenticated requester's retained request
metadata. Choose a request to inspect status; **Refresh question status** is an
explicit read, not another model call. **Read cited answer** appears when a saved
result is available. Queued does not mean answered. Pending work has a ten-minute
deadline (or an earlier standing-grant expiry), at most eight outstanding
questions per Workshop project and one active answer worker. Existing limits of
20 generations and USD 2.50 per day remain in force; current policy can lower
them.

The eventual saved question/answer follows the existing **project-private**
proposal contract, not requester-only messaging: current project readers may
inspect it while the original requester's disclosure authority remains valid.

After uncertain submission, use **Inspect this request** or **Retry exact
submission**. The same question, hashes and UUID are retained; no automatic
provider retry or replacement UUID is generated. A new question after uncertain
provider work can repeat billing. Save the displayed request UUID before leaving
the panel. Private input/consent lives only in browser memory and clears when the
project or authenticated identity changes; it is not saved in browser storage.

Intake, execution and answer delivery require fresh authorization. This narrow
queued path requires direct database `memory_read`, `loops_run` and
`bundle_export`, not merely group-mapping-only access, and a revocable session
or service token. Revoking/expiring the **original submitting credential** blocks
further execution and answer delivery even after a new login. Source changes,
withdrawal, source-grant expiry and policy revocation also fail closed. Revocation
cannot recall bytes already sent. Browser writes retain normal CSRF/Origin
checks; read-only/evaluation services cannot accept paid work.

This panel requires the separately provisioned interaction profile, approved
source grant and isolated operator `run-next` worker (or the preview's explicitly
activated `serve` wrapper). The app itself has no provider key or Docker socket.
An enabled policy, visible controls or queued request does not prove the worker
is running. These are software/operator requirements, not a claim that paid
question execution is already deployed. This workflow does not enable the other
three areas or cross-area communication. See the
[CLI/TUI and operator instructions](cli/supervisor.md#explicit-private-questions).

The main tabs match the TUI order:

| Tab | What it shows |
| --- | --- |
| Memories | Canonical memories, filters, markdown-style canonical text, embeddings, sources, history, and related memories. |
| Agents | Local Codex/Claude sessions, with the current project sorted first, plus tokens, context pressure, child processes, ports, and rate limits. |
| Query | Cited answers, ranked memory results, lexical/semantic/graph timing, token usage, ranking explanations, and graph connections. |
| Graph | 3D WebGL graph explorer with code neighborhoods, optional memory provenance links, optional memory relationship edges, and typed node/edge detail. |
| Activity | Persisted activity, token/duration metadata, get-up-to-speed briefings, and LLM audit/debug status. |
| Errors | Persisted diagnostics plus browser-session errors with explanations, fix hints, `memory doctor` hints, commands, and raw errors. |
| Project | Project-level counts, memory type/source breakdowns, embedding coverage, automation state, watcher status, and recent activity. |
| Review | Replacement proposals with policy, target/candidate detail, and approve/reject actions. |
| Watchers | Watcher presence, heartbeat state, owner agent/session, restart attempts, and recovery details. |
| Skills | Repo-local Memory skills, version status, upgrade action, file paths, `SKILL.md` content, and repair controls. |
| Embeddings | Configured embedding backends, active search backend, automatic creation, coverage, re-embed, and reindex controls. |
| Resume | Re-entry briefing with checkpoint, current thread, next steps, recent changes, context memories, timeline, warnings, and commits. |
| Automations | Loop-engineering approval queue, automation cards, effective mode, scope, risk, last run, next trigger, daily budget, outputs, and controls. |
| Access | Global-admin view of Authentik users, service tokens, explicit project memberships, and one-time token creation. |

The web-only `Bundles` tool remains under the More menu for memory export/import
previews and transfer. `Automations` is a primary browser tab and also has a
read-oriented TUI tab.

In multi-user mode, the top bar shows the Authentik identity. The browser keeps
the opaque session in an HttpOnly cookie and sends a separate CSRF value on
writes; it never downloads a service token. The Access tab appears only for a
global admin. Newly created service-token secrets are shown once and should be
stored in OpenBao or another secret manager before dismissal.

## Automations

The Automations tab is the browser control surface for loop engineering. It reads
registered loop definitions from `/v1/loops`, resolves effective settings for the
current project/repo root, and displays the latest loop run for each automation.

The top approval queue is the human-in-the-loop review surface for risky loop
actions and memory proposal changes. Each pending request shows:

- the proposed action JSON, action type, risk reason, requester, reviewer, loop,
  and linked run;
- linked memory proposal candidate/evidence when the proposed action references
  a memory proposal;
- `Approve`, `Reject`, and `Save edit` controls. Save edit validates the edited
  JSON before sending it to the service.

Rejected approvals are recorded in the run trace and block a queued/running
linked run so an agent cannot continue through a rejected gate. Approved,
rejected, and edited decisions also update linked memory proposal status when
the proposed action names a proposal id.

Each automation card shows:

- effective mode and whether it is disabled, blocked, paused, snoozed, or stopped by the global kill switch;
- scope, including whether the active setting is inherited or overridden at project/repo level;
- risk level, next supported trigger, daily budget, outputs, and the most recent run;
- a mode menu for enabling or changing the loop mode.

The detail pane adds policy capabilities, blocked reasons, pause/snooze expiry,
last output, and action buttons:

- `Enable`: writes an explicit project/repo override using the definition default mode.
- `Disable`: turns the automation off for the current scope.
- `Pause 1h`: pauses the automation temporarily.
- `Snooze 1d`: suppresses the automation until the next day.
- `Run now`: records a manual control-plane run. In the first loop-engineering
  slices this is a dry-run/control-plane record; real autonomous execution is
  intentionally separate.
- `Global stop`: toggles the shared kill switch for all loop automations.

Use repo root carefully. When a repo root is resolved, setting changes are stored
as repo-scoped overrides. Without a repo root they are stored as project-scoped
overrides.

Use `Load run` on an automation with a last run to inspect the run ledger. The
detail view shows trigger source/event/trust, run reason, version, effective
settings, policy gates, context pack, cost, output, trace records, linked memory
proposals, and linked approval requests. The context pack section shows included
memory count, estimated tokens, repo instruction references, exclusions, stale or
contradictory memory flags, warnings, and the diff from the previous context
pack trace for that loop/project. Redacted trace payloads stay hidden and failed
or blocked runs surface their diagnostic summary and blocked reasons.

## Skills

The Skills tab is the full inventory view for repo-local Memory Layer skills.
It defaults to the `memory-layer` umbrella skill so the main health signal stays
quiet, but the filter can switch to the full Memory-owned bundle when you need
to inspect focused skills such as query, resume, remember, proposal review, or
plan execution.

The list shows each skill's version, freshness, upgrade action, source path, and
template path. Selecting a row opens the detail pane with the description,
installed path, source/template versions, and rendered `SKILL.md` content so you
can inspect exactly what an agent will read.

Use `Repair skills` when the repo-local bundle is missing, stale, unversioned, or
corrupt. The action uses the same repair path as `memory doctor --fix`: it
downloads the current GitHub skill bundle when available, falls back to the
installed template when offline, backs up replaced files, and only mutates
Memory-owned skill directories.

## Graph Explorer

The Graph tab is a WebGL-only 3D explorer with three independently visible
layers:

- `Code`: parser-backed code graph neighborhoods from the latest completed
  graph extraction run. This layer is on by default.
- `Provenance`: memory-to-source links derived from active memories and their
  source records, including file, symbol, commit, and provenance verification
  status when available. This layer is off by default.
- `Memory relationships`: active memory-to-memory relation edges such as
  `supports`, `supersedes`, `duplicates`, `depends_on`, and `related_to`. This
  layer is off by default.

The browser does not extract or mutate graph data. The code layer reads
`/v1/projects/{project}/graph`; the memory layers read the separate read-only
`/v1/projects/{project}/memory-graph` endpoint and then merge visible layers in
the browser.

Build or refresh the code graph with:

```bash
memory graph extract --project memory
```

The default view is a bounded neighborhood, not an unlimited whole-repository
render. Filters can seed the graph from text search, file path, symbol name, edge
kind, depth, node cap, and edge cap. The service enforces hard caps of depth `2`,
`1000` nodes, and `2000` edges. When results are capped, the tab shows the
truncation reason.

The layer checkboxes sit underneath the 3D scene. Hovering a checkbox or a
visible node/edge brightens that whole layer and dims the others. Toggling the
provenance or relationship layers only changes the browser view; it does not
change backend code graph filters.

Clicking a code node selects it. Shift-clicking a second code node switches the
current browser view to the nodes and edges that connect those two nodes through
any simple path in the loaded code graph. This is a local view mode; normal node
selection, clearing the selection, Back/Forward, or refreshing the graph exits
it. The Shift-click connection view applies only to the Code layer in this
release.

Clicking a memory node shows its memory type, confidence, importance, tags, and
memory id. Clicking a source node shows source type, file path, symbol, commit,
verification status, and source id. Clicking provenance or relationship edges
shows their endpoints and edge-specific metadata.

Query result graph connections and graph extraction activity details include
actions that open the Graph tab with matching file, symbol, edge kind, or run id
filters.

Browsers without WebGL show an unsupported state. There is no SVG or 2D fallback
because the graph view is intended to exercise the same WebGL minimum supported
surface in every client.

## Runtime Status

The status strip shows the same operational components as the TUI bottom bar:

- `Web`: web UI version and restart-required state.
- `Service`: backend version, primary/relay role, and service identity.
- `Manager`: watcher-manager state, mode, tracked sessions, and warnings.
- `Watchers`: active/unhealthy watcher counts.
- `Skills`: compact repo-local skill health for the selected Skills filter.
  Click the component to open the Skills tab for the full inventory and repair
  controls.

If the install or upgrade process wrote a restart marker, the Web component turns into a restart state so the user knows to reload the page or restart the running UI.

## Query Evidence

The Query tab is designed to explain how an answer was produced:

- The answer cites numbered returned memories.
- The timing breakdown separates browser roundtrip, lexical search, semantic search, graph retrieval, rerank/relation work, and answer generation.
- Per-result details show score components such as full-text scores, semantic similarity, relation boost, graph boost, tag/path matches, confidence, importance, and recency.
- Graph connections explain which file or symbol helped retrieve a memory. Answers still cite memories, not raw graph rows.

Use the query history with the up/down arrows in the query box. Restoring a previous query restores both the question and the results that belonged to it.

## Activity And Audit

The Activity tab can generate two kinds of handoff briefing:

- `Deterministic`: cheap, grounded summary from persisted activities and memories.
- `LLM briefing`: synthesized narrative when an LLM backend is configured.

The same tab can toggle LLM audit/debug logging. Enable it briefly when debugging prompts or provider behavior, then disable it again because audit events can include large prompt payloads.

## Help

Press `h` or use the Help button to open contextual help for the active tab. Press `h` again or `Esc` to return.
