# Listing v11 inspection

v11 selects active items from the requested project, then applies
`item.id NOT IN (SELECT item_id FROM completions)` without project correlation.
It reports an empty list as `all complete`. It does not distinguish a true empty
pending set from a predicate that becomes UNKNOWN because of imported nulls.

One proposed patch q27 removes NULL rows from the subquery but still ignores
project. Another converts null IDs to a made-up sentinel. Neither expresses Q18's
same-project matching rule. Neither is approved, and imported completion history
has not been deleted or reassigned.
