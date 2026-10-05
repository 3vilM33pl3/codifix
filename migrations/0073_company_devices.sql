-- Company enrollment has no owner-local project or credential foreign key.
-- Retain every exact remote intent; local activation is a separate transaction.
CREATE TABLE onboarding_company_device_intents (
    request_id UUID PRIMARY KEY REFERENCES onboarding_device_requests(request_id) ON DELETE RESTRICT,
    company_id UUID NOT NULL,
    device_id UUID NOT NULL,
    human_id UUID NOT NULL REFERENCES auth_principals(id) ON DELETE RESTRICT,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    approve_sha256 TEXT NOT NULL CHECK(approve_sha256 ~ '^[0-9a-f]{64}$'),
    approve_json JSONB NOT NULL,
    source_sha256 TEXT NOT NULL CHECK(source_sha256 ~ '^[0-9a-f]{64}$'),
    registry_sha256 TEXT NOT NULL CHECK(registry_sha256 ~ '^[0-9a-f]{64}$'),
    grant_until TIMESTAMPTZ NOT NULL,
    bindings_json JSONB NOT NULL CHECK(jsonb_typeof(bindings_json)='array' AND jsonb_array_length(bindings_json)<=32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER onboarding_company_device_intent_retained BEFORE UPDATE OR DELETE ON onboarding_company_device_intents
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();

CREATE TABLE onboarding_company_device_outcomes (
    operation_id UUID PRIMARY KEY,
    request_id UUID NOT NULL REFERENCES onboarding_company_device_intents(request_id) ON DELETE RESTRICT,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','uncertain','confirmed')),
    receipt_json JSONB,
    bootstrap_json JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK(state<>'confirmed' OR (receipt_json IS NOT NULL AND bootstrap_json IS NOT NULL))
);
CREATE FUNCTION onboarding_company_device_outcome_retained() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' OR (NEW.operation_id,NEW.request_id) IS DISTINCT FROM (OLD.operation_id,OLD.request_id)
       OR (OLD.receipt_json IS NOT NULL AND NEW.receipt_json IS DISTINCT FROM OLD.receipt_json)
       OR (OLD.bootstrap_json IS NOT NULL AND NEW.bootstrap_json IS DISTINCT FROM OLD.bootstrap_json)
       OR (OLD.state='confirmed' AND NEW.state<>'confirmed') THEN
        RAISE EXCEPTION 'company device outcomes and confirmed trust are retained';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER onboarding_company_device_outcome_retained BEFORE UPDATE OR DELETE ON onboarding_company_device_outcomes
    FOR EACH ROW EXECUTE FUNCTION onboarding_company_device_outcome_retained();

CREATE TABLE onboarding_company_device_selections (
    device_id UUID NOT NULL REFERENCES onboarding_devices(id) ON DELETE RESTRICT,
    request_id UUID NOT NULL REFERENCES onboarding_company_device_intents(request_id) ON DELETE RESTRICT,
    area TEXT NOT NULL CHECK(area IN ('workshop','homelab','general')),
    owner_instance_id UUID NOT NULL,
    project_id UUID NOT NULL,
    manifest_json JSONB NOT NULL,
    approval_json JSONB NOT NULL,
    bootstrap_json JSONB NOT NULL,
    registry_sha256 TEXT NOT NULL CHECK(registry_sha256 ~ '^[0-9a-f]{64}$'),
    grant_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(device_id,area,owner_instance_id,project_id)
);
CREATE TRIGGER onboarding_company_device_selection_retained BEFORE UPDATE OR DELETE ON onboarding_company_device_selections
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
