# `memory eval capture-quality` (experimental)

Evaluate candidate memory types and lasting value without writing memories or
changing capture or curation behavior. The command reads a local JSON suite
and writes a new local results directory. It does not require the Memory Layer
service or a database.

## Start offline

Run the 40-case synthetic pilot with the existing rule classifier:

```bash
memory eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --out target/memory-evals/capture-quality/rules-001
```

`--backend rules` is the default. It calls the production candidate extractor
locally; it does not use a provider. It has no lasting-value prediction.

Validate a paired run without reading a provider key or making network calls:

```bash
memory eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --backend both --dry-run \
  --out target/memory-evals/capture-quality/preview-001
```

Each `--out` must name a new directory. Retain interrupted or failed evidence
and choose a different directory for a subsequent attempt.

When working from source, use `cargo run --locked --bin memory --` before
`eval capture-quality` in these examples. Avoid a large release build merely
to run this experiment.

## Run Jev explicitly

Inspect the entire suite before choosing `--backend jev` or `--backend both`.
These modes send candidate text, task title, user prompt, agent summary, and
any explicit type to TypeSafe's hosted API. They use `TYPESAFE_API_KEY` from
the process environment. Keep the key out of suite files, commands, and Git.

Once the key is provisioned and this particular live run is approved:

```bash
memory eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --backend both \
  --out target/memory-evals/capture-quality/jev-001
```

The adapter pins `jev-1.14.0` and sends one request per case to
`https://api.typesafe.ai/v1/systemone`. The endpoint cannot be supplied by a
fixture. Type and lasting value are separate questions in the same request.
Expected labels, rationales, categories, and source metadata are excluded.

Missing credentials fail before transmission. Per-case failures remain in the
results instead of disappearing from the denominator. Raw provider error
bodies and keys are not included in reports. Model predictions are advisory;
they cannot authorize a write, supersession, capture rejection, or disclosure.

## Replay saved responses

```bash
memory eval capture-quality \
  --suite evals/suites/capture-quality-pilot-v1/suite.json \
  --backend both \
  --replay target/memory-evals/capture-quality/jev-001/run.json \
  --out target/memory-evals/capture-quality/replay-001
```

Replay does not need credentials or a network connection. It revalidates saved
responses against the fixture and question fingerprints before scoring them.
Changing labels, inputs, or the question rubric requires a new experiment.
Replay is marked as replay evidence; it does not represent another paid call
or a new latency measurement. `--replay` requires `jev` or `both` and cannot
be combined with `--dry-run`.

## Read the results

`run.json` retains machine-readable observations and response evidence. The
Markdown report summarizes inferred and explicit type accuracy, coverage,
abstentions, errors, and confusion patterns. Lasting-value metrics are scored
against separate labels; the rule baseline has no equivalent score.

Treat the 0.5 lasting-value threshold as a descriptive evaluation convention,
not an approval threshold or a calibrated production policy. An uncertain
type is an abstention, not an eighteenth canonical memory type. Explicit-type
results test handling of operator input and cannot establish better inference.

The reports retain model and input/question fingerprints, latency, token
usage where reported, and estimated input charges at the published
$0.042 per million input tokens. Unknown usage remains unknown; replay and
dry-run artifacts must not be counted as new spend.

The suite is agent-reviewed and entirely synthetic. Its deliberately selected
edge cases are not a representative sample of real captures. The current
heuristic cannot infer `incident`, `plan`, `insight`, or abstain; report that
capability difference when interpreting the paired result. These observations
do not establish factual correctness, retrieval improvement, or readiness for
automatic curation.

See [the pilot suite and label rationale](../../../evals/suites/capture-quality-pilot-v1/README.md),
[TypeSafe's API contract](https://docs.typesafe.ai/api), and
[the pinned model's pricing](https://docs.typesafe.ai/models).
