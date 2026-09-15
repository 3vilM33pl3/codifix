# E3 seed77 bucket fixture

The pinned function's account results for this exercise are:
account u17 -> bucket314; account u88 -> bucket55; account u91 -> bucket145.
These values are supplied deterministic fixture data; no external hash service
is required. Account u17's display name changes from Reed to Fern while its
authenticated ID remains u17. That edit does not change its supplied bucket.

Session-derived outputs differ: u17's session s1 -> bucket42; session s2 ->734.
Those are outputs for the wrong assignment input, not alternate valid buckets
for the account contract. Reusing the same seed cannot compensate for changing
the subject being hashed.

The10% rollout was active through12:00 UTC. Revision r2 deliberately increased
it to20% at12:00 with the same experiment ID, seed and account bucket function.
No other seed or identity migration was approved.
