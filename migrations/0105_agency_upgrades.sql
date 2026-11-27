-- The upgrade runner stays alive while the requesting companion is restarted.
CREATE TABLE agency_upgrades (
    task_id uuid PRIMARY KEY REFERENCES agency_tasks(id),
    instance_id uuid NOT NULL,
    release jsonb NOT NULL,
    state text NOT NULL CHECK (state IN ('queued','installing','installed','verified','failed')),
    report jsonb,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
