# Alder Service recovery journal

Synthetic operations record, 2026-08-24. Times are UTC.

| Time | Observation |
| --- | --- |
| 08:51 | Database restore command started from the previous evening's backup. |
| 08:54 | Restore command exited successfully. The restored schema is revision 14. |
| 08:56 | Local inspection found 120 memory records and 120 referenced evidence files. |
| 08:58 | Two sampled evidence digests matched their stored references. The remaining files were not checked in this exercise. |
| 09:02 | A synchronization attempt was refused by the company gateway. |
| 09:04 | Operators disabled publication and left the restored instance available for local inspection. |

The restored activity cursor is 48000. The backup inventory records accepted
source material through the preceding evening. It also contains seven queued
publication intents; this inventory does not establish that they reached the
gateway. The application retained the original intent identities.

This journal does not record when the incident began, when restoration was
authorized, or when normal service was recovered. No complete reconciliation or
end-to-end recovery measurement was recorded. A successful restore command is
the database tool's result, not a validation of all application behavior.
