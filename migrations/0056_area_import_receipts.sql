-- Fresh-target bulk import is one database operation with immutable receipts.
-- No source rows, credentials, jobs, policies or identities are activated here.
CREATE TABLE area_import_receipts (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    id uuid NOT NULL UNIQUE CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    target_instance uuid NOT NULL CHECK (target_instance <> '00000000-0000-0000-0000-000000000000'),
    area text NOT NULL CHECK (area IN ('trust','homelab','workshop','general')),
    source_instance uuid NOT NULL,
    snapshot_id uuid NOT NULL,
    inventory_sha256 text NOT NULL CHECK (inventory_sha256 ~ '^[0-9a-f]{64}$'),
    manifest_sha256 text NOT NULL CHECK (manifest_sha256 ~ '^[0-9a-f]{64}$'),
    partition_sha256 text NOT NULL CHECK (partition_sha256 ~ '^[0-9a-f]{64}$'),
    source_migration integer NOT NULL,
    target_migration integer NOT NULL CHECK (target_migration = 56),
    row_count bigint NOT NULL CHECK (row_count >= 0),
    projection_count bigint NOT NULL CHECK (projection_count >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE area_import_rows (
    import_id uuid NOT NULL REFERENCES area_import_receipts(id),
    table_name text NOT NULL,
    record_key text NOT NULL,
    source_sha256 text NOT NULL CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    target_sha256 text NOT NULL CHECK (target_sha256 ~ '^[0-9a-f]{64}$'),
    transformed boolean NOT NULL,
    PRIMARY KEY (import_id,table_name,record_key)
);
CREATE TABLE area_import_provenance (
    import_id uuid NOT NULL REFERENCES area_import_receipts(id),
    source_id uuid NOT NULL,
    decision_id uuid NOT NULL,
    decision_sha256 text NOT NULL CHECK (decision_sha256 ~ '^[0-9a-f]{64}$'),
    canonical_id uuid NOT NULL,
    memory_version_id uuid NOT NULL,
    source_sha256 text NOT NULL CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    original_version_sha256 text NOT NULL CHECK (original_version_sha256 ~ '^[0-9a-f]{64}$'),
    archived_task_id uuid NOT NULL,
    archived_task_sha256 text NOT NULL CHECK (archived_task_sha256 ~ '^[0-9a-f]{64}$'),
    target_sha256 text NOT NULL CHECK (target_sha256 ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (import_id,source_id),
    UNIQUE (import_id,decision_id)
);
CREATE FUNCTION area_import_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'area import evidence is immutable'; END $$;
CREATE TRIGGER area_import_receipt_guard BEFORE UPDATE OR DELETE ON area_import_receipts
    FOR EACH ROW EXECUTE FUNCTION area_import_immutable();
CREATE TRIGGER area_import_row_guard BEFORE UPDATE OR DELETE ON area_import_rows
    FOR EACH ROW EXECUTE FUNCTION area_import_immutable();
CREATE TRIGGER area_import_provenance_guard BEFORE UPDATE OR DELETE ON area_import_provenance
    FOR EACH ROW EXECUTE FUNCTION area_import_immutable();
REVOKE ALL ON area_import_receipts,area_import_rows,area_import_provenance FROM PUBLIC;
