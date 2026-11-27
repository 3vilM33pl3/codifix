ALTER TABLE agency_mandates ADD COLUMN used_turns BIGINT NOT NULL DEFAULT 0 CHECK(used_turns>=0);
ALTER TABLE agency_mandates ADD COLUMN reserved_turns BIGINT NOT NULL DEFAULT 0 CHECK(reserved_turns>=0);
ALTER TABLE agency_tasks ADD COLUMN used_microusd BIGINT NOT NULL DEFAULT 0 CHECK(used_microusd>=0);
ALTER TABLE agency_tasks ADD COLUMN used_calls BIGINT NOT NULL DEFAULT 0 CHECK(used_calls>=0);
ALTER TABLE agency_tasks ADD COLUMN used_turns BIGINT NOT NULL DEFAULT 0 CHECK(used_turns>=0);
ALTER TABLE agency_tasks ADD COLUMN request_digest TEXT NOT NULL DEFAULT '';
