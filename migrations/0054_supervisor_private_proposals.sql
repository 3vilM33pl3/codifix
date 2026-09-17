-- Private model suggestions are evidence, never approvals or executable work.
-- Seeds/enables nothing. Project UUID is retained without a projects FK so this
-- journal does not alter project deletion behavior or erase retained provenance.
CREATE FUNCTION supervisor_proposal_evidence_valid(digests bytea[]) RETURNS boolean
LANGUAGE sql IMMUTABLE STRICT AS $$
    SELECT cardinality(digests) BETWEEN 0 AND 8
        AND (cardinality(digests) = 0 OR (array_ndims(digests) = 1 AND array_lower(digests,1) = 1))
        AND NOT EXISTS (SELECT 1 FROM unnest(digests) AS value WHERE value IS NULL OR octet_length(value) <> 32)
$$;

CREATE TABLE supervisor_proposals (
    action_id uuid PRIMARY KEY REFERENCES supervisor_actions(id),
    area text NOT NULL,
    attempt_id uuid NOT NULL UNIQUE REFERENCES supervisor_attempts(id),
    policy_revision uuid NOT NULL,
    fence bigint NOT NULL CHECK (fence > 0),
    project_id uuid NOT NULL CHECK (project_id <> '00000000-0000-0000-0000-000000000000'),
    request_digest bytea NOT NULL CHECK (octet_length(request_digest) = 32),
    proposal_digest bytea NOT NULL CHECK (octet_length(proposal_digest) = 32),
    body jsonb NOT NULL CHECK ((
        jsonb_typeof(body) = 'object'
        AND body ?& ARRAY['summary','proposal','evidence_indices']
        AND body - 'summary' - 'proposal' - 'evidence_indices' = '{}'::jsonb
        AND jsonb_typeof(body->'summary') = 'string'
        AND octet_length(body->>'summary') BETWEEN 1 AND 4096
        AND body->>'proposal' IN ('none','ask_human','propose_correction','review_release')
        AND jsonb_typeof(body->'evidence_indices') = 'array'
        AND jsonb_array_length(body->'evidence_indices') <= 8
        AND pg_column_size(body) <= 32768
    ) IS TRUE),
    evidence_digests bytea[] NOT NULL CHECK (supervisor_proposal_evidence_valid(evidence_digests)),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (area,action_id) REFERENCES supervisor_actions(area,id),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision)
);
CREATE INDEX supervisor_proposals_scope ON supervisor_proposals(area,project_id,action_id);
CREATE TRIGGER supervisor_proposal_immutable BEFORE UPDATE OR DELETE ON supervisor_proposals
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_proposals FROM PUBLIC;
