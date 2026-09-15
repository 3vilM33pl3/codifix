# Willow report replacement — P17

This fictional filesystem model distinguishes visible rename from crash-durable
replacement. P17 writes a complete temporary sibling, checks all writes, flushes
the temporary file successfully, renames it over the selected report name, then
flushes the parent directory successfully. Report durable success only after
both required flushes and the rename are confirmed. A rename is atomic for live
name lookup in this model, but is not itself a directory durability barrier.

Before rename, failure leaves the previous report selected. After a successful
rename with an unconfirmed directory flush, report an uncertain durability
outcome and reconcile the selected generation after recovery. Do not say the
rename rolled back merely because the final confirmation failed. Keep operation
ID, prior/new generation and confirmed stages in a durable intent journal.
No claim about every filesystem or storage device is made by this test model.
