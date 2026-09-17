-- Supervisor authority is opt-in. No worker, provider, role or permission is
-- enabled by this migration. All four seeded authorities have no active policy.
CREATE TABLE supervisor_authorities (
    area text PRIMARY KEY CHECK (area IN ('trust','homelab','workshop','general')),
    current_revision uuid
);
INSERT INTO supervisor_authorities(area) VALUES ('trust'),('homelab'),('workshop'),('general');

CREATE TABLE supervisor_policies (
    area text NOT NULL REFERENCES supervisor_authorities(area),
    revision uuid NOT NULL CHECK (revision <> '00000000-0000-0000-0000-000000000000'),
    enabled boolean NOT NULL DEFAULT false,
    revoked boolean NOT NULL DEFAULT false,
    model text NOT NULL DEFAULT 'gpt-5.6-sol' CHECK (model = 'gpt-5.6-sol'),
    daily_usd_micros bigint NOT NULL DEFAULT 2500000 CHECK (daily_usd_micros BETWEEN 0 AND 2500000),
    daily_calls integer NOT NULL DEFAULT 20 CHECK (daily_calls BETWEEN 0 AND 20),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (area,revision),
    CHECK (NOT revoked OR NOT enabled)
);
ALTER TABLE supervisor_authorities ADD FOREIGN KEY (area,current_revision)
    REFERENCES supervisor_policies(area,revision);

CREATE FUNCTION supervisor_guard_policy() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'supervisor policy history is retained'; END IF;
    IF (NEW.area,NEW.revision,NEW.model,NEW.created_at) IS DISTINCT FROM
       (OLD.area,OLD.revision,OLD.model,OLD.created_at)
       OR (OLD.revoked AND NOT NEW.revoked)
       OR NEW.daily_usd_micros > OLD.daily_usd_micros
       OR NEW.daily_calls > OLD.daily_calls THEN
        RAISE EXCEPTION 'supervisor policy identity, revocation and reductions are permanent';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER supervisor_policy_guard BEFORE UPDATE OR DELETE ON supervisor_policies
    FOR EACH ROW EXECUTE FUNCTION supervisor_guard_policy();

CREATE TABLE supervisor_actions (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    area text NOT NULL,
    source_event uuid NOT NULL CHECK (source_event <> '00000000-0000-0000-0000-000000000000'),
    kind text NOT NULL CHECK (kind IN ('health_check','stale_knowledge_review','ask_question','propose_correction','review_release','install_maintenance')),
    policy_revision uuid NOT NULL,
    source_digest bytea NOT NULL CHECK (octet_length(source_digest) = 32),
    state text NOT NULL DEFAULT 'observed' CHECK (state IN ('observed','proposed','authorized','queued','running','verified','denied','failed','expired','cancelled','uncertain')),
    fence bigint NOT NULL DEFAULT 0 CHECK (fence >= 0),
    worker uuid,
    lease_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (area,source_event,kind,policy_revision),
    UNIQUE (area,id),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision),
    CHECK ((state = 'running' AND worker IS NOT NULL AND lease_until IS NOT NULL)
        OR (state <> 'running' AND worker IS NULL AND lease_until IS NULL))
);
CREATE INDEX supervisor_actions_queue ON supervisor_actions(area,state,created_at);

-- Independent area/day lock rows. No client-supplied accounting clock or reset.
CREATE TABLE supervisor_budget_days (
    area text NOT NULL REFERENCES supervisor_authorities(area),
    day date NOT NULL,
    PRIMARY KEY (area,day)
);
CREATE TABLE supervisor_attempts (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    area text NOT NULL,
    action_id uuid NOT NULL,
    policy_revision uuid NOT NULL,
    fence bigint NOT NULL CHECK (fence > 0),
    day date NOT NULL,
    model text NOT NULL CHECK (model = 'gpt-5.6-sol'),
    request_digest bytea NOT NULL CHECK (octet_length(request_digest) = 32),
    worst_usd_micros bigint NOT NULL CHECK (worst_usd_micros BETWEEN 1 AND 2500000),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (area,action_id) REFERENCES supervisor_actions(area,id),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision),
    FOREIGN KEY (area,day) REFERENCES supervisor_budget_days(area,day)
);
CREATE INDEX supervisor_attempts_day ON supervisor_attempts(area,day);
CREATE INDEX supervisor_attempts_action ON supervisor_attempts(action_id);
CREATE TABLE supervisor_attempt_states (
    attempt_id uuid PRIMARY KEY REFERENCES supervisor_attempts(id),
    state text NOT NULL DEFAULT 'reserved' CHECK (state IN ('reserved','in_flight','uncertain','settled','released_before_send')),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE supervisor_settlements (
    attempt_id uuid PRIMARY KEY REFERENCES supervisor_attempts(id),
    -- Entire u64 contract supported, without floating point or lossy casts.
    actual_usd_micros numeric(20,0) NOT NULL CHECK (actual_usd_micros BETWEEN 0 AND 18446744073709551615),
    evidence_digest bytea NOT NULL CHECK (octet_length(evidence_digest) = 32),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE supervisor_action_results (
    action_id uuid PRIMARY KEY REFERENCES supervisor_actions(id),
    outcome text NOT NULL CHECK (outcome IN ('verified','failed')),
    evidence_digest bytea NOT NULL CHECK (octet_length(evidence_digest) = 32),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
-- Deliberately no arbitrary JSON, prompts, response bodies, URLs or error prose.
CREATE TABLE supervisor_journal_heads (
    area text PRIMARY KEY REFERENCES supervisor_authorities(area)
);
INSERT INTO supervisor_journal_heads SELECT area FROM supervisor_authorities;
CREATE TABLE supervisor_journal (
    sequence bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    area text NOT NULL REFERENCES supervisor_authorities(area),
    action_id uuid REFERENCES supervisor_actions(id),
    attempt_id uuid REFERENCES supervisor_attempts(id),
    policy_revision uuid NOT NULL,
    event text NOT NULL CHECK (event IN ('policy_installed','policy_enabled','policy_paused','policy_revoked','policy_reduced','observed','proposed','authorized','queued','claimed','renewed','verified','failed','denied','expired','cancelled','uncertain','reserved','begun','released_before_send','settled','settled_over_bound')),
    evidence_digest bytea CHECK (evidence_digest IS NULL OR octet_length(evidence_digest) = 32),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision)
);
CREATE INDEX supervisor_journal_area ON supervisor_journal(area,sequence);

CREATE FUNCTION supervisor_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'supervisor evidence is append-only'; END $$;
CREATE TRIGGER supervisor_attempt_immutable BEFORE UPDATE OR DELETE ON supervisor_attempts
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
CREATE TRIGGER supervisor_settlement_immutable BEFORE UPDATE OR DELETE ON supervisor_settlements
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
CREATE TRIGGER supervisor_result_immutable BEFORE UPDATE OR DELETE ON supervisor_action_results
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
CREATE TRIGGER supervisor_journal_immutable BEFORE UPDATE OR DELETE ON supervisor_journal
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();

CREATE FUNCTION supervisor_guard_attempt_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'supervisor attempts are retained'; END IF;
    IF NEW.attempt_id <> OLD.attempt_id OR NOT (
        (OLD.state = 'reserved' AND NEW.state IN ('in_flight','uncertain','released_before_send')) OR
        (OLD.state = 'in_flight' AND NEW.state IN ('uncertain','settled')) OR
        (OLD.state = 'uncertain' AND NEW.state = 'settled')
    ) THEN RAISE EXCEPTION 'invalid supervisor attempt transition'; END IF;
    IF NEW.state = 'settled' AND NOT EXISTS (
        SELECT 1 FROM supervisor_settlements WHERE attempt_id = NEW.attempt_id
    ) THEN RAISE EXCEPTION 'settlement evidence required'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER supervisor_attempt_state_guard BEFORE UPDATE OR DELETE ON supervisor_attempt_states
    FOR EACH ROW EXECUTE FUNCTION supervisor_guard_attempt_state();

CREATE FUNCTION supervisor_guard_action() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'supervisor actions are retained'; END IF;
    IF (NEW.id,NEW.area,NEW.source_event,NEW.kind,NEW.policy_revision,NEW.source_digest,NEW.created_at)
       IS DISTINCT FROM (OLD.id,OLD.area,OLD.source_event,OLD.kind,OLD.policy_revision,OLD.source_digest,OLD.created_at)
       OR NEW.fence < OLD.fence THEN RAISE EXCEPTION 'immutable supervisor action identity'; END IF;
    IF NEW.state <> OLD.state AND NOT (
        (OLD.state = 'observed' AND NEW.state = 'proposed') OR
        (OLD.state = 'proposed' AND NEW.state = 'authorized') OR
        (OLD.state = 'authorized' AND NEW.state = 'queued') OR
        (OLD.state = 'queued' AND NEW.state = 'running') OR
        (OLD.state IN ('observed','proposed','authorized','queued') AND NEW.state IN ('denied','cancelled','expired')) OR
        (OLD.state = 'running' AND NEW.state IN ('verified','failed','uncertain')) OR
        (OLD.state = 'uncertain' AND NEW.state IN ('verified','failed'))
    ) THEN RAISE EXCEPTION 'invalid supervisor action transition'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER supervisor_action_guard BEFORE UPDATE OR DELETE ON supervisor_actions
    FOR EACH ROW EXECUTE FUNCTION supervisor_guard_action();
