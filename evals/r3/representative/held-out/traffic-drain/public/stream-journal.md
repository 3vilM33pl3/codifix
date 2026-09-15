# Selected shutdown journal

Times are elapsed seconds in one synthetic rehearsal. The desired drain begins
at t=100 with a 30-second deadline, hence final deadline t=130.

| Time | Observation |
| --- | --- |
| 98 | Stream s1 accepted and begins answering |
| 99 | Stream s2 accepted and begins answering |
| 100 | Shutdown requested; discovery removal acknowledged |
| 101 | Gateway with stale snapshot sends new request s3 |
| 102 | v4 terminates responder; s1/s2 connections end without terminal answer |

A separate fake-responder test, allowed to remain alive with admissions closed,
completes s1 at t=112. s2 is still active at t=130 and receives explicit cancel.
s3 is rejected not-admitted by the local gate. The fake test does not prove what
the original interrupted s1/s2 provider operations did after connection loss.
No result receipt or provider usage reconciliation for that original run exists.
