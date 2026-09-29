-- Metadata-only, immutable evidence for stopped-writer reconciliation.
CREATE TABLE area_reconciliation_receipts (
    id uuid PRIMARY KEY CHECK (id <> '00000000-0000-0000-0000-000000000000'),
    target_instance uuid NOT NULL,
    area text NOT NULL CHECK (area IN ('trust','homelab','workshop','general')),
    preview_sha256 text NOT NULL CHECK (preview_sha256 ~ '^[0-9a-f]{64}$'),
    result_sha256 text NOT NULL CHECK (result_sha256 ~ '^[0-9a-f]{64}$'),
    preview_json jsonb NOT NULL,
    inserted bigint NOT NULL CHECK (inserted >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TRIGGER area_reconciliation_receipt_guard BEFORE UPDATE OR DELETE ON area_reconciliation_receipts
    FOR EACH ROW EXECUTE FUNCTION area_import_immutable();
REVOKE ALL ON area_reconciliation_receipts FROM PUBLIC;
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check CHECK(target_migration BETWEEN 56 AND 61);
