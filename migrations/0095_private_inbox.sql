-- Private inbox state is never projected into AT Protocol repositories.
CREATE TABLE inbox_grants (
 id UUID PRIMARY KEY, home_instance_id UUID NOT NULL,
 principal_id UUID NOT NULL REFERENCES auth_principals(id), contact_id UUID NOT NULL,
 document JSONB NOT NULL CHECK(pg_column_size(document)<=262144),
 revision BIGINT NOT NULL DEFAULT 1, revoked BOOLEAN NOT NULL DEFAULT false,
 expires_at TIMESTAMPTZ NOT NULL, UNIQUE(home_instance_id,principal_id,contact_id)
);
CREATE TABLE inbox_conversations (
 id UUID PRIMARY KEY, home_instance_id UUID NOT NULL,
 principal_id UUID NOT NULL REFERENCES auth_principals(id), request_digest TEXT NOT NULL,
 title TEXT NOT NULL, lead_member_id UUID, paused BOOLEAN NOT NULL DEFAULT false,
 revision BIGINT NOT NULL DEFAULT 1, next_sequence BIGINT NOT NULL DEFAULT 1,
 read_sequence BIGINT NOT NULL DEFAULT 0,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX inbox_owner ON inbox_conversations(home_instance_id,principal_id,updated_at DESC);
CREATE TABLE inbox_members (
 id UUID PRIMARY KEY, conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 grant_id UUID NOT NULL REFERENCES inbox_grants(id), request_digest TEXT NOT NULL,
 active BOOLEAN NOT NULL DEFAULT true, revision BIGINT NOT NULL DEFAULT 1,
 joined_sequence BIGINT NOT NULL, disclosure_summary TEXT NOT NULL,
 selected_message_ids JSONB NOT NULL, disclosure JSONB NOT NULL,
 UNIQUE(conversation_id,grant_id)
);
ALTER TABLE inbox_conversations ADD FOREIGN KEY(lead_member_id) REFERENCES inbox_members(id);
CREATE TABLE inbox_messages (
 id UUID PRIMARY KEY, conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 sequence BIGINT NOT NULL, role TEXT NOT NULL CHECK(role IN ('human','specialist','system')),
 member_id UUID REFERENCES inbox_members(id), root_id UUID NOT NULL,
 body TEXT NOT NULL CHECK(octet_length(body)<=65536), format TEXT NOT NULL,
 artifact JSONB, recipient_member_ids JSONB NOT NULL, request_digest TEXT NOT NULL,
 delivery TEXT NOT NULL CHECK(delivery IN ('queued','confirmed','failed','uncertain')),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(), UNIQUE(conversation_id,sequence)
);
CREATE TABLE inbox_roots (
 id UUID PRIMARY KEY REFERENCES inbox_messages(id), conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 budget JSONB NOT NULL, turns INTEGER NOT NULL DEFAULT 0 CHECK(turns BETWEEN 0 AND 8),
 used_microusd BIGINT NOT NULL DEFAULT 0, used_calls BIGINT NOT NULL DEFAULT 0,
 deadline TIMESTAMPTZ NOT NULL, cancelled BOOLEAN NOT NULL DEFAULT false
);
CREATE TABLE inbox_dispatches (
 id UUID PRIMARY KEY, home_instance_id UUID NOT NULL, principal_id UUID NOT NULL,
 conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 member_id UUID NOT NULL REFERENCES inbox_members(id), member_revision BIGINT NOT NULL,
 grant_id UUID NOT NULL REFERENCES inbox_grants(id), grant_revision BIGINT NOT NULL,
 root_message_id UUID NOT NULL REFERENCES inbox_roots(id),
 request JSONB NOT NULL CHECK(pg_column_size(request)<=262144), request_digest TEXT NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL, task_id UUID UNIQUE REFERENCES agency_tasks(id),
 status TEXT NOT NULL DEFAULT 'queued', created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX inbox_outbox ON inbox_dispatches(home_instance_id,created_at) WHERE status='queued';
CREATE TABLE inbox_task_origins (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id), home_instance_id UUID NOT NULL,
 principal_id UUID NOT NULL, conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 member_id UUID NOT NULL REFERENCES inbox_members(id), root_message_id UUID NOT NULL REFERENCES inbox_roots(id),
 grant_id UUID NOT NULL REFERENCES inbox_grants(id), ticket_id UUID NOT NULL REFERENCES inbox_dispatches(id)
);
CREATE TABLE inbox_task_cards (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id), conversation_id UUID NOT NULL REFERENCES inbox_conversations(id),
 document JSONB NOT NULL, result_digest TEXT
);
CREATE TABLE inbox_task_commands (
 id UUID PRIMARY KEY, task_id UUID NOT NULL REFERENCES agency_tasks(id),
 conversation_id UUID NOT NULL REFERENCES inbox_conversations(id), kind TEXT NOT NULL,
 body JSONB NOT NULL, done BOOLEAN NOT NULL DEFAULT false,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE inbox_identity_attempts (
 id UUID PRIMARY KEY, home_instance_id UUID NOT NULL, principal_id UUID NOT NULL REFERENCES auth_principals(id),
 session_binding TEXT NOT NULL, status TEXT NOT NULL, authorization_url TEXT,
 expires_at TIMESTAMPTZ NOT NULL, result JSONB,
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE inbox_identity_links (
 home_instance_id UUID NOT NULL, principal_id UUID NOT NULL REFERENCES auth_principals(id),
 did TEXT NOT NULL, handle TEXT, pds TEXT NOT NULL, linked_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(home_instance_id,principal_id), UNIQUE(home_instance_id,did)
);
