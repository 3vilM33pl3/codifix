# Operations card — revision 2

Archived copy, printed 2026-07-02. Superseded by recovery runbook revision 4.

Earlier deployments used a long-lived shared writer credential. This card told
operators to restore that credential with the database and restart the writer
immediately after the database tool succeeded. It also recommended resetting
the cursor when synchronization stalled.

Those instructions predate generation-based fencing and the retirement decision.
The printed copy remains in the archive to explain historical operation, not to
authorize current recovery actions. It contains no usable credential.
