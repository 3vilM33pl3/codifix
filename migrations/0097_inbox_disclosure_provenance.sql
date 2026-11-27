-- A specialist's output inherits the complete context of its immutable dispatch.
-- Joining later does not authorize old context indirectly through a late reply.
ALTER TABLE inbox_messages ADD COLUMN source_ticket_id UUID REFERENCES inbox_dispatches(id);
ALTER TABLE inbox_dispatches ADD COLUMN source_ticket_id UUID REFERENCES inbox_dispatches(id);
CREATE INDEX inbox_message_source ON inbox_messages(source_ticket_id) WHERE source_ticket_id IS NOT NULL;
-- Rejoining creates a fresh review and history boundary; old memberships remain
-- as immutable provenance and cannot become active again implicitly.
ALTER TABLE inbox_members DROP CONSTRAINT inbox_members_conversation_id_grant_id_key;
CREATE UNIQUE INDEX inbox_active_membership ON inbox_members(conversation_id,grant_id) WHERE active;
