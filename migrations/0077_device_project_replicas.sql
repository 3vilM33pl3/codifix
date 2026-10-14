-- Private device replication: no shared publication, PDS or execution authority.
CREATE TABLE device_replica_heads (
    scope JSONB PRIMARY KEY,
    sequence BIGINT NOT NULL DEFAULT 0 CHECK (sequence >= 0)
);
CREATE TABLE device_replica_sources (
    scope JSONB NOT NULL REFERENCES device_replica_heads(scope),
    record_key TEXT NOT NULL,
    record JSONB NOT NULL,
    PRIMARY KEY (scope, record_key)
);
CREATE TABLE device_replica_events (
    scope JSONB NOT NULL REFERENCES device_replica_heads(scope),
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    entry JSONB NOT NULL,
    PRIMARY KEY (scope, sequence)
);
CREATE TABLE device_replica_projections (
    subscription UUID PRIMARY KEY,
    device UUID NOT NULL,
    scope JSONB NOT NULL,
    cursor BIGINT NOT NULL DEFAULT 0 CHECK (cursor >= 0),
    observed_at TIMESTAMPTZ
);
CREATE TABLE device_replica_records (
    subscription UUID NOT NULL REFERENCES device_replica_projections(subscription),
    record_key TEXT NOT NULL,
    record JSONB NOT NULL,
    PRIMARY KEY (subscription, record_key)
);
CREATE TABLE device_replica_pages (
    subscription UUID NOT NULL REFERENCES device_replica_projections(subscription),
    nonce UUID NOT NULL,
    digest TEXT NOT NULL,
    next_cursor BIGINT NOT NULL,
    PRIMARY KEY (subscription, nonce)
);
