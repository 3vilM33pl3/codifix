-- Local user drafts/outbox, separate from the read-only delivered projection.
-- No memberships, replica records, authoritative memories or cursors are changed.
CREATE TABLE laptop_contribution_drafts (
    operation_id UUID PRIMARY KEY,
    binding JSONB NOT NULL,
    request_sha256 TEXT NOT NULL CHECK (request_sha256 ~ '^[0-9a-f]{64}$'),
    draft JSONB NOT NULL,
    metadata JSONB NOT NULL,
    generation BIGINT NOT NULL DEFAULT 0 CHECK (generation >= 0),
    state TEXT NOT NULL CHECK (state IN ('draft','queued','uncertain','accepted','blocked')),
    stored_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (metadata->>'state' = state),
    CHECK ((metadata->>'generation')::BIGINT = generation),
    CHECK (metadata->>'operation_id' = operation_id::TEXT),
    CHECK (metadata->'binding' = binding),
    CHECK (metadata->>'request_sha256' = request_sha256),
    CHECK ((metadata->'receipt' != 'null'::JSONB) = (state = 'accepted')),
    CHECK ((metadata->'blocked_reason' != 'null'::JSONB) = (state = 'blocked'))
);
CREATE INDEX laptop_contribution_binding ON laptop_contribution_drafts USING HASH(binding);
CREATE FUNCTION laptop_contribution_guard() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'local contribution identities are durable';
    END IF;
    IF OLD.state = 'accepted'
       OR NEW.operation_id != OLD.operation_id OR NEW.binding != OLD.binding
       OR NEW.request_sha256 != OLD.request_sha256 OR NEW.draft != OLD.draft
       OR NEW.stored_at != OLD.stored_at OR NEW.generation != OLD.generation + 1
       OR NOT ((OLD.state = 'draft' AND NEW.state IN ('queued','blocked'))
           OR (OLD.state = 'queued' AND NEW.state IN ('uncertain','accepted','blocked'))
           OR (OLD.state = 'uncertain' AND NEW.state IN ('queued','accepted','blocked'))
           OR (OLD.state = 'blocked' AND NEW.state = 'queued'))
       OR (NEW.metadata - ARRAY['generation','state','blocked_reason','receipt'])
          != (OLD.metadata - ARRAY['generation','state','blocked_reason','receipt']) THEN
        RAISE EXCEPTION 'immutable contribution binding or invalid transition';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER laptop_contribution_drafts_guard BEFORE UPDATE OR DELETE ON laptop_contribution_drafts
FOR EACH ROW EXECUTE FUNCTION laptop_contribution_guard();
CREATE TABLE laptop_contribution_transitions (
    operation_id UUID NOT NULL REFERENCES laptop_contribution_drafts(operation_id),
    generation BIGINT NOT NULL CHECK (generation >= 0),
    entry JSONB NOT NULL,
    PRIMARY KEY(operation_id,generation)
);
CREATE TRIGGER laptop_contribution_transitions_immutable BEFORE UPDATE OR DELETE ON laptop_contribution_transitions
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
