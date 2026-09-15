# Settlement handover procedure, revision 7

Approved by the Copper Relay origin owner on 2026-08-19. This revision supersedes
the revision-4 printed retry card. Applicable to independently administered
Northbank and Copper Relay instances.

Before another transfer, establish the settlement's ledger status with the origin
owner. An acknowledgement timeout does not establish that a write failed.
Quarantine ambiguous outcomes for reconciliation. Do not blindly replay them.

Use the stable settlement identity as the deduplication key. Transport request
IDs identify attempts and may change; they are not settlement identities.
The deduplication record must cover the supported replay interval. Before
unpausing traffic, verify both configuration and a delayed-duplicate test at
the full supported interval, and record the origin owner's decision.

Northbank may submit a correction proposal with evidence. It may not overwrite
Copper Relay's authoritative publication. Keep earlier accepted versions and
their author/publisher attribution available for audit.
