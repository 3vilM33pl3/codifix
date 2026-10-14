-- Retain identity-only authority observations after owner retention removes
-- projected content. Otherwise an acknowledged local copy can resurrect.
CREATE TABLE device_replica_owner_watermarks (
    subscription UUID NOT NULL REFERENCES device_replica_projections(subscription),
    canonical_id UUID NOT NULL,
    version BIGINT NOT NULL DEFAULT 0 CHECK(version >= 0),
    inactive BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY(subscription, canonical_id)
);
INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,version)
SELECT subscription,(record->'value'->>'canonical_id')::uuid,
       max((record->'value'->>'version')::bigint)
FROM device_replica_records WHERE record->>'kind'='memory_version'
GROUP BY subscription,(record->'value'->>'canonical_id')::uuid;
INSERT INTO device_replica_owner_watermarks(subscription,canonical_id,inactive)
SELECT subscription,(record->'value'->>'canonical_id')::uuid,
       record->'value'->>'status'<>'active'
FROM device_replica_records WHERE record->>'kind'='canonical_state'
ON CONFLICT(subscription,canonical_id) DO UPDATE SET inactive=EXCLUDED.inactive;
