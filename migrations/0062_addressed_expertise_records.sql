-- Separate, addressed private ATProto records. Never include these rows in
-- federation_accepted or the space-wide publication pull.
CREATE TABLE federation_expertise_addressed (
    space UUID NOT NULL REFERENCES federation_gateway_spaces(space) ON DELETE RESTRICT,
    request_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('question','answer')),
    sender UUID NOT NULL,
    recipient UUID NOT NULL,
    record_digest JSONB NOT NULL,
    record JSONB,
    status TEXT NOT NULL CHECK (status IN ('queued','published','failed','auth_blocked')),
    proof JSONB,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    payload_until TIMESTAMPTZ NOT NULL,
    metadata_until TIMESTAMPTZ NOT NULL,
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    last_error TEXT,
    auth_generation BIGINT,
    PRIMARY KEY (space,request_id,kind),
    CHECK (sender <> recipient AND request_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (expires_at > created_at AND payload_until > expires_at AND metadata_until > payload_until),
    CHECK ((lease_token IS NULL) = (lease_until IS NULL)),
    CHECK (record IS NULL OR octet_length(record::text) <= 65536),
    CHECK (status <> 'published' OR proof IS NOT NULL)
);
CREATE INDEX federation_expertise_addressed_ready ON federation_expertise_addressed(space,next_attempt_at,created_at,request_id)
    WHERE status='queued';
