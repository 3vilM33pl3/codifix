-- Sanitized operational health only; never provider/request/source contents.
ALTER TABLE device_replica_projections ADD COLUMN last_sync_error TEXT
    CHECK(last_sync_error IS NULL OR last_sync_error IN ('authority_denied','synchronization_incomplete'));
ALTER TABLE device_replica_projections ADD COLUMN last_sync_attempt_at TIMESTAMPTZ;
