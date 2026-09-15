# Held-out knowledge family: lock inversion — DRAFT

Five fictional sources and four questions. A simultaneous two-thread wait cycle
and a separate callback self-reentry require different explanations.

1. a C→I and b I→C form the observed cycle; heartbeat does not imply worker progress.
2. Consistent C→I order preserves atomic projections; partial publication is unsafe.
3. Callbacks run unlocked, with generation revalidation before applying results.
4. Selected fixes do not prove global liveness or quantified fleet benefit.

Accept equivalent precise wording with citations. Private draft labels require
blinded review and stay outside answering and development-calibration contexts.
