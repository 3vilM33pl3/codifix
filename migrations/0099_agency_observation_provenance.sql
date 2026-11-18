-- Runtime metadata remains secret-free and bounded; no default activation.
ALTER TABLE agency_runtime_heartbeats
 ADD COLUMN enabled_specialists TEXT[] NOT NULL DEFAULT '{}' CHECK(cardinality(enabled_specialists)<=100),
 ADD COLUMN enrolled_peers TEXT[] NOT NULL DEFAULT '{}' CHECK(cardinality(enrolled_peers)<=100);
-- Immutable admission provenance, recorded from authenticated runtime peers.
-- Only root standalone peer work is observable. Existing rows are NOT backfilled.
CREATE TABLE agency_task_observation_origins (
 task_id UUID PRIMARY KEY REFERENCES agency_tasks(id) ON DELETE CASCADE,
 home_instance_id UUID NOT NULL,
 requester TEXT NOT NULL CHECK(length(requester)<=128),
 requester_instance_id UUID NOT NULL,
 requester_operator BOOLEAN NOT NULL,
 peer TEXT NOT NULL CHECK(length(peer)<=128),
 direction TEXT NOT NULL CHECK(direction IN ('incoming','outgoing')),
 CHECK(home_instance_id <> '00000000-0000-0000-0000-000000000000'),
 CHECK(requester_instance_id <> '00000000-0000-0000-0000-000000000000'),
 CHECK((direction='outgoing' AND requester_operator AND requester_instance_id=home_instance_id)
    OR (direction='incoming' AND NOT requester_operator AND requester_instance_id<>home_instance_id AND requester=peer))
);
CREATE FUNCTION agency_observation_origin_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'agency observation admission provenance is immutable'; END $$;
CREATE TRIGGER agency_observation_origin_guard BEFORE UPDATE ON agency_task_observation_origins
 FOR EACH ROW EXECUTE FUNCTION agency_observation_origin_immutable();
ALTER TABLE agency_observation_events DROP CONSTRAINT agency_observation_events_task_id_fkey;
ALTER TABLE agency_observation_events ADD FOREIGN KEY(task_id) REFERENCES agency_tasks(id) ON DELETE CASCADE;
-- The original journal was additive but broader than the intended peer boundary.
-- It is not authoritative history without authenticated admission provenance.
DELETE FROM agency_observation_events WHERE NOT EXISTS
 (SELECT 1 FROM agency_task_observation_origins o WHERE o.task_id=agency_observation_events.task_id);
DROP TRIGGER agency_observation_journal ON agency_tasks;
CREATE OR REPLACE FUNCTION agency_record_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Existing runtime roles keep working without any observation grants. Capture is
 -- optional and must never widen ordinary writer access to private Inbox tables.
 IF NOT has_table_privilege(current_user,'agency_task_observation_origins','SELECT')
 OR NOT has_table_privilege(current_user,'agency_observation_events','INSERT')
 OR NOT has_sequence_privilege(current_user,pg_get_serial_sequence('agency_observation_events','sequence'),'USAGE') THEN RETURN NEW; END IF;
 IF NOT EXISTS(SELECT 1 FROM agency_tasks WHERE id=NEW.id) THEN RETURN NEW; END IF;
 IF NEW.parent_id IS NOT NULL OR NEW.requester LIKE 'inbox-gateway:%'
 OR NEW.payload::text LIKE '%"conversation_turn"%' OR NEW.payload::text LIKE '%inbox-gateway:%'
 OR coalesce(NEW.payload->>'capability','') NOT IN ('delegate','inspect','plan_prepare','plan_apply','plan_inspect','model_draft')

 OR NOT EXISTS(SELECT 1 FROM agency_task_observation_origins o JOIN agency_mandates m ON m.id=NEW.mandate_id
     WHERE o.task_id=NEW.id AND o.requester=NEW.requester
       AND m.project_id=NEW.project_id AND m.document->>'subject_instance_id'=o.home_instance_id::text
       AND ((o.direction='incoming' AND NEW.payload->>'capability'<>'delegate')
         OR (o.direction='outgoing' AND NEW.payload->>'capability'='delegate'
           AND NEW.payload#>>'{payload,request,capability}' IN ('inspect','plan_prepare','plan_apply','plan_inspect','model_draft')
           AND NEW.payload#>>'{payload,request,capability}'=NEW.payload#>>'{payload,request,payload,kind}'
           AND NEW.payload#>>'{resource,kind}'='peer' AND NEW.payload#>>'{resource,id}'=o.peer
           AND NEW.payload#>>'{payload,peer}'=o.peer))) THEN RETURN NEW; END IF;
 IF TG_OP='INSERT' THEN
  INSERT INTO agency_observation_events(task_id,kind,body)
   VALUES(NEW.id,'request',left(coalesce(NEW.payload#>>'{payload,request,objective}',NEW.contract->>'objective',''),4096));
 ELSE
  IF NEW.contract#>>'{clarification,question}' IS NOT NULL AND (NEW.contract#>>'{clarification,question}' IS DISTINCT FROM OLD.contract#>>'{clarification,question}' OR (NEW.contract->>'status'='input_required' AND OLD.contract->>'status' IS DISTINCT FROM 'input_required')) THEN
   INSERT INTO agency_observation_events(task_id,kind,body) VALUES(NEW.id,'clarification',left(NEW.contract#>>'{clarification,question}',4096));
  END IF;
  IF NEW.contract#>>'{clarification,answer}' IS DISTINCT FROM OLD.contract#>>'{clarification,answer}' AND NEW.contract#>>'{clarification,answer}' IS NOT NULL THEN
   INSERT INTO agency_observation_events(task_id,kind,body) VALUES(NEW.id,'answer',left(NEW.contract#>>'{clarification,answer}',4096));
  END IF;
  IF NEW.contract#>>'{outcome,summary}' IS DISTINCT FROM OLD.contract#>>'{outcome,summary}' AND NEW.contract#>>'{outcome,summary}' IS NOT NULL THEN
   INSERT INTO agency_observation_events(task_id,kind,body) VALUES(NEW.id,'result',left(NEW.contract#>>'{outcome,summary}',4096));
  END IF;
  IF NEW.contract->>'status' IS DISTINCT FROM OLD.contract->>'status' THEN
   INSERT INTO agency_observation_events(task_id,kind,body) VALUES(NEW.id,'status',left(coalesce(NEW.contract->>'status','unknown'),4096));
  END IF;
 END IF;
 RETURN NEW;
END $$;
-- Submission inserts provenance after its task row. Deferral keeps both atomic.
CREATE CONSTRAINT TRIGGER agency_observation_journal AFTER INSERT OR UPDATE ON agency_tasks
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION agency_record_observation();
