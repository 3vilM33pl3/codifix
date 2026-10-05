-- Run as the separate schema owner after applying repository migrations.
-- Create gateway_runtime with a generated password via the operator secret
-- manager first. Do not make it the database owner or a member of other roles.
REVOKE CONNECT ON DATABASE memory_isolation FROM PUBLIC;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT CONNECT ON DATABASE memory_isolation TO gateway_runtime;
GRANT USAGE ON SCHEMA public TO gateway_runtime;
GRANT SELECT, INSERT, UPDATE ON
    federation_gateway_spaces, federation_accepted, federation_publish_outbox,
    federation_security, federation_audit, federation_retention_decisions
TO gateway_runtime;
GRANT SELECT, INSERT, UPDATE ON federation_expertise_mailbox,
    federation_expertise_daily_budget, federation_expertise_deliveries,
    federation_expertise_addressed TO gateway_runtime;
GRANT SELECT, INSERT ON federation_expertise_attempts TO gateway_runtime;
-- Optional project-scoped delivery policy; no app identity/projection access.
GRANT SELECT, INSERT, UPDATE ON federation_project_feeds,
    federation_project_streams TO gateway_runtime;
GRANT SELECT, INSERT ON federation_project_events,
    federation_project_operations TO gateway_runtime;
-- Only the private, expiring mailbox and its metadata journal may be purged.
-- Publication history and independent audit receipts remain append-only.
GRANT DELETE ON federation_expertise_mailbox, federation_expertise_attempts
TO gateway_runtime;
GRANT DELETE ON federation_expertise_deliveries,
    federation_expertise_addressed TO gateway_runtime;
-- No TRUNCATE, schema modification, app tables, source captures, local
-- principals, collection snapshots/grants, inboxes or other privileges.
