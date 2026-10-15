# Jev capture-quality pilot: offline implementation evidence

Date: 15 October 2026. Implementation revision: `ac3097f1` (subsequent
`3a485008` adds a report-layout regression only).

The experimental command is implemented. This record covers the local rules
baseline, request preparation, transport tests, and offline replay. **No live
Jev calls were made, so no Jev quality or cost advantage has been measured.**

## Inputs and provenance

- Suite: [capture-quality-pilot-v1](../../../evals/suites/capture-quality-pilot-v1/README.md).
- Forty entirely synthetic cases, authored by an agent and independently
  inspected by Astra; `label_status = agent_reviewed`, not human adjudication.
- Seventeen clear types, eight keyword collisions, five transient cases, four
  ambiguous cases, three adversarial cases, and three explicit-type cases.
- All 17 canonical memory types represented; 29 positive and 11 negative
  lasting-value labels.
- No private-memory exports, external API calls, or canonical writes.

Suite SHA-256:
`5807a88a28e6d576b6879377e0045c7ab2ded5859d55433de1af036cd9f07f0b`.

Prepared question SHA-256:
`1b2a75d9c4f41cc086e0a6a0c8e542174ddffdba0b32e70a5ebba698811eb489`.

## Local observations

| Backend / track | Correct | Accuracy | Errors | Coverage |
| --- | --- | --- | --- | --- |
| Existing rules / inferred type | 21 / 37 | 56.8% | 0 | 100% |
| Existing rules / explicit type handling | 3 / 3 | 100% | 0 | 100% |
| Jev / dry-run only | Not evaluated | Not evaluated | Not evaluated | Not evaluated |

The paired dry-run artifact contains 40 rules observations plus 40 pending Jev
observations and their exact prepared requests. Pending requests have no Jev
metrics, latency, usage, or spend estimates.

Eight inferred cases require `incident`, `plan`, `insight`, or abstention,
which the current heuristic cannot emit. Several other cases intentionally
contain competing keyword cues. Consequently, 56.8% is an exploratory result
on a deliberately difficult fixture, not an estimate of production accuracy.
Explicit types are operator input and are excluded from inferred accuracy.

## Reproduction and artifacts

From the repository root, with the source-built CLI:

```bash
target/debug/memory --config /nonexistent/jev-pilot-config.toml \
  eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --out target/memory-evals/capture-quality/2026-10-15-rules

target/debug/memory --config /nonexistent/jev-pilot-config.toml \
  eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --backend both --dry-run \
  --out target/memory-evals/capture-quality/2026-10-15-preview
```

Use new output paths when repeating the commands; existing artifacts are
deliberately protected from overwrite. Both commands were run with
`TYPESAFE_API_KEY` removed from the child environment and a nonexistent
service configuration path. A `strace -f -e trace=network` of the preview
showed only a local `AF_UNIX` runtime socket pair: no network connections or
IP sockets. The trace is at `/tmp/memory-jev-dry-run-network.trace` locally.

Local JSON artifacts under `target/memory-evals/capture-quality/`:

- `2026-10-15-rules/run.json`, SHA-256
  `322449236e73bf125d59ed6608c2317f580d622b5e0142df233f2e4c8b4af909`.
- `2026-10-15-preview/run.json`, SHA-256
  `d4b9e34734a84311efcea968d08879699525d4de079da99812fbc4c0652216c4`.
- Each directory also contains `report.md`; these generated artifacts are
  local evidence and are not checked into Git.

## Validation and remaining evidence

Focused tests cover the actual extractor baseline, conflicting explicit input,
fixture validation, labels excluded from requests, malformed responses,
probability validation, HTTP errors, redirects, timeout, secret-safe errors,
saved-response bindings, and offline replay without new latency or spend.
Binary integration tests exercise nonexistent/invalid configuration, missing-key
failure before output or traffic, and a network proxy spy. The default-feature
build and focused tests also pass; production curation source is unchanged.

Verified commands and results:

- `cargo test -p mem-eval --all-targets --locked`: 80 passed.
- `cargo test -p mem-cli capture_quality --locked`: five focused tests passed.
- `cargo test -p mem-cli --test capture_quality --locked`: three binary
  integration tests passed.
- CLI help checks: 13 passed with `--no-default-features`.
- `cargo build -p mem-cli --bin memory --locked`: default-feature build passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Clippy for the touched crates and all targets with `--no-default-features`
  completed successfully; unrelated pre-existing warnings remain.

The next evaluation step requires a reviewed live run using a provisioned
TypeSafe key. Human label adjudication remains outstanding. Only after the
pilot shows useful quality should the questions be frozen and 150 new held-out
cases assembled; this implementation does not enable production integration.

The plan-start helper saved its local checkpoint but could not store the plan
because the dev service at `127.0.0.1:4250` was unavailable. This does not affect
the file-based pilot; service-backed plan verification is a separate outstanding
workflow record.
