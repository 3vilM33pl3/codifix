# Deployment inspection, 2026-08-19 11:00 UTC

Public API: all four pods at 2.9. Allocation consumer: two pods at 2.8.
Nightly export: image 2.7, next run at 23:00 UTC, selects `available` directly.
M42 applied; M43 not applied. The deployment board displays "API rollout done".

Database disk usage 41%; replication lag 120 ms in this one inspection. Neither
number is an acceptance criterion for removing a column. The export job is
owned by Operations; it is not included in the API deployment controller.

Rollback package 2.8 remains available. Package 2.7 writes only `available` and
does not maintain on_hand/reserved. The release operator may select 2.8 while the
old column remains and the dual-write invariant holds. Rolling writers back to
2.7 after new readers are enabled can create divergent values.
