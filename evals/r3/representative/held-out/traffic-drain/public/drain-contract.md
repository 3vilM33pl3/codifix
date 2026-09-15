# Harbor responder draining — D12

This fictional gateway admits long-lived answer streams. D12 distinguishes
process liveness, readiness for new admissions and completion of accepted streams.
To drain a responder, first close its local new-admission gate and mark it not
ready. The gateway's routing view may lag, so a stale route reaching the closed
gate receives an explicit retryable not-admitted response before execution.

Track active accepted streams until each terminates. Existing streams may finish
after readiness is false. Keep the process alive while draining, up to a defined
deadline. At the deadline cancel remaining streams explicitly, record their
terminal/uncertain outcomes as supported by evidence and only then terminate.
Do not report canceled or unfinished work as successful completion.

Readiness is not the active-stream count. Removing an endpoint from discovery
does not close established streams. A requester may retry a not-admitted request;
retrying already accepted work needs its request-identity/idempotency contract.
No exactly-once guarantee is defined by D12 alone.
