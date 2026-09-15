# Meter interruption response

The controller stopped before dispatching D. C's request ID and payload digest
are recorded, but no complete reusable provider result was persisted. Do not
mark C settled from an empty response or relabel it as free. Seek an authoritative
usage/result receipt; record unresolved billing if none can be obtained.

A complete validated local result for A is retained. Exact replay of A should
reuse it and not bill again. The result binds model identity and request digest;
a response for a modified prompt cannot be substituted. A successful local
settlement write alone does not prove exactly-once provider execution.

If work is resumed after reconciliation, reserve before calling and enforce both
task and total caps. Requests that do not fit remain unstarted; a likely smaller
actual bill does not justify dispatching above the worst-case approved allowance.
