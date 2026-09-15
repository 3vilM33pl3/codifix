# Fir analysis meter, revision 3

This fixture uses integer microcredits, not money or actual provider pricing.
Before each external invocation the controller durably reserves a worst-case
allowance against both task and campaign budgets. Settled usage replaces its
reservation, freeing only the unused difference. Unknown outcomes retain the
full reservation until reconciled. Calls are not assumed free because a response
body or acknowledgement was lost.

One logical task may contain several provider calls. A cached completed response
with the same request identity and payload digest is reused without a new call.
Changed input under that identity is rejected. An uncertain request is not
automatically invoked again. Any explicitly approved new call uses a new identity
and must fit the remaining budget; it may repeat the prior provider charge.

Parent sequence totals already include child steps. For task execution totals,
count the parent once; child diagnostics explain steps rather than add cost again.
Preparation/indexing usage has a separate ledger, but counts toward the approved
overall campaign cap. It must not be hidden inside a per-task success percentage.
