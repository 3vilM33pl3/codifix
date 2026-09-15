# Backfill B17 closeout

At 10:45 UTC the scanned snapshot contained 10,000 stock rows. Of these, 9,960
had two non-null new fields, 32 had both new fields null, and eight had exactly
one null field. The "processed=10000" metric counts examined rows, not converted
rows. This attempt updated the 32 both-null rows and left the eight exceptions
untouched. The exception queue remains open.

In the 9,960 already-populated rows, a reconciliation query found three where
`available != on_hand - reserved`. Their cause is not established. No rows were
deleted or automatically repaired. Another comparison is required after reviewed
repairs and before switching every reader.

The scan ran for 44 seconds with a 500-row batch setting. There was no concurrent
peak-load experiment, lock-wait distribution or query-latency measurement. The
run does not measure the duration or customer impact of M43.
