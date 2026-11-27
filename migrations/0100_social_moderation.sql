-- A receiving companion controls its own contacts, independent of the sender.
CREATE TABLE agency_social_moderation (
    instance_id uuid NOT NULL,
    peer_id text NOT NULL,
    banned_until timestamptz,
    PRIMARY KEY (instance_id, peer_id)
);

CREATE TABLE agency_social_used_tokens (
    instance_id uuid NOT NULL,
    peer_id text NOT NULL,
    jti text NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (instance_id, peer_id, jti)
);
CREATE INDEX agency_social_token_expiry ON agency_social_used_tokens (expires_at);
