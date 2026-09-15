# Fern dependency circuit

This fictional in-memory controller protects one dependency. No network calls,
real clocks or service credentials are involved. Five modules separate commands,
policy, permits and the state machine.

Commands are `call TIME`, `finish TIME TOKEN ok|fail`, and `state TIME`. TIME is
an integer from 0 through 1,000,000. Each line returns one line. Malformed commands
return `invalid` without changes. Time must never move backwards: return `time`
and leave the entire controller unchanged if it would. Equal timestamps are valid.
All other syntactically valid commands advance the logical clock, even if they
return `busy`, `open` or `stale`; expiry processing occurs first.

## Stage 1 — closed failures and cooldown

Start closed with zero failures. Closed calls receive unique increasing tokens
(`grant:1`, etc.). At most eight outstanding permits exist. Completion consumes
a live token exactly once. A success resets consecutive failures to zero; three
failed completions open the circuit for ten ticks from the third completion.
Opening invalidates all outstanding permits and resets the failure counter.
Before its deadline calls return `open`. State renders `closed failures:N` or
`open until:T`. IDs are never reused, even after state transitions.

## Stage 2 — single recovery probe

At the open deadline enter half-open. `state` renders `half-open idle` until one
call obtains the sole probe permit, then `half-open busy`; additional calls are
`busy`. A successful current probe closes and resets failures. A failed current
probe reopens for ten ticks from that completion. Unknown, consumed or invalidated
tokens return `stale` and cannot affect failure counts or close a newer circuit.

## Stage 3 — expiring permits and time safety

Every permit expires five ticks after grant. At or after its deadline a completion
is stale. Expired closed permits are removed without inventing success/failure
and release capacity. Expiry of the active half-open probe reopens for ten ticks
from its expiry time, not the later polling time. If a command arrives after that
cooldown too, it observes half-open idle. Unconfirmed execution is not a proven
dependency failure; probe timeout is a conservative admission policy only.

Expiry processing must not resurrect old tokens. State reads process time but
never grant permits. Test exact boundaries, late polls, repeated receipts and
backwards time. This is not a distributed lease, retry worker or proof of actual
dependency health. Do not add runtime or network dependencies.
