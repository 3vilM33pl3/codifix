-- Proposal decisions append separately from immutable contribution receipts.
CREATE TABLE device_curated_contribution_resolutions (
    operation_id UUID PRIMARY KEY REFERENCES device_curated_contribution_receipts(operation_id),
    proposal_id UUID NOT NULL UNIQUE REFERENCES memory_proposals(id),
    resolution_json JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX device_curated_receipt_proposal
    ON device_curated_contribution_receipts((receipt_json->>'proposal_id'))
    WHERE receipt_json->>'proposal_id' IS NOT NULL;
ALTER TABLE laptop_curated_outbox ADD COLUMN resolution_json JSONB;
ALTER TABLE laptop_curated_outbox DROP CONSTRAINT laptop_curated_outbox_blocked_reason_check;
ALTER TABLE laptop_curated_outbox ADD CONSTRAINT laptop_curated_outbox_blocked_reason_check
    CHECK(blocked_reason IS NULL OR blocked_reason IN ('invalid_curated_content','authority_denied','proposal_rejected'));
