# Development knowledge family: schema transition — DRAFT

Five fictional operational sources; one scenario and four questions. The API
status, consumer inventory and backfill counts intentionally disagree with the
retired shortcut. This is a data-compatibility problem, not a configuration-cache
or replay problem. Keep this family out of held-out material.

1. M43 is not ready: the export still reads the old column, eight partial-null
   exceptions and three divergences remain, and separate approval is absent.
   Processing 10,000 rows does not mean converting all of them coherently.
2. The 32 both-null rows were converted. Review the remaining partial and
   divergent rows, then reconcile. Negative availability can be a legitimate
   backorder. Exact repair values and causes are unavailable.
3. 2.8 is the documented compatible rollback candidate while M42 retains the old
   field and dual-write consistency holds. 2.7 writers can diverge from 2.9
   readers. Do not interpret this as approval to ignore existing divergence.
4. Forty-four seconds measures one backfill scan. It does not establish M43
   duration, peak-load behavior or customer impact; refuse those guarantees.

Accept equivalent precise language and source-grounded reasoning, not an exact
word match. Citations must support the conclusion. No human ballot, label freeze
or paid run is implied. Private rubrics and this sheet are never agent context.
