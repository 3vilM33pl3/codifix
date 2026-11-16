-- Desktop login uses the existing OIDC callback but never shares browser secrets.
ALTER TABLE auth_oidc_flows ADD COLUMN desktop_context JSONB;
ALTER TABLE auth_oidc_flows ADD CONSTRAINT auth_oidc_flow_target
    CHECK (desktop_context IS NULL OR return_to IS NULL);
ALTER TABLE auth_web_sessions ADD COLUMN session_kind TEXT NOT NULL DEFAULT 'browser'
    CHECK (session_kind IN ('browser', 'desktop'));
CREATE TABLE auth_desktop_codes (
    code_hash BYTEA PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES auth_principals(id) ON DELETE CASCADE,
    code_challenge TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_auth_desktop_codes_expiry ON auth_desktop_codes(expires_at);
