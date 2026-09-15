# M42 reader transition checklist, revision 4

Before completing the reader transition: resolve partial-null rows through the
exception workflow; reconcile all three representations; test a negative
available/backorder case; confirm all active writer versions maintain both
representations. Keep monitoring divergence while mixed readers remain.

Before M43: inventory non-API consumers, upgrade the nightly export, verify no
remaining old-column reads/writes, obtain separate approval and take a recoverable
backup. A rollback must name compatible reader/writer versions and the schema
stage. Keeping an old image is not a data conversion strategy.

Stop on unexplained divergence. Do not zero negative values to force agreement
or derive a missing half of an already edited row without owner review. The
present evidence has no customer downtime objective or production peak-load
result; schedule a bounded rehearsal before making those claims.
