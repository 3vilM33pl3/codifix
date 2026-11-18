-- Metadata only. Applying this migration never starts a companion or enrolls a peer.
CREATE TABLE agency_runtime_heartbeats (
 instance_id UUID PRIMARY KEY CHECK(instance_id <> '00000000-0000-0000-0000-000000000000'),
 version TEXT NOT NULL CHECK(length(version) BETWEEN 1 AND 128),
 observed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
-- Commit-ordered within each task: the existing task row serializes revisions.
-- Fields are a bounded allowlist; source payloads and artifact references stay private.
CREATE TABLE agency_observation_events (
 task_id UUID NOT NULL REFERENCES agency_tasks(id),
 sequence BIGINT GENERATED ALWAYS AS IDENTITY,
 kind TEXT NOT NULL CHECK(kind IN ('request','clarification','answer','result','status')),
 body TEXT NOT NULL CHECK(length(body)<=4096),
 recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,sequence)
);
CREATE FUNCTION agency_record_observation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 -- Human inbox conversations and their forwarded payloads are excluded entirely.
 IF NEW.requester LIKE 'inbox-gateway:%' OR NEW.payload::text LIKE '%"conversation_turn"%' OR NEW.payload::text LIKE '%inbox-gateway:%' THEN RETURN NEW; END IF;
 IF TG_OP='INSERT' THEN
  INSERT INTO agency_observation_events(task_id,kind,body)
   VALUES(NEW.id,'request',left(coalesce(NEW.payload#>>'{payload,request,objective}',NEW.contract->>'objective',''),4096));
 ELSE
  IF NEW.contract#>>'{clarification,question}' IS DISTINCT FROM OLD.contract#>>'{clarification,question}' AND NEW.contract#>>'{clarification,question}' IS NOT NULL THEN
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
CREATE TRIGGER agency_observation_journal AFTER INSERT OR UPDATE OF contract ON agency_tasks
 FOR EACH ROW EXECUTE FUNCTION agency_record_observation();
