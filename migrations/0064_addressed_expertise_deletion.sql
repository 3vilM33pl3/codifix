-- An addressed payload is not locally purged until its exact PDS record has
-- been removed and a newer signed repository snapshot proves key absence.
ALTER TABLE federation_expertise_addressed
    ADD COLUMN delete_status TEXT CHECK (delete_status IN ('queued','verified','failed','auth_blocked')),
    ADD COLUMN delete_proof JSONB,
    ADD COLUMN delete_lease_token UUID,
    ADD COLUMN delete_lease_until TIMESTAMPTZ,
    ADD COLUMN delete_attempts SMALLINT NOT NULL DEFAULT 0 CHECK (delete_attempts BETWEEN 0 AND 5),
    ADD COLUMN delete_next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    ADD COLUMN delete_error TEXT,
    ADD COLUMN delete_auth_generation BIGINT,
    ADD CONSTRAINT addressed_delete_lease_pair CHECK ((delete_lease_token IS NULL) = (delete_lease_until IS NULL)),
    ADD CONSTRAINT addressed_delete_verified_proof CHECK (delete_status <> 'verified' OR delete_proof IS NOT NULL);

CREATE INDEX federation_expertise_addressed_delete_ready
    ON federation_expertise_addressed(space,delete_next_attempt_at,payload_until)
    WHERE record IS NOT NULL AND delete_status IS DISTINCT FROM 'verified';
