-- Child credentials are independent from the company device bearer. Scope is
-- immutable metadata only; no ordinary membership or permission is issued.
CREATE TABLE onboarding_company_agents (
    agent_id UUID PRIMARY KEY,
    request_id UUID NOT NULL UNIQUE,
    device_id UUID NOT NULL REFERENCES onboarding_devices(id) ON DELETE RESTRICT,
    human_id UUID NOT NULL REFERENCES auth_principals(id) ON DELETE RESTRICT,
    principal_id UUID NOT NULL UNIQUE REFERENCES auth_principals(id) ON DELETE RESTRICT,
    token_id UUID NOT NULL UNIQUE REFERENCES auth_service_tokens(id) ON DELETE RESTRICT,
    company_id UUID NOT NULL,
    request_sha256 TEXT NOT NULL CHECK(request_sha256 ~ '^[0-9a-f]{64}$'),
    request_json JSONB NOT NULL,
    receipt_json JSONB NOT NULL,
    registry_sha256 TEXT NOT NULL CHECK(registry_sha256 ~ '^[0-9a-f]{64}$'),
    credential_hash BYTEA NOT NULL CHECK(octet_length(credential_hash)=32),
    expires_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX onboarding_company_agents_device_idx ON onboarding_company_agents(device_id,agent_id);
CREATE TRIGGER onboarding_company_agent_binding_retained BEFORE UPDATE OR DELETE ON onboarding_company_agents
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
CREATE TABLE onboarding_company_agent_revocations (
    agent_id UUID PRIMARY KEY REFERENCES onboarding_company_agents(agent_id) ON DELETE RESTRICT,
    revoked_by UUID NOT NULL REFERENCES auth_principals(id) ON DELETE RESTRICT,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER onboarding_company_agent_revocation_retained BEFORE UPDATE OR DELETE ON onboarding_company_agent_revocations
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
