# Schedule s-81 rehearsal journal

s-81 is configured for 01:30 in Harbor/Civil. The service host runs UTC. A
2026-10-24 warm-up resolved local 01:30 to 00:30 UTC and persisted offset +01:00.
The old worker reused that offset for later dates instead of resolving Z17.

At 2026-10-25 00:30 UTC, the transport accepted occurrence s-81/2026-10-25. At
01:30 UTC a restart's catch-up loop proposed it again with a different job ID;
the test transport rejected the duplicate occurrence identity. One acceptance
is recorded. A job ID is not the schedule/date identity.

For 2026-10-26 the stale-offset worker planned 00:30 UTC, one hour earlier than
the Z17 civil-time resolution. No live customer message was sent; this was a
synthetic rehearsal. The journal contains neither a full annual execution history
nor end-to-end dispatch latency measurements.
