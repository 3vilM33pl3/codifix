CREATE TABLE agency_attempts (
 id UUID PRIMARY KEY, task_id UUID NOT NULL REFERENCES agency_tasks(id),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
ALTER TABLE agency_tasks ADD COLUMN reserved_microusd BIGINT NOT NULL DEFAULT 0 CHECK(reserved_microusd>=0);
ALTER TABLE agency_tasks ADD COLUMN reserved_calls BIGINT NOT NULL DEFAULT 0 CHECK(reserved_calls>=0);
ALTER TABLE agency_tasks ADD COLUMN reserved_turns BIGINT NOT NULL DEFAULT 0 CHECK(reserved_turns>=0);
