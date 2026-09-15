# Copper Relay acknowledgement incident

Synthetic exercise. Recorded by Northbank Operations, 2026-08-18.
Times below are UTC. Settlement values and customer identities were not included
in the approved incident packet.

| Time | Observation |
| --- | --- |
| 10:03:11 | Northbank sent settlement `settle-047` using transport request `req-a1`. |
| 10:03:12 | Copper Relay's ledger reported `settle-047` committed under `req-a1`. |
| 10:03:14 | Northbank timed out waiting for the acknowledgement. Its UI displayed “unknown”. |
| 10:03:17 | An operator followed the printed retry card, sending `settle-047` under `req-b2`. |
| 10:03:18 | Copper Relay recorded a second ledger posting for `settle-047`. Its deduplication trace compared request IDs. |
| 10:06:00 | The on-call lead paused retries for this settlement and requested reconciliation. |

No packet establishes the total affected population, the monetary impact or
whether other settlements were duplicated. A separate cache warning at 09:58
cleared before this request; no supplied evidence connects it to the two postings.
