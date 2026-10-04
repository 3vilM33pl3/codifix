# Code Map

Use this map to find the right PR boundary before opening the large files.

## Crates

- `crates/mem-platform-files`: narrow protected filesystem primitives shared by
  platform journals and offline/federation operators. Native Windows ACL,
  handle/identity and atomic-replacement checks; no configuration, SQL, network,
  process discovery or service-management dependency. Re-exported by `mem-platform`.

- `crates/mem-record`: shared request/response and domain record types (pure serde, no I/O).
- `crates/mem-area`: offline, hash-bound four-area inventory, review planner and strict payload materializer. No database access or implicit project-name classification; unknown tables and unresolved ownership block migration.
- `crates/mem-area-store`: trusted fresh-target PostgreSQL import of opaque reviewed partitions; atomic source rows and immutable reconciliation/provenance receipts. No database discovery, migrations, credentials, live cutover or operator CLI.
- `crates/mem-area-ops`: protected `memory-area-import` operator for explicit local targets and exact artifact bindings. `schema-plan/initialize-schema` separately prepare only a reviewed fresh, empty database; `inspect/prepare/import` never migrate. No database discovery, upgrades, credential provisioning or service activation.
- `crates/mem-supervisor`: pure supervisor policy, action, budget and fencing contracts. No provider/process/runtime access; adapters must establish durable admission and verified effects. See the [four-area execution ledger](../../reviews/four-area-network/README.md).
- `crates/mem-supervisor-store`: opt-in supervisor policies, fenced actions, immutable instance/project/model-input binding, physical attempts, daily accounting and commit-ordered metadata journal. Private typed proposals use a separate exact-project store; no service authorization or scheduler.
- `crates/mem-supervisor-runtime`: bounded text-only Sol requests, exact token counting, durable actual usage and one-shot provider sends admitted by the supervisor store. Exact-input Pi admission and a one-work authenticated router are available; no deployed Pi worker or ordinary query context. Source/disclosure consent remains a service-boundary obligation.
- `crates/mem-supervisor-pi`: pinned, credential-free Linux process/control probe. Production provider gate stays closed; independently sandboxed real AMD64 control tests use only an invalid test token. Not a sandbox, installer or agent service.
- `crates/mem-store`: transaction-aware local events, durable jobs, execution/effect journals and operator repair; no network/configuration/service dependency. Connected workers are in `mem-service/src/background.rs`, clients in Web/CLI/TUI Jobs; see the [R1 inventory](../../reviews/federation-first/r1-coverage.md).
- `crates/mem-config`: configuration loading, discovery, and validation helpers.
- `crates/mem-cli`: the `memory` binary, CLI command routing, TUI, setup wizard, eval runner orchestration, and local service tooling.
- `crates/mem-service`: HTTP service runtime, routes, repository reads, migrations entrypoint, activity persistence, and service-mounted MCP.
- `crates/mem-search`: query retrieval, lexical/semantic/graph merging, answer synthesis, citations, and query diagnostics.
- `crates/mem-curate`: raw capture curation, memory replacement proposals, curation policies, and memory write shaping.
- `crates/mem-graph`: repository index graph extraction, symbol/reference storage, and graph status.
- `crates/mem-watch`: watcher daemon and automation capture logic.
- `crates/mem-mcp`: read-only MCP protocol adapter over the service HTTP API.
- `crates/mem-eval`: eval suite schemas, scoring, comparisons, gates, and report rendering.
- `crates/mem-test-support`: pgvector-backed integration test harness.
- `crates/mem-federation`: transport-neutral federation records, Memory projection, legacy replica reducer, and the [origin-owned publication domain](federation-publications.md) with shared offline conformance traces. No runtime or persistence implementation.
- `crates/mem-federation-store`: [PostgreSQL federation durability](federation-durability.md): reviewed stream subscriptions and transactional source capture, ordered gateway acceptance, leased publication, receiver inbox/projection/cursor and verified-prefix resync. Trusted in-process policy inputs, no network dependencies.
- `crates/mem-federation-runtime`: dedicated gateway executable, verified PDS publication, signed audit receipts/pages, receiver key lifecycle, scoped untrusted context and default-deny model egress. `mem-service/src/federation.rs` owns only instance worker lifecycle and resolves current local permissions. See [security/retention](federation-security.md); E user workflows and deployment release gates remain separate.
- `crates/mem-atproto`: pure ATProto/Atrium encoding, unpublished legacy and schema-v2 publication Lexicons, generated XRPC requests, syntax fixtures and MST checks over `mem-federation`.
- `crates/mem-atproto-verify`: bounded full-CAR/commit/MST inclusion verification under an enrolled DID/key. Separate pinned Atrium crypto adapter, no network/discovery/database; the PDS runtime requires verification before acknowledgement.

## Formal

- `formal/memory-federation`: Lean 4.33.1 reducer proofs and executable shared-trace conformance, kept separate from the Rust runtime crates.

F1 is an opt-in, explicitly internal-use increment. Expertise contracts and
v3 advertisements are in `mem-federation/src/expertise.rs`; origin-only reviews
and the private gateway mailbox are separate modules in `mem-federation-store`.
`mem-federation-runtime/src/expertise_transport.rs` provides signed private RPC,
and `expertise_answer.rs` accepts only an already eligible collection subset.
Pure lexical/snippet helpers shared with existing local retrieval live in
`mem-record/src/text.rs`. No gateway dependency on `mem-search` was introduced.
Gateway configuration requires an explicit compatible `expertise_fleet`; default
configuration keeps F1 disabled. See the
[F1 execution status](../../reviews/federation-first/f1-execution.md) for evidence
and the [internal-use runbook](../../user/f1-internal-expertise.md). Fresh per-read
origin authorization lives in `expertise_delivery.rs`; its optional service
validator and bounded deterministic answering loops are in
`mem-service/src/federation/expertise.rs`. Immutable private results are in
`mem-federation-store/src/expertise_results.rs`. Requester-private exact
question consent and metadata receipts live in
`mem-federation-store/src/expertise_requests.rs`; opt-in browser prepare/submit
handlers, capability browsing and original-requester result reads are in
`mem-service/src/federation/workflow/requester.rs`. Browser collection draft,
activation and withdrawal handlers are in `workflow/collections.rs`; the preview
uses a caller-owned transaction to retain current authentication locks.
Local collection listing/detail uses `mem-federation-store/src/expertise_catalog.rs`;
these are stored-review inspection, not answer authorization.
`web/src/features/federation/ExpertisePanel.tsx` implements
explicit discovery, exact-question confirmation and manual result reads; collection
management uses `CollectionsPanel.tsx` for browsing, exact review and withdrawal.
Service-grant controls use the same collection handler and worker identity policy.
CLI/TUI live under `mem-cli/src/commands/federation`, including `terminal.rs`;
dedicated MCP tools use `mem-mcp`. Caller/output bindings are operator-owned.
`mem-service/src/federation/expertise/model.rs` is the isolated tool-free synthesis
path, using configured LLM settings without the ordinary query prompt or content
audit. The real-client, independent-database fake-model/game rehearsal is
`workflow/tests/f1_instances.rs`, also exercised against the pinned PDS gate.

## Frontends

- `crates/mem-cli/src/tui.rs`: TUI orchestration and tab rendering. Pure helpers are being split under `crates/mem-cli/src/tui/`.
- `web/src/App.tsx`: web UI shell, project selection, polling, and feature composition.
- `web/src/features/`: extracted browser feature tabs.
- `web/src/api.ts`: browser API boundary. Keep fetch details here instead of scattering endpoint strings.
- `web/src/features/access/`: Authentik identity, service-token, and project-membership administration.

## Common Change Paths

- Add a CLI command: update `crates/mem-cli/src/main.rs` or `crates/mem-cli/src/commands/`, add help metadata, add a `docs/user/cli/*.md` page, and extend command-help tests.
- Add a service endpoint: add shared types in `mem-record`, route/service logic in `mem-service`, tests in the touched crate, and client wiring where needed.
- Change authentication: update shared contracts in `mem-api`, central policy and stores under `mem-service/src/auth/`, the OpenAPI spec, and transport-specific credential propagation. Never authorize independently in a leaf handler.
- Change query ranking: update `mem-search`, preserve diagnostics, and add tests for result ordering or explanation fields.
- Change memory curation: update `mem-curate`, replacement proposal behavior, memory type docs, and database tests when persistence changes.
- Change the TUI: keep tab behavior localized; extract pure rendering helpers before moving state.
- Change the web UI: extract feature components under `web/src/features/<feature>/`, add a component smoke test, then build.
- Change eval behavior: update `mem-eval` for schema/scoring and `mem-cli` for execution. Use `--allow-shell` for real shell suites and keep production query code separate from research adapters.
- Add a migration: create a new numbered file in `migrations/`; never edit a migration that may already have been applied.

## Validation

Start with the narrowest useful command:

```bash
cargo test -p mem-cli --all-targets --locked
cargo test -p mem-service --all-targets --locked
cargo test -p mem-eval --all-targets --locked
npm --prefix web run test
npm --prefix web run build
```

Before release or broad refactors:

```bash
cargo fmt --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For DB-sensitive work, also run the pgvector-backed tests described in
[CONTRIBUTING.md](../../../CONTRIBUTING.md#database-tests).
