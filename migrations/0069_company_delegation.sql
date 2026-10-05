-- A direct grant may only be claimed when no local row already exists.
-- Ownership remains after revocation; ordinary local helpers must not erase it.
CREATE TABLE onboarding_company_grants (
    project_id UUID NOT NULL REFERENCES projects(id),
    principal_id UUID NOT NULL REFERENCES auth_principals(id),
    company_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    role TEXT NOT NULL CHECK (role IN ('reader','writer','operator')),
    membership_id UUID,
    active BOOLEAN NOT NULL,
    PRIMARY KEY(project_id,principal_id),
    CHECK (active = (membership_id IS NOT NULL))
);

CREATE TABLE onboarding_company_receipts (
    company_id UUID NOT NULL,
    operation_id UUID NOT NULL,
    request_sha256 TEXT NOT NULL CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    request_json JSONB NOT NULL,
    receipt_json JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY(company_id,operation_id)
);

CREATE TABLE onboarding_company_intents (
    operation_id UUID PRIMARY KEY,
    company_id UUID NOT NULL,
    owner_instance_id UUID NOT NULL,
    request_sha256 TEXT NOT NULL CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    request_json JSONB NOT NULL,
    owner_origin TEXT NOT NULL,
    actor_id UUID NOT NULL REFERENCES auth_principals(id),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE onboarding_company_outcomes (
    operation_id UUID PRIMARY KEY REFERENCES onboarding_company_intents(operation_id),
    outcome TEXT NOT NULL CHECK (outcome IN ('confirmed','rejected','uncertain')),
    receipt_json JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((outcome='confirmed') = (receipt_json IS NOT NULL))
);

CREATE FUNCTION protect_company_onboarding_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'company onboarding intents and receipts are immutable';
END $$;
CREATE TRIGGER protect_company_receipts BEFORE UPDATE OR DELETE ON onboarding_company_receipts
    FOR EACH ROW EXECUTE FUNCTION protect_company_onboarding_history();
CREATE TRIGGER protect_company_intents BEFORE UPDATE OR DELETE ON onboarding_company_intents
    FOR EACH ROW EXECUTE FUNCTION protect_company_onboarding_history();

CREATE FUNCTION protect_company_grant_ownership() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' OR NEW.project_id<>OLD.project_id OR NEW.principal_id<>OLD.principal_id
        OR NEW.company_id<>OLD.company_id OR NEW.revision<>OLD.revision+1 THEN
        RAISE EXCEPTION 'company grant ownership is permanent and revisioned';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER protect_company_grants BEFORE UPDATE OR DELETE ON onboarding_company_grants
    FOR EACH ROW EXECUTE FUNCTION protect_company_grant_ownership();
