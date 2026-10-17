-- Release-specific Codifix cutover receipts. Original signed history stays in place.
CREATE TABLE codifix_retired_federation_spaces (
    space UUID PRIMARY KEY,
    local_instance UUID NOT NULL CHECK(local_instance <> '00000000-0000-0000-0000-000000000000'),
    manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
    replicas JSONB NOT NULL CHECK(jsonb_typeof(replicas)='array'),
    manifest JSONB NOT NULL,
    retired_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER codifix_retirement_immutable BEFORE UPDATE OR DELETE ON codifix_retired_federation_spaces
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
CREATE TABLE codifix_laptop_binding_rebinds (
    local_project_id UUID PRIMARY KEY REFERENCES projects(id),
    old_binding JSONB NOT NULL,
    new_binding JSONB NOT NULL,
    manifest_sha256 TEXT NOT NULL CHECK(manifest_sha256 ~ '^[0-9a-f]{64}$'),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER codifix_rebind_immutable BEFORE UPDATE OR DELETE ON codifix_laptop_binding_rebinds
FOR EACH ROW EXECUTE FUNCTION federation_accepted_immutable();
-- A final checkpoint pins original deliveries, receipts AND the enrolled key schedule.
CREATE FUNCTION codifix_retained_history_sha256(s UUID,r UUID) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT encode(sha256(convert_to(jsonb_build_object(
  'records',COALESCE((SELECT jsonb_agg(delivery ORDER BY sequence) FROM federation_inbox WHERE space=s AND receiver=r),'[]'::jsonb),
  'receipts',COALESCE((SELECT jsonb_agg(receipt ORDER BY sequence) FROM federation_replica_receipts WHERE space=s AND receiver=r),'[]'::jsonb),
  'keys',(SELECT keys FROM federation_replica_trust WHERE space=s AND receiver=r)
 )::text,'UTF8')),'hex')
$$;
-- One exact, reviewed rebind. Logical origin and queued historical payloads stay bound.
CREATE FUNCTION codifix_rebind_laptop_project(p UUID,oldb JSONB,newb JSONB,h TEXT) RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE saved laptop_curated_bindings%ROWTYPE; receipt codifix_laptop_binding_rebinds%ROWTYPE;
BEGIN
    SELECT * INTO STRICT saved FROM laptop_curated_bindings WHERE local_project_id=p FOR UPDATE;
    SELECT * INTO receipt FROM codifix_laptop_binding_rebinds WHERE local_project_id=p;
    IF FOUND THEN
        IF receipt.old_binding=oldb AND receipt.new_binding=newb AND receipt.manifest_sha256=h
           AND saved.binding=newb AND saved.enabled THEN RETURN; END IF;
        RAISE EXCEPTION 'Codifix rebind receipt mismatch';
    END IF;
    IF (oldb IS NULL OR newb IS NULL OR h IS NULL
       OR jsonb_typeof(newb) IS DISTINCT FROM 'object'
       OR NOT (newb ?& ARRAY['scope','device_id','manifest_revision','manifest_sha256'])
       OR (newb-ARRAY['scope','device_id','manifest_revision','manifest_sha256'])<>'{}'::jsonb
       OR jsonb_typeof(newb->'scope') IS DISTINCT FROM 'object'
       OR NOT ((newb->'scope') ?& ARRAY['company_id','area','owner_instance_id','project_id','space_id'])
       OR ((newb->'scope')-ARRAY['company_id','area','owner_instance_id','project_id','space_id'])<>'{}'::jsonb
       OR newb->'scope'->>'space_id' IS NULL
       OR (newb->'scope'->>'space_id')::uuid='00000000-0000-0000-0000-000000000000'::uuid
       OR h !~ '^[0-9a-f]{64}$' OR saved.binding<>oldb OR saved.enabled
       OR oldb->'scope'->>'space_id'=newb->'scope'->>'space_id'
       OR ((oldb->'scope')-'space_id') IS DISTINCT FROM ((newb->'scope')-'space_id')
       OR oldb->>'device_id' IS DISTINCT FROM newb->>'device_id'
       OR newb->'scope'->>'project_id'<>p::text
       OR NOT EXISTS(SELECT 1 FROM codifix_retired_federation_spaces WHERE space=(oldb->'scope'->>'space_id')::uuid)
       OR EXISTS(SELECT 1 FROM codifix_retired_federation_spaces WHERE space=(newb->'scope'->>'space_id')::uuid)
       OR newb->>'manifest_sha256' !~ '^[0-9a-f]{64}$'
       OR (newb->>'manifest_revision')::bigint<=0) IS DISTINCT FROM FALSE THEN
        RAISE EXCEPTION 'Codifix rebind requires exact retired origin and fresh binding';
    END IF;
    INSERT INTO codifix_laptop_binding_rebinds(local_project_id,old_binding,new_binding,manifest_sha256)
      VALUES(p,oldb,newb,h);
    UPDATE laptop_curated_bindings SET binding=newb,enabled=TRUE WHERE local_project_id=p;
END $$;

-- Normalize two pgcrypto-era default qualifications across fresh/restored candidates.
-- Existing identifiers are unchanged; only future default evaluation is explicit.
ALTER TABLE public.federation_source_changes ALTER COLUMN id SET DEFAULT pg_catalog.gen_random_uuid();
ALTER TABLE public.project_commits ALTER COLUMN id SET DEFAULT pg_catalog.gen_random_uuid();
