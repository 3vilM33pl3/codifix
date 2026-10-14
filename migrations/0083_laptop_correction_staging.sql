-- New local correction versions never mutate the downloaded owner projection.
CREATE TABLE laptop_correction_staging_receipts (
    operation_id UUID PRIMARY KEY,
    subscription_id UUID NOT NULL REFERENCES device_replica_projections(subscription),
    local_project_id UUID NOT NULL REFERENCES projects(id),
    request_sha256 TEXT NOT NULL CHECK(request_sha256 ~ '^[0-9a-f]{64}$'),
    receipt_json JSONB NOT NULL,
    staged_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
