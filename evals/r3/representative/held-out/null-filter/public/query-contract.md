# Maple pending-work filter — Q18

This fictional query lists active items of the requested project for which no
matching completed record exists in that same project. IDs are non-null on
items; imported completion rows may have a null item ID, meaning unlinked data,
not completion of every item. A completion from another project does not match.

The SQL semantics used here have TRUE, FALSE and UNKNOWN. Comparing a value to
NULL yields UNKNOWN. WHERE retains only TRUE. For `x NOT IN (values)`, a matching
value makes the predicate FALSE; otherwise a NULL in the list makes it UNKNOWN.
An empty list yields TRUE. These explicit rules govern the fixture.

Use a project-correlated `NOT EXISTS` match or an equivalent explicitly null-safe
anti-join. Filter project and active eligibility before pagination/limits. Preserve
unlinked completion rows for review; do not delete or invent item IDs to make a
listing work. A query repair is not permission to widen project access.
