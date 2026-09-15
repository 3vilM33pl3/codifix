# U20 repair review — pending

Persist the exact trace/publishing-span carrier at enqueue. Consume that carrier
independently of ambient thread-local state, create a fresh processing span for
each attempt and link it to the original carrier. Restore/clear scoped ambient
context after processing so pooled threads do not leak diagnostic identities.

Test reused threads, unsampled successful jobs, missing/malformed carriers,
multiple attempts and exact links. Keep business outcome reconciliation based
on durable receipts. Sampling changes cannot reconstruct a carrier already lost
or separate merged attempt IDs. Do not fill missing spans with fabricated ones.
No rollout, payload egress or measured latency improvement is recorded.
