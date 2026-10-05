-- New intents bind exact provider input without retaining personal payloads.
-- Existing intents cannot be reconstructed: NULL means legacy/unknown, never
-- an invented provider correlation or request fingerprint.
ALTER TABLE onboarding_idp_intents
    ADD COLUMN provider_origin TEXT,
    ADD COLUMN provider_correlation TEXT CHECK (length(provider_correlation) BETWEEN 1 AND 64),
    ADD COLUMN request_sha256 TEXT CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT onboarding_idp_binding_complete CHECK (
        num_nonnulls(provider_origin,provider_correlation,request_sha256) IN (0,3)
    );
