-- Apply only in an independent Workshop database, after schema migration and
-- creation of a non-superuser workshop_worker role. No CONNECT, credentials,
-- membership, policy activation or data migration is performed here.
GRANT USAGE ON SCHEMA public TO workshop_worker;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO workshop_worker;
-- PostgreSQL row locks require UPDATE on at least one column, even for KEY
-- SHARE. These grants do not give the worker write access to source contents.
GRANT UPDATE(id) ON projects,auth_principals,auth_project_memberships,memory_entries TO workshop_worker;
GRANT UPDATE(id) ON auth_service_tokens,auth_web_sessions TO workshop_worker;
GRANT UPDATE(canonical_id) ON memory_canonical_state TO workshop_worker;
GRANT UPDATE(area) ON supervisor_authorities,supervisor_policies,supervisor_journal_heads TO workshop_worker;
GRANT UPDATE(enabled,revoked) ON supervisor_policies TO workshop_worker;
GRANT INSERT,UPDATE ON supervisor_actions,supervisor_budget_days,supervisor_attempts,supervisor_attempt_states,supervisor_settlements,supervisor_action_results,supervisor_journal,supervisor_model_contexts,supervisor_proposals TO workshop_worker;
GRANT INSERT ON supervisor_question_denials TO workshop_worker;
GRANT USAGE,SELECT ON SEQUENCE supervisor_journal_sequence_seq TO workshop_worker;
