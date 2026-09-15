# Incident 118 response review

Reduce offered load with visible busy responses; inspect the reporting lock's
owner and business effect before cancellation. Do not blindly kill every session.
Reconcile any interrupted report and replace its unbounded transaction with
reviewed bounded work. Verify pool budgets across API and scheduler before
enabling more replicas. Preserve observations before changing multiple knobs.

After containment, test with fixed replica and pool counts, representative report
activity, admission rate and latency percentiles. Capture acquisition waits and
lock age separately from query execution time. The present incident sample does
not establish whether database hardware must be upgraded or an exact paid tier.
Nor does the simultaneous reporter completion prove that admission reduction
alone caused recovery. Customer completion/loss totals were not collected.
