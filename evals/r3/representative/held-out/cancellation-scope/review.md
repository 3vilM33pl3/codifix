# Held-out knowledge family: cancellation scope — DRAFT

Five fictional sources separate caller lifetime, remote stop requests and terminal
outcomes. Four questions form one held-out cluster; no live renderer is invoked.

1. r-17 was accepted/running before the caller timed out. Dropping local waiting
   does not cancel it; the red UI label reflects a local timeout, not remote proof.
2. Stop-requested is nonterminal. r-18's canceled-before-start receipt belongs to
   another operation and cannot close r-17. Reconciliation remains unresolved.
3. Retain identity and use bounded status reconciliation, not fresh automatic
   submission. Already-completed work is not undone by cancellation; compensation
   needs separate authorization. Exercise the three distinct cancellation races.
4. No usage, final status or maximum stop time is supplied. Decline exact charges,
   refunds and latency guarantees; the caller timeout is not a remote deadline.

Accept equivalent precise reasoning with citations and genuine blinded review.
Labels are draft and private; do not use the family for development calibration.
