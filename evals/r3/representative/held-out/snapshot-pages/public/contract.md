# Pine snapshot incident pages

Live records have a positive u32 ID, a project slug and severity1..9. Slugs contain
one to sixteen lowercase ASCII letters or hyphens. `put ID PROJECT SEVERITY`
inserts/replaces one live record and prints `ok`; `drop ID` prints `removed` or
`missing`. These operations do not change already-created read snapshots.

`begin PROJECT` captures exactly that project's eligible records, ordered by
severity descending then ID ascending. Snapshots receive increasing positive
IDs starting at1 and never reuse an expired ID. Output is `snapshot:ID rows:N`.
An empty project produces an empty snapshot. This is an in-memory model, not
database snapshot persistence or deployed authentication.

`page SNAPSHOT PROJECT CURSOR` reads at most two rows from that snapshot. `-`
means the first page. Subsequent cursors have form `SNAPSHOT.PROJECT.SEVERITY.ID`
and bind the full last sort tuple, project and snapshot. Continuation is strictly
after that tuple in the documented mixed-direction order. A cursor's tuple must
identify an actual row of that snapshot. Unknown/malformed/mismatched cursors
print `invalid-cursor`, with no rows or state change. A mismatched requested
project also prints `invalid-cursor`, including on a first-page request.

Successful pages print `rows:ID@SEVERITY,... next:CURSOR`, or `next:end` if no
eligible rows remain after this page. Empty output is `rows: next:end`. Determine
the next marker from eligible snapshot rows, not an unfiltered live-table limit.
New insertions, deletions, project moves and severity edits require a new snapshot
to become visible. Count and pages of an existing snapshot remain consistent.

`expire SNAPSHOT` prints `expired` or `missing-snapshot`. Any page of an unknown
or expired snapshot prints `missing-snapshot`; it must not silently create a new
snapshot or restart listing. Other syntax/ID/project/severity errors and extra
tokens print `invalid`. Existing state is preserved on error.

The prototype selected current live rows and used only cursor severity. That
omits ties and changes pages during live mutations. Repair the snapshot contract
without introducing a database, crypto or alternative command interface.
