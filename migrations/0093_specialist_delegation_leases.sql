ALTER TABLE agency_tasks ADD COLUMN execution_lease_seconds INTEGER CHECK(execution_lease_seconds BETWEEN 5 AND 60);
ALTER TABLE agency_tasks ADD COLUMN execution_lease_until TIMESTAMPTZ;
ALTER TABLE agency_remote_tasks ADD COLUMN cancel_requested BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE agency_remote_tasks ADD COLUMN cancel_completed BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE agency_remote_tasks ADD COLUMN last_control_check TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp();
CREATE TABLE agency_task_starts(task_id UUID PRIMARY KEY REFERENCES agency_tasks(id) ON DELETE CASCADE);
