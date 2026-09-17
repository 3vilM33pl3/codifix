-- Explicit LOCAL identity binding only. No row, policy or worker is enabled.
-- A copied database also copies this identity; deployment must fence clones.
CREATE TABLE supervisor_instance_binding (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    instance_id uuid NOT NULL UNIQUE CHECK (instance_id <> '00000000-0000-0000-0000-000000000000'),
    area text NOT NULL UNIQUE REFERENCES supervisor_authorities(area),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (instance_id,area)
);
CREATE TRIGGER supervisor_instance_binding_immutable BEFORE UPDATE OR DELETE ON supervisor_instance_binding
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();

-- Private work scope is fixed before any token-counting disclosure or send.
-- Retained project identity is not an authorization grant and does not alter
-- project deletion semantics. New dispatch separately checks current existence.
CREATE TABLE supervisor_model_contexts (
    action_id uuid PRIMARY KEY REFERENCES supervisor_actions(id),
    instance_id uuid NOT NULL,
    area text NOT NULL,
    policy_revision uuid NOT NULL,
    project_id uuid NOT NULL CHECK (project_id <> '00000000-0000-0000-0000-000000000000'),
    request_digest bytea NOT NULL CHECK (octet_length(request_digest) = 32),
    evidence_digests bytea[] NOT NULL CHECK (supervisor_proposal_evidence_valid(evidence_digests)),
    context_digest bytea NOT NULL CHECK (octet_length(context_digest) = 32),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (instance_id,area) REFERENCES supervisor_instance_binding(instance_id,area),
    FOREIGN KEY (area,action_id) REFERENCES supervisor_actions(area,id),
    FOREIGN KEY (area,policy_revision) REFERENCES supervisor_policies(area,revision)
);
CREATE TRIGGER supervisor_model_context_immutable BEFORE UPDATE OR DELETE ON supervisor_model_contexts
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_model_contexts FROM PUBLIC;

ALTER TABLE supervisor_journal DROP CONSTRAINT supervisor_journal_event_check;
ALTER TABLE supervisor_journal ADD CONSTRAINT supervisor_journal_event_check CHECK (event IN (
    'policy_installed','policy_enabled','policy_paused','policy_revoked','policy_reduced',
    'observed','proposed','authorized','queued','claimed','renewed','verified','failed',
    'denied','expired','cancelled','uncertain','reserved','begun','released_before_send',
    'settled','settled_over_bound','model_context_bound'
));
