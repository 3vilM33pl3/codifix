-- Preserve old receipts while allowing exact, inactive fresh-target imports
-- into the new addressed-expertise schema. No existing authority is copied.
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check
    CHECK (target_migration BETWEEN 56 AND 65);
