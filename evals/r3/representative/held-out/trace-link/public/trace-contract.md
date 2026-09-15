# Pine asynchronous tracing — U20

This fictional trace system distinguishes trace identity, span identity and
sampling. For an asynchronous queue handoff, the publisher records an immutable
carrier containing trace ID and publishing span ID. A consumer starts its own
processing span and records a link to that exact carrier. Its processing span
need not reuse the publisher's span ID or be a synchronous child. Retry attempts
have distinct processing span IDs and retain the same original carrier link.

Sampling controls exported diagnostic spans, not whether a request or queued
work exists. An absent exported span is not proof of missing execution. Business
outcomes come from the queue/job receipts, not trace export alone. Reject malformed
carriers explicitly; do not invent another request's identity or attach payload
contents to telemetry. This small contract is not a full tracing standard.
