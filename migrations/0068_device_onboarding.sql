-- Human-reviewed enrollment of a client-generated credential digest only.
ALTER TABLE onboarding_devices ADD COLUMN machine_principal_id UUID REFERENCES auth_principals(id);
CREATE UNIQUE INDEX onboarding_devices_machine ON onboarding_devices(machine_principal_id) WHERE machine_principal_id IS NOT NULL;
CREATE UNIQUE INDEX onboarding_devices_token ON onboarding_devices(service_token_id) WHERE service_token_id IS NOT NULL;
CREATE TABLE onboarding_device_requests (
    request_id UUID PRIMARY KEY,
    device_id UUID NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 128),
    platform TEXT NOT NULL CHECK(platform IN ('macos','linux','windows')),
    credential_hash BYTEA NOT NULL CHECK(octet_length(credential_hash)=32),
    poll_hash BYTEA NOT NULL CHECK(octet_length(poll_hash)=32 AND poll_hash<>credential_hash),
    user_code TEXT NOT NULL UNIQUE CHECK(user_code ~ '^[A-Z0-9]{4}-[A-Z0-9]{4}$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now()+interval '10 minutes',
    last_poll_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','denied')),
    approved_by UUID REFERENCES auth_principals(id),
    machine_principal_id UUID REFERENCES auth_principals(id),
    selection_json JSONB,
    grants_json JSONB,
    decided_at TIMESTAMPTZ
);
CREATE INDEX onboarding_device_requests_outstanding ON onboarding_device_requests(expires_at) WHERE status='pending';
CREATE FUNCTION onboarding_device_request_binding() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' OR
       (NEW.request_id,NEW.device_id,NEW.name,NEW.platform,NEW.credential_hash,NEW.poll_hash,NEW.user_code,NEW.created_at,NEW.expires_at)
       IS DISTINCT FROM
       (OLD.request_id,OLD.device_id,OLD.name,OLD.platform,OLD.credential_hash,OLD.poll_hash,OLD.user_code,OLD.created_at,OLD.expires_at)
       OR (OLD.status<>'pending' AND
       (NEW.status,NEW.approved_by,NEW.machine_principal_id,NEW.selection_json,NEW.grants_json,NEW.decided_at)
       IS DISTINCT FROM
       (OLD.status,OLD.approved_by,OLD.machine_principal_id,OLD.selection_json,OLD.grants_json,OLD.decided_at)) THEN
       RAISE EXCEPTION 'device enrollment binding and decisions are immutable';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER onboarding_device_requests_binding BEFORE UPDATE OR DELETE ON onboarding_device_requests
    FOR EACH ROW EXECUTE FUNCTION onboarding_device_request_binding();
