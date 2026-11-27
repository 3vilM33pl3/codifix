-- One active outage per companion/service. Each report and status update retains its source.
CREATE TABLE agency_incidents (
    id uuid PRIMARY KEY,
    instance_id uuid NOT NULL,
    service text NOT NULL,
    task_id uuid NOT NULL REFERENCES agency_tasks(id),
    opened_at timestamptz NOT NULL,
    closed_at timestamptz,
    state text NOT NULL,
    investigation jsonb,
    verified jsonb,
    ticket jsonb
);
CREATE UNIQUE INDEX agency_incident_open ON agency_incidents(instance_id,service) WHERE closed_at IS NULL;
CREATE TABLE agency_incident_reports (
    task_id uuid PRIMARY KEY REFERENCES agency_tasks(id),
    incident_id uuid NOT NULL REFERENCES agency_incidents(id),
    reporter text NOT NULL
);
CREATE TABLE agency_incident_events (
    seq bigserial PRIMARY KEY,
    incident_id uuid NOT NULL REFERENCES agency_incidents(id),
    event_id text NOT NULL,
    at timestamptz NOT NULL DEFAULT clock_timestamp(),
    body jsonb NOT NULL,
    notified boolean NOT NULL DEFAULT false,
    tracked boolean NOT NULL DEFAULT false,
    problem text,
    UNIQUE(incident_id,event_id)
);
ALTER TABLE agency_incident_events ADD COLUMN retry_at timestamptz NOT NULL DEFAULT clock_timestamp();
