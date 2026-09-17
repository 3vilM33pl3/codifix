-- Opt-in, metadata-only local observations. No schedule, binding, policy or
-- worker is activated by migration. Old schedules must never survive partition
-- as runtime authority; observations are historical operational evidence.
CREATE TABLE supervisor_health_schedule (
    instance_id uuid PRIMARY KEY,
    area text NOT NULL UNIQUE,
    next_due_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (instance_id,area) REFERENCES supervisor_instance_binding(instance_id,area)
);
CREATE TABLE supervisor_health_observations (
    action_id uuid PRIMARY KEY REFERENCES supervisor_actions(id),
    instance_id uuid NOT NULL,
    area text NOT NULL,
    policy_revision uuid NOT NULL,
    observed_at timestamptz NOT NULL,
    next_due_at timestamptz NOT NULL,
    project_count bigint NOT NULL CHECK (project_count >= 0),
    canonical_count bigint NOT NULL CHECK (canonical_count >= 0),
    blocked_jobs bigint NOT NULL CHECK (blocked_jobs >= 0),
    failed_jobs bigint NOT NULL CHECK (failed_jobs >= 0),
    uncertain_actions bigint NOT NULL CHECK (uncertain_actions >= 0),
    FOREIGN KEY (instance_id,area) REFERENCES supervisor_instance_binding(instance_id,area),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision),
    CHECK (next_due_at >= observed_at + interval '5 minutes')
);
CREATE INDEX supervisor_health_observations_latest ON supervisor_health_observations(instance_id,observed_at DESC);
CREATE TRIGGER supervisor_health_observation_immutable BEFORE UPDATE OR DELETE ON supervisor_health_observations
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_health_schedule,supervisor_health_observations FROM PUBLIC;

-- Preserve old receipts and permit fresh targets at this explicitly supported
-- head. This does not authorize replaying an old import after schema upgrade.
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check
    CHECK (target_migration IN (56,57));
