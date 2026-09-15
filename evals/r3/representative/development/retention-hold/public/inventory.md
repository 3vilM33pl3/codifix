# Inventory at fixed review time, 2026-08-31 12:00 UTC

Case C10 attachment a10: captured August 1 10:00 UTC; expiry August 31 10:00;
hold H4 active for C10. Payload exists; ordinary search excludes it.
Case C11 attachment a11: same capture and expiry; no hold. Payload still exists
because the cleanup worker was paused. Ordinary search excludes it.
Case C12 attachment a12: captured August 20 10:00; expiry September 19 10:00;
no hold. It remains eligible for ordinary authorized search.

All three metadata receipts remain within 180 days. The worker backlog lists two
expired payload candidates before hold filtering; this is not two approved
deletions. A status named "expired" describes access/retention eligibility, not
proof of physical destruction.

H4's case authority has not released it. A support ticket for C10 was closed on
August 30, but the hold registry is a separate authority source.
