-- Exact private question consent, separate from operational feeds and corpus.
-- Its request UUID is also the stable action/source-event UUID. A queue replay
-- must never allocate another attempt or silently change the selected profile.
CREATE TABLE supervisor_questions (
    request_id uuid PRIMARY KEY CHECK (request_id <> '00000000-0000-0000-0000-000000000000'),
    project_id uuid NOT NULL REFERENCES projects(id),
    instance_id uuid NOT NULL,
    profile_digest bytea NOT NULL CHECK (octet_length(profile_digest)=32),
    request_digest bytea NOT NULL CHECK (octet_length(request_digest)=32),
    policy_revision uuid NOT NULL,
    principal_id uuid NOT NULL REFERENCES auth_principals(id),
    token_id uuid,
    session_id uuid,
    question text NOT NULL CHECK (octet_length(question) BETWEEN 1 AND 4096),
    attempt_id uuid NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL DEFAULT clock_timestamp() + interval '10 minutes',
    CHECK ((token_id IS NOT NULL)::integer + (session_id IS NOT NULL)::integer = 1),
    CHECK (expires_at > created_at)
);
CREATE INDEX supervisor_questions_project_page ON supervisor_questions(project_id,request_id);
CREATE INDEX supervisor_questions_pending ON supervisor_questions(created_at,request_id);
CREATE TRIGGER supervisor_question_immutable BEFORE UPDATE OR DELETE ON supervisor_questions
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_questions FROM PUBLIC;
CREATE TABLE supervisor_question_denials (
    request_id uuid PRIMARY KEY REFERENCES supervisor_questions(request_id),
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER supervisor_question_denial_immutable BEFORE UPDATE OR DELETE ON supervisor_question_denials
    FOR EACH ROW EXECUTE FUNCTION supervisor_immutable();
REVOKE ALL ON supervisor_question_denials FROM PUBLIC;
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check
    CHECK (target_migration IN (56,57,58,59,60));
