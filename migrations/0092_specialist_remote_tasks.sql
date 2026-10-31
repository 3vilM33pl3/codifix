CREATE TABLE agency_remote_tasks (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id), peer_id TEXT NOT NULL,
 request JSONB NOT NULL, remote_id TEXT, context_id TEXT,
 last_response JSONB, reply_revision BIGINT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
