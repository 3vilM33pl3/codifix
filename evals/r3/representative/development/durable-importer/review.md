# Durable event importer — development labels DRAFT

This is one repository family and one three-step observation, not three samples.
The whole sequence succeeds only if every stage passes. Each later stage must
also preserve the acceptance criteria from earlier stages.

Public context is the repository's README, reviewed contract, smoke input and
source files. `private/` contains reference code, step prompts and expected
outputs. Reveal only the current prompt to an agent; never mount the private
folder or return grading feedback to subsequent steps. The initial repository
passes the ordinary smoke input but has intentional transaction/recovery bugs.

Expected outcomes:

1. Before-commit failure leaves total, cursor and accepted identities unchanged.
   A real successful operation still changes state. Overflow does not partially
   commit. This rules out a blanket rejection or no-op implementation.
2. Gaps wait durably; receipt of the predecessor drains the authority-ordered
   contiguous prefix. Exact duplicates have no extra effects. Conflicting IDs
   or sequence positions are rejected, including conflicts with pending records.
   A boundary-arithmetic test distinguishes authority order from lexical-key order.
3. Worker replacement preserves durable accepted and pending state. A lost
   acknowledgement does not erase a commit; an exact retry remains a duplicate.
   A failed precommit operation remains safely retryable after restart.

Equivalent designs are acceptable: clone-and-swap is a reference, not a required
implementation. Grading compares observable protocol/state, not source strings.
The in-memory Store is an explicit test double, not demonstrated disk durability.
No provider, real database or cryptography is required for this repository.

The reference and intentionally broken/no-op controls must be verified in the
isolated Rust runner before labels are offered for approval. The complete R3
suite, campaign binding and Olivier's label approval remain separate requirements.
