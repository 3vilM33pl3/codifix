-- Local watched-agent standing-policy intents. Exact retries preserve the
-- original question bytes; no human-confirmation receipt is fabricated.
CREATE TABLE federation_agent_expertise_intents (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    principal UUID NOT NULL,
    token_id UUID NOT NULL,
    space UUID NOT NULL,
    request_id UUID NOT NULL,
    input_digest JSONB NOT NULL,
    policy_digest JSONB NOT NULL,
    record JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(project_id,request_id),
    CHECK (request_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    CHECK (octet_length(record::text) <= 65536)
);
CREATE INDEX federation_agent_expertise_intents_scope
    ON federation_agent_expertise_intents(project_id,token_id,created_at DESC);

CREATE TABLE federation_agent_expertise_daily (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    token_id UUID NOT NULL,
    utc_day DATE NOT NULL,
    used SMALLINT NOT NULL CHECK (used BETWEEN 0 AND 20),
    PRIMARY KEY(project_id,token_id,utc_day)
);
