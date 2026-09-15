# Retired scale card, revision 2

For the old single-process service, low database CPU was treated as spare capacity
and the default response to API latency was to double worker replicas. This card
predates per-process pools and the scheduler's shared connection budget.

It is retained to explain historical operations, not as current scale approval.
Revision 6 requires connection and contention measurements across all processes.
