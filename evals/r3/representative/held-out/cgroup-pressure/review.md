# Held-out knowledge family: group memory pressure — DRAFT

Five synthetic sources and four questions use a stipulated disjoint charge model,
not a claim about every kernel's reclaim behavior.

1. 480 MiB charged, 32 MiB headroom; adding 48 reaches 528, over by 16.
2. Budget total bytes/working set, not just heap, host-free memory or queue count.
3. Fake admission verifies accounting only. Original external outcomes remain
   unknown after the process kill; no rollback or retry guarantee follows.
4. Leak identity, fleet limits and failure probabilities need additional evidence.

Accept equivalent exact arithmetic and explanations with citations. Private
labels remain draft, held out from calibration and subject to blinded review.
