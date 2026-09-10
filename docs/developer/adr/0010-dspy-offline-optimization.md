# ADR 0010: DSPy as an offline LLM-program optimization layer

Status: proposed
Date: 2026-09-10

## Context

Memory Layer uses language models for optional curation and answer synthesis,
while keeping capture, canonical memory, provenance, authorization, persistence,
and retrieval behavior inside the existing Rust service architecture. Improving
the prompts for model-backed steps by hand is difficult to reproduce across
models and can optimize one example at the expense of held-out quality.

The repository already has an evaluation system with versioned suites, paired
conditions, item-level scoring, release gates, latency, token usage, and artifact
capture. Relevant evaluations include retrieval quality, grounded answers,
adversarial stale-memory behavior, resume quality, and agent build tasks. This
provides the measurable objective and held-out evidence needed for systematic
prompt optimization.

DSPy is a Python framework for expressing language-model programs and compiling
their instructions or demonstrations against a metric. Adopting DSPy inside the
production service would introduce a second runtime and could blur the boundary
between experimental optimization and authoritative Memory behavior. Rejecting
optimization entirely would leave useful evaluation evidence disconnected from
prompt development.

## Decision

Memory Layer may use DSPy as an **offline research and optimization tool** for
bounded, model-backed steps. DSPy is not part of the authoritative production
data path.

The first eligible targets are:

- grounded-answer synthesis from retrieved memory and citations;
- optional LLM-assisted curation that produces candidates for the existing
  validation and curation pipeline;
- query rewriting or candidate summarization; and
- an experimental LLM reranker, provided deterministic retrieval remains
  available and the normal query diagnostics preserve its contribution.

A DSPy experiment must call Memory through the service HTTP API or consume
versioned evaluation artifacts. It must not access PostgreSQL directly or create
an independent capture, curation, retrieval, or persistence path.

Optimization outputs are build-time artifacts, not self-modifying runtime state.
A prompt or demonstration set may enter the product only after it is:

1. serialized in a reviewable, provider-neutral format that the existing Rust
   model client can consume without requiring Python or DSPy;
2. versioned with the DSPy program, optimizer and parameters, model identities,
   dataset and split checksums, metric definition, budget, and evaluation result;
3. compared with the current baseline on an untouched held-out set using the
   existing Memory evaluation and gate machinery; and
4. reviewed and committed like any other behavior-changing source artifact.

Production services, CLI commands, watchers, and MCP adapters must not import
DSPy, launch a DSPy process, or optimize prompts automatically. Re-optimization
is an explicit development or research operation with a bounded provider-call
and financial budget.

## Evaluation contract

Optimization data must be separated into training, validation, and untouched
test data. The final test set must not participate in candidate generation,
selection, reflection, few-shot selection, or metric tuning. Suites and splits
must be versioned so a result can be reproduced and benchmark contamination can
be detected.

Metrics must encode Memory's safety and quality priorities rather than only
answer fluency. At minimum, applicable experiments use hard failure gates for:

- missing required provenance or citations;
- unsupported or forbidden assertions;
- selecting known stale evidence when fresher contradictory evidence exists;
- invalid structured output; and
- bypassing project scope, authorization, or the service API boundary.

After those gates pass, the optimization score may combine task success,
retrieval or assertion recall, calibrated confidence, latency, token usage, and
estimated provider cost. LLM judges may supplement deterministic scores, but
must be versioned, calibrated against reviewed examples, and must not override a
deterministic safety failure.

Promotion requires a paired comparison against the current artifact using the
same model configuration and evaluation inputs. Improvements on training or
validation data alone are insufficient. Missing usage or cost data is unknown,
not zero.

## Architectural boundaries

The following remain authoritative and outside DSPy optimization:

- raw capture and immutable source evidence;
- canonical IDs, versions, replacement decisions, and provenance assembly;
- authentication, authorization, project scoping, and federation policy;
- PostgreSQL persistence and transactional behavior;
- deterministic curation validation and fallback behavior; and
- lexical, semantic, graph, and relation-aware retrieval contracts.

An optimized model step may propose or rank data only through an existing typed
boundary. Existing validation, provenance, scope, and fallback rules apply to its
output exactly as they do to a hand-written prompt.

## Initial adoption

The first experiment should be a separate Python research adapter for
grounded-answer synthesis. It should translate a versioned subset of the
`research-v1` evaluation data into DSPy examples, wrap existing Memory scoring in
a DSPy metric, compile a candidate instruction or demonstration set, and export
that candidate for evaluation by `memory eval`.

The pilot is successful only if the exported candidate improves held-out
grounding or task success without violating stale-memory and citation gates and
without an unacceptable latency or cost regression. The pilot does not itself
authorize replacing the production prompt.

## Consequences

Prompt changes can be derived from explicit metrics and evaluated reproducibly
instead of relying entirely on manual prompt editing. The same harness can test
whether a smaller or cheaper model retains acceptable quality.

The project gains a Python development dependency for DSPy experiments, but the
released Rust runtime and service deployment remain unchanged. Optimization can
consume substantial provider calls, so experiments require budgets and retained
artifacts. Results remain sensitive to dataset quality, metric design, provider
changes, and overfitting; held-out evaluation and manual review remain mandatory.

## Alternatives considered

- **Embed DSPy in the production query or curation path.** Rejected because it
  adds a Python runtime and makes optimization machinery part of service
  availability, latency, and security.
- **Let DSPy replace retrieval, curation, or provenance rules.** Rejected because
  those are authoritative system invariants rather than prompt parameters.
- **Keep hand-tuning prompts only.** Rejected as the sole approach because the
  existing evaluation system can support measurable, reproducible optimization.
- **Build a prompt optimizer in Rust immediately.** Deferred because it would
  duplicate mature experimental machinery before the project has demonstrated
  that optimization improves a bounded Memory task.
