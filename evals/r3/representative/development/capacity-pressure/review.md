# Development knowledge family: capacity pressure — DRAFT

Five fictional sources, four questions. This family tests queueing and shared
resource budgets, not a Memory performance incident. Keep it in development.

1. Pool acquisition can fail before SQL with spare CPU. The sample shows all
   connections occupied and 24 transactions waiting on a reporting lock. Green
   cached liveness does not prove readiness; scaling can add connection demand.
2. Configured application demand is 300 versus 192 slots: 108 over budget. After
   allowing 12 scheduler connections, the API ceiling is 15 per replica at twelve
   replicas. That is arithmetic, not a measured optimal pool size or throughput.
3. A longer statement timeout does not change the two-second acquisition timer.
   Prefer explicit bounded admission, deliberate lock investigation and work
   reconciliation; don't kill all sessions or amplify pressure with instant retry.
4. Reporter completion confounds the improvement after admission reduction.
   No maximum rate or loss total was measured. Decline those guarantees and
   identify the missing controlled measurements.

Accept equivalent evidence-grounded explanations. Do not require a specific
vendor or query implementation. Citations and genuine blinded semantic review
remain necessary. Labels are draft; this is not capacity advice for a live system.
