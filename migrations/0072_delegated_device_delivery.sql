-- A remote device is an audience, not an owner-local credential or project.
-- Keep the local-device FK in 0071 unchanged; retain separate company bindings.
CREATE TABLE onboarding_delegated_device_approvals (
    operation_id UUID PRIMARY KEY,
    company_id UUID NOT NULL,
    device_id UUID NOT NULL,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    human_principal_id UUID NOT NULL REFERENCES auth_principals(id) ON DELETE RESTRICT,
    request_sha256 BYTEA NOT NULL CHECK(octet_length(request_sha256)=32),
    request_json JSONB NOT NULL,
    receipt_json JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX onboarding_delegated_device_audience
    ON onboarding_delegated_device_approvals(company_id,device_id,project_id);
CREATE TRIGGER onboarding_delegated_approval_retained
    BEFORE UPDATE OR DELETE ON onboarding_delegated_device_approvals
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();

CREATE TABLE onboarding_delegated_snapshot_requests (
    company_id UUID NOT NULL,
    device_id UUID NOT NULL,
    nonce UUID NOT NULL,
    approval_id UUID NOT NULL REFERENCES onboarding_delegated_device_approvals(operation_id) ON DELETE RESTRICT,
    request_json JSONB NOT NULL,
    PRIMARY KEY(company_id,device_id,nonce)
);
CREATE TRIGGER onboarding_delegated_snapshot_retained
    BEFORE UPDATE OR DELETE ON onboarding_delegated_snapshot_requests
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
