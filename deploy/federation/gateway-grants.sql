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
-- No DELETE, TRUNCATE, schema modification, app tables, source captures, local
-- principals, inboxes or other database privileges are granted.
