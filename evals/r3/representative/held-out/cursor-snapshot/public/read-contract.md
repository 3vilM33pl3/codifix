# Iris incident listing contract L4

Listings are ordered by severity descending, then incident ID ascending. Severity
is not unique. A page cursor binds the full last sort tuple, project filter and
immutable read snapshot. A subsequent page continues strictly after that tuple
under the same order and snapshot. An ID-only or severity-only cursor is not
sufficient. Filters apply before ordering and page limits.

The page size is two rows. A missing next cursor means that snapshot has no more
eligible rows, not that the live system will never gain incidents. New writes and
severity edits appear in a new listing started without the old cursor, not by
silently changing the snapshot between pages. A cursor from another project or
expired snapshot is rejected explicitly; do not restart at page one invisibly.

Counts returned for a listing are snapshot counts. They are not a live population
counter or a guarantee of complete customer coverage beyond the selected project.
