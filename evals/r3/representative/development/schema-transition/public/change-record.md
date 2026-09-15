# Alder inventory: schema change M42

Approved 2026-08-12. Inventory originally stored `stock.available` as a signed
integer. The new representation stores `stock.on_hand` and `stock.reserved`;
available is their difference. A negative available value is valid for a recorded
backorder. The goal is allocation visibility, not changing backorder policy.

M42 adds nullable columns without deleting `available`. Release 2.8 dual-writes
all three fields in one transaction and reads the old field. The backfill derives
on_hand = available and reserved = 0 for legacy rows, but only where BOTH new
columns are null. A partial-null row goes to the exception queue; it is never
silently overwritten. Operator-reviewed corrections own their values.

Release 2.9 reads the difference and still dual-writes. Removing `available` is
M43, a separately approved contract migration. M43 requires every reader and
writer, including scheduled jobs, to use the new representation. No M43 approval
is recorded here.
