-- Successful private model replies survive worker restarts without another send.
-- This is task evidence, not public discovery or shared long-term memory.
CREATE TABLE agency_conversation_artifacts (
    task_id uuid PRIMARY KEY REFERENCES agency_tasks(id),
    turn_id uuid NOT NULL,
    request_digest text NOT NULL,
    evidence jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE agency_inbox_projections (
    task_id uuid PRIMARY KEY REFERENCES agency_tasks(id),
    revision bigint NOT NULL,
    projected_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
