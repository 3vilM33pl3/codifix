# Pinned synthetic zone table Z17

These explicit fixture rules are authoritative for this exercise. No live time
service or assumption about a real jurisdiction's future rules is needed.

Zone `Harbor/Civil`: standard offset UTC+00:00; summer offset UTC+01:00.
On 2026-03-29 at 01:00 UTC the clock jumps from 00:59:59 to 02:00:00 local.
Local 01:30 therefore has no corresponding instant on that date.
On 2026-10-25 at 01:00 UTC the clock moves from 01:59:59 to 01:00:00 local.
Local 01:30 occurs at both 00:30 UTC and 01:30 UTC on that date.

On 2026-10-24, local 01:30 is 00:30 UTC. On 2026-10-26, local 01:30 is
01:30 UTC. A fixed 24-hour interval is not the definition of consecutive local
business dates. Z17's digest is recorded in the scheduler's preparation receipt;
no change to these fixture rules is recorded in the incident packet.
