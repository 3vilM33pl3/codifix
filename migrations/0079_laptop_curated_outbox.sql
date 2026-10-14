-- Explicit selected-device bindings; ordinary owner databases have no rows.
CREATE TABLE laptop_curated_bindings (
    local_project_id UUID PRIMARY KEY REFERENCES projects(id),
    binding JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE laptop_curated_outbox (
    operation_id UUID PRIMARY KEY,
    local_project_id UUID NOT NULL REFERENCES projects(id),
    local_memory_id UUID NOT NULL REFERENCES memory_entries(id),
    binding JSONB NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','accepted','proposed','conflict','blocked')),
    payload JSONB,
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    receipt JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((lease_token IS NOT NULL)=(state='running'))
);
CREATE TABLE laptop_curated_origins (
    local_canonical_id UUID PRIMARY KEY,
    owner_canonical_id UUID NOT NULL,
    owner_version INTEGER NOT NULL CHECK(owner_version>0)
);
CREATE FUNCTION enqueue_laptop_curated_version() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO laptop_curated_outbox(operation_id,local_project_id,local_memory_id,binding)
    SELECT NEW.id,NEW.project_id,NEW.id,b.binding FROM laptop_curated_bindings b
    WHERE b.local_project_id=NEW.project_id AND b.enabled AND NOT NEW.is_tombstone;
    RETURN NEW;
END $$;
-- Outbound intent commits in the exact curation transaction. Remote replica
-- records live in device_replica_records and never execute this trigger.
CREATE TRIGGER enqueue_laptop_curated_version AFTER INSERT ON memory_entries
FOR EACH ROW EXECUTE FUNCTION enqueue_laptop_curated_version();
