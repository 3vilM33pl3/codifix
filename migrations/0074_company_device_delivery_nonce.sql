-- A company device nonce binds the complete selected area/owner/project request,
-- even when two distinct owners preserve the same source project UUID.
CREATE TABLE onboarding_company_device_delivery_requests (
    device_id UUID NOT NULL REFERENCES onboarding_devices(id) ON DELETE RESTRICT,
    nonce UUID NOT NULL,
    request_json JSONB NOT NULL,
    PRIMARY KEY(device_id,nonce)
);
CREATE TRIGGER onboarding_company_device_delivery_request_retained
    BEFORE UPDATE OR DELETE ON onboarding_company_device_delivery_requests
    FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
