-- Reviewed metadata only. Catalog rows do not authorize offline/export/delivery.
CREATE TABLE onboarding_project_versions (
    project_id UUID NOT NULL REFERENCES projects(id),
    revision BIGINT NOT NULL CHECK (revision > 0),
    area TEXT NOT NULL CHECK (area IN ('general','work','personal','trust')),
    owner_instance_id UUID NOT NULL CHECK (owner_instance_id <> '00000000-0000-0000-0000-000000000000'),
    offline_allowed BOOLEAN NOT NULL,
    entry_json JSONB NOT NULL,
    reviewed_by UUID NOT NULL REFERENCES auth_principals(id),
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (project_id,revision),
    CHECK (area <> 'trust' OR NOT offline_allowed)
);
CREATE TABLE onboarding_devices (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES auth_principals(id),
    name TEXT NOT NULL,
    platform TEXT NOT NULL,
    service_token_id UUID REFERENCES auth_service_tokens(id),
    web_session_id UUID REFERENCES auth_web_sessions(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);
-- External effects have an immutable intent and a separate immutable result.
-- A missing result is uncertain/pending, never silently replayed as success.
CREATE TABLE onboarding_idp_intents (
    id UUID PRIMARY KEY,
    actor_principal_id UUID NOT NULL REFERENCES auth_principals(id),
    operation TEXT NOT NULL CHECK (operation IN ('create','invite','disable')),
    provider_user_id BIGINT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE onboarding_idp_outcomes (
    intent_id UUID PRIMARY KEY REFERENCES onboarding_idp_intents(id),
    outcome TEXT NOT NULL CHECK (outcome IN ('success','error','uncertain')),
    provider_user_id BIGINT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE FUNCTION onboarding_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'onboarding review and operation evidence is immutable'; END $$;
CREATE TRIGGER onboarding_project_versions_immutable BEFORE UPDATE OR DELETE ON onboarding_project_versions
    FOR EACH ROW EXECUTE FUNCTION onboarding_immutable();
CREATE TRIGGER onboarding_idp_intents_immutable BEFORE UPDATE OR DELETE ON onboarding_idp_intents
    FOR EACH ROW EXECUTE FUNCTION onboarding_immutable();
CREATE TRIGGER onboarding_idp_outcomes_immutable BEFORE UPDATE OR DELETE ON onboarding_idp_outcomes
    FOR EACH ROW EXECUTE FUNCTION onboarding_immutable();
