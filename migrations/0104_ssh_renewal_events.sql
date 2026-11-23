CREATE TABLE agency_ssh_renewal_events (
    instance_id uuid NOT NULL,
    event_id uuid NOT NULL,
    host text NOT NULL,
    started_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    ca_task_id uuid,
    PRIMARY KEY (instance_id, event_id)
);
