# Wait-state interpretation limits

A live heartbeat can coexist with blocked workers. The simultaneous ownership
and waits show a cycle here; low CPU alone would not establish deadlock. The
callback test is a separate self-reentry case, not proof that all waits share
the same cause.

There is no representative latency series, contention denominator, database
lock evidence or fleet incident count. Fixing the demonstrated order and reentry
hazards is not proof of whole-system deadlock freedom. Preserve those limits
and do not recommend publishing half an operation to unblock a thread.
