# D12 repair and verification — pending

Add a local admission gate, distinct readiness/liveness states, an active-stream
registry and the 30-second bounded drain sequence. Readiness becomes false at
drain start even while liveness remains true. Reject late arrivals before
execution; do not keep accepting work to make a probe look healthy.

Test stale routing after removal, an already accepted stream completing during
drain, an unfinished stream at the deadline and repeated shutdown signals. The
same deadline must not be extended by each signal. Emit completion only when
the registry is empty or the remaining streams have documented deadline outcomes.

Preserve request identities and expose uncertainty for original accepted calls
without confirmed terminal outcomes. Do not automatically launch them again
under new IDs. Snapshot-cache changes can reduce stale attempts but do not
replace local admission fencing or management of existing streams. No rollout
or successful repair of the original run is recorded.
