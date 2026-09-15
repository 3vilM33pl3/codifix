# Held-out knowledge family: circuit probes — DRAFT

Five fictional sources, four questions. The explicit shared-guard policy is the
exercise contract; it is not a universal breaker design. Keep the family held-out.

1. Cooldown ended at10.2. p1 could probe at10.3; p2 violated the one shared slot.
   Waiting ten seconds permits a probe, not a claim of recovered dependency.
2. p1 opened generation8 until21.0. Generation7 p2 cannot close it; results need
   both current generation and claim binding. Admission and fencing are separate.
3. Exclude local policy/validation errors; cached liveness is not dependency
   readiness. Test shared admission and stale/current completions, retaining a
   positive recovery path. No corrected deployment is recorded.
4. Actual recovery, customer error rate and optimal cooldown are not measured.
   Decline those claims rather than using a stale success or configured delay.

Accept equivalent precise time/state reasoning with citations and genuine blinded
review. Private draft labels are not answering context or calibration material.
