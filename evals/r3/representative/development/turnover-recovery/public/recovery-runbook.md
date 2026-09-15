# Alder recovery runbook — revision 4

Effective 2026-08-23. Replaces revision 2's credential-reuse procedure.

Keep publication disabled while recovering a restored instance. Verify restored
record/evidence integrity and preserve operation identities and prior provenance.
Separate those checks from enrollment of the currently authorized writer.

After the administrator approves replacement identity and capabilities, reconcile
the saved cursor and queued intents against gateway receipts. Apply ordered
catch-up with duplicate detection. Do not manufacture fresh operation identities
to get around an ambiguous or already accepted publication. Revalidate source
versions, reviewed evidence and current permissions before sending pending work.
Verify a bounded catch-up exercise before resuming normal publication.

Local reads may use retained, eligible knowledge while disconnected. Show its
origin, version and last-known state. Do not present the local copy as proof of
current remote state, restored write authority or permission to fetch new data.

The service has a proposed 30-minute recovery-time objective. Validation requires
a measured end-to-end exercise with a defined start and recovery endpoint; a
restore-tool duration alone does not demonstrate that objective.
