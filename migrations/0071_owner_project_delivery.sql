-- Origin registration is a PATCH. Retain the complete set (including disabled
-- streams), not just currently enabled sources, across restarts and lost replies.
CREATE TABLE onboarding_owner_project_heads (
    project_id UUID PRIMARY KEY REFERENCES projects(id) ON DELETE RESTRICT,
    scope JSONB NOT NULL,
    revision BIGINT NOT NULL DEFAULT 0 CHECK(revision >= 0),
    policy_revision BIGINT NOT NULL DEFAULT 0 CHECK(policy_revision >= 0),
    fingerprint BYTEA,
    reviews JSONB NOT NULL DEFAULT '[]',
    pending_operation UUID
);
CREATE TABLE onboarding_owner_project_operations (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES onboarding_owner_project_heads(project_id) ON DELETE RESTRICT,
    request JSONB NOT NULL,
    request_sha256 BYTEA NOT NULL CHECK(octet_length(request_sha256)=32),
    fingerprint BYTEA NOT NULL CHECK(octet_length(fingerprint)=32),
    receipt JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    confirmed_at TIMESTAMPTZ
);
ALTER TABLE onboarding_owner_project_heads ADD CONSTRAINT onboarding_owner_pending
    FOREIGN KEY(pending_operation) REFERENCES onboarding_owner_project_operations(id) ON DELETE RESTRICT;
CREATE FUNCTION onboarding_owner_intent_retained() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['receipt','confirmed_at']) IS DISTINCT FROM
       (to_jsonb(OLD)-ARRAY['receipt','confirmed_at']) OR
       (OLD.receipt IS NOT NULL AND NEW.receipt IS DISTINCT FROM OLD.receipt) THEN
        RAISE EXCEPTION 'owner registration intent and confirmed receipt are retained';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER onboarding_owner_intent_retained BEFORE UPDATE OR DELETE ON onboarding_owner_project_operations
    FOR EACH ROW EXECUTE FUNCTION onboarding_owner_intent_retained();
CREATE TABLE onboarding_owner_snapshot_requests (
    device_id UUID NOT NULL REFERENCES onboarding_devices(id) ON DELETE RESTRICT,
    nonce UUID NOT NULL,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    request JSONB NOT NULL,
    PRIMARY KEY(device_id,nonce)
);
CREATE TRIGGER onboarding_owner_snapshot_request_retained BEFORE UPDATE OR DELETE ON onboarding_owner_snapshot_requests
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
