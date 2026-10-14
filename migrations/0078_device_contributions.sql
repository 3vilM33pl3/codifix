-- Curated-only owner receipts. No raw capture or session body is accepted.
CREATE TABLE device_curated_contribution_receipts (
    operation_id UUID PRIMARY KEY,
    approval_id UUID NOT NULL REFERENCES onboarding_delegated_device_approvals(operation_id),
    project_id UUID NOT NULL REFERENCES projects(id),
    device_id UUID NOT NULL,
    human_principal_id UUID NOT NULL REFERENCES auth_principals(id),
    local_memory_id UUID NOT NULL,
    request_sha256 TEXT NOT NULL CHECK(request_sha256 ~ '^[0-9a-f]{64}$'),
    receipt_json JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(device_id, project_id, local_memory_id)
);
