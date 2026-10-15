# Capture type and lasting-value pilot

This exploratory suite contains **40 fully synthetic cases**. No case contains
an export from a private project, user account, or Memory Layer database.
Names, operational events, ports, and resource identifiers are invented.

The labels were authored by an agent and independently inspected by a second
agent against the repository's memory-type definitions. `agent_reviewed` is
not a claim of human adjudication or a representative production benchmark.
Each case includes a rationale so an operator can inspect and correct it.

## What the suite measures

- **Type:** which of the 17 `MemoryType` variants describes the candidate's
  primary meaning. Four intentionally ambiguous cases have a null expected
  type: the useful result is abstention, rather than a forced classification.
- **Lasting value:** whether the candidate states project knowledge useful
  beyond the current task. This is an independent label, not a memory type or
  permission to discard a capture. Active tasks and approved plans can be
  valuable execution records while receiving a negative lasting-value label.

The categories are 17 clear types, eight keyword collisions, five transient
facts, four ambiguous notes, three adversarial notes, and three explicitly
typed candidates. All 17 types occur in the clear cases.

The rule baseline calls the existing deterministic candidate extractor with
the same title, prompt, summary, and candidate text supplied to Jev. It does
not attempt to predict lasting value. Explicitly typed candidates are reported
separately: preserving an operator-supplied type is not evidence of improved
type inference.

The final explicit case deliberately supplies an `architecture` label for
documentation prose. Its expected type tests preservation of operator input,
not semantic agreement with that label.

Keyword-collision cases intentionally exercise the heuristic's known boundary:
for example, a troubleshooting lesson does not become documentation merely
because its task title mentions a README. This hand-built suite emphasizes
such boundaries; its error rates do not estimate their frequency in real use.
Eight of the 37 inferred cases require `incident`, `plan`, `insight`, or
abstention, which the current heuristic cannot emit. Keep that capability
difference visible when interpreting aggregate accuracy.

## Run locally

See [the capture-quality command reference](../../../docs/user/cli/capture-quality.md)
for offline, dry-run, live, and replay commands. The suite file is `suite.json`.
The default backend is offline rules. No Memory Layer service is required.

## Review before expanding

Review `expected_type`, `expected_durable`, and `rationale` independently.
Expected labels, rationale, case category, and source metadata must never be
included in Jev's decision state. An explicit type is operator input, not a
hidden expected label, and its results remain a separate group.

The pilot measures type and lasting-value judgments. It does not establish
factual support, completeness, duplication handling, safe canonical writes,
or end-to-end retrieval improvement. A typed result can still be wrong, and
model confidence is not evidence confidence.

If the pilot is promising, freeze the question rubric before collecting 150
new held-out cases. Include per-type sample counts and uncertainty. Do not
reuse these 40 development cases as held-out evidence or automatically deploy
the classifier based on this small, synthetic suite.

## References

- [Memory type definitions](../../../docs/developer/architecture/memory-types.md)
- [TypeSafe API](https://docs.typesafe.ai/api)
- [TypeSafe models and pricing](https://docs.typesafe.ai/models)
- [TypeSafe confidence](https://docs.typesafe.ai/confidence)
