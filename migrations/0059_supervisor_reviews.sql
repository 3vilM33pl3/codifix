-- Human review is separate from model output and execution authority. No work
-- is created by this journal; original private proposals remain immutable.
CREATE TABLE supervisor_proposal_reviews (
    action_id uuid PRIMARY KEY REFERENCES supervisor_proposals(action_id),
    request_id uuid NOT NULL UNIQUE CHECK (request_id <> '00000000-0000-0000-0000-000000000000'),
    project_id uuid NOT NULL,
    reviewer_id uuid NOT NULL,
    proposal_digest bytea NOT NULL CHECK (octet_length(proposal_digest)=32),
    decision text NOT NULL CHECK (decision IN ('approve','reject')),
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER supervisor_proposal_review_immutable BEFORE UPDATE OR DELETE ON supervisor_proposal_reviews
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_proposal_reviews FROM PUBLIC;
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check
    CHECK (target_migration IN (56,57,58,59));
