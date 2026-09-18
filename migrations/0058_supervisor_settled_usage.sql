-- Preserve authoritative provider counters with their immutable cost settlement.
-- Existing rows remain NULL (unknown); never fabricate historical zero usage.
-- Fixed order: input, output, total, cached input, cache-write input, reasoning.
ALTER TABLE supervisor_settlements ADD COLUMN token_usage numeric(20,0)[];
ALTER TABLE supervisor_settlements ADD CONSTRAINT supervisor_settled_usage_valid CHECK (
    token_usage IS NULL OR (
        array_ndims(token_usage) = 1 AND array_lower(token_usage,1) = 1
        AND cardinality(token_usage) = 6
        AND array_position(token_usage,NULL) IS NULL
        AND 0 <= ALL(token_usage) AND 18446744073709551615 >= ALL(token_usage)
        AND token_usage[1] + token_usage[2] = token_usage[3]
        AND token_usage[4] + token_usage[5] <= token_usage[1]
        AND token_usage[6] <= token_usage[2]
    )
);

-- Fresh imports keep their exact target-head binding; old receipts stay valid
-- evidence but never authorize replay against a different schema head.
ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check;
ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check
    CHECK (target_migration IN (56,57,58));
