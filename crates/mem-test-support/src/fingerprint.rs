// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fingerprints of small disposable evaluation databases, not production backup
//! validation. The caller must exclude privileged writers during measurement.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub struct BenchmarkFingerprint {
    pub sha256: String,
    pub rows: BTreeMap<String, usize>,
}

fn hash_field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value);
}

/// All public base tables are included by default, including reinforcement and
/// activity tables. Only the four explicitly permitted operational journals are
/// excluded. New tables cannot silently escape the hash through an allowlist.
/// Uses one read-only repeatable-read snapshot and normalized SQL text settings.
pub async fn benchmark_fingerprint(pool: &PgPool) -> Result<BenchmarkFingerprint> {
    let mut tx = pool.begin().await?;
    sqlx::raw_sql(
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY;
        SET LOCAL TIME ZONE 'UTC'; SET LOCAL DateStyle = 'ISO, YMD';
        SET LOCAL IntervalStyle = 'postgres'; SET LOCAL extra_float_digits = 3;
        SET LOCAL bytea_output = 'hex'; SET LOCAL statement_timeout = '30s';",
    )
    .execute(&mut *tx)
    .await?;
    let tables: Vec<String> = sqlx::query_scalar("SELECT c.relname FROM pg_class c
        JOIN pg_namespace n ON n.oid=c.relnamespace
        WHERE n.nspname='public' AND c.relkind IN ('r','p')
          AND NOT c.relispartition
          AND c.relname NOT IN ('external_effects','external_effect_outcomes','foreground_executions','auth_audit_events')
        ORDER BY c.relname COLLATE \"C\"")
        .fetch_all(&mut *tx).await?;
    ensure!(
        !tables.is_empty() && tables.len() <= 1024,
        "invalid benchmark table inventory"
    );
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"memory-r3-benchmark-v1");
    let mut rows = BTreeMap::new();
    let mut total_bytes = 0usize;
    for table in tables {
        hash_field(&mut hash, table.as_bytes());
        let schema: Vec<String> = sqlx::query_scalar(
            "SELECT jsonb_build_array(a.attname,
            format_type(a.atttypid,a.atttypmod), a.attnotnull, a.attidentity, a.attgenerated,
            pg_get_expr(d.adbin,d.adrelid))::text
            FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid
            JOIN pg_namespace n ON n.oid=c.relnamespace
            LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
            WHERE n.nspname='public' AND c.relname=$1 AND a.attnum>0 AND NOT a.attisdropped
            ORDER BY a.attnum",
        )
        .bind(&table)
        .fetch_all(&mut *tx)
        .await?;
        for column in schema {
            hash_field(&mut hash, column.as_bytes());
        }
        hash_field(&mut hash, b"rows");
        // Catalog-derived identifier, quoted rather than treated as SQL syntax.
        // SQL limits protect this small-fixture utility from unbounded fetching.
        let identifier = table.replace('"', "\"\"");
        let (count, bytes, largest): (i64, i64, i32) = sqlx::query_as(&format!(
            "SELECT count(*),coalesce(sum(octet_length(value)),0),coalesce(max(octet_length(value)),0)
             FROM (SELECT to_jsonb(t)::text AS value FROM public.\"{identifier}\" t) s"))
            .fetch_one(&mut *tx).await?;
        ensure!(
            count <= 100000
                && largest <= 16 * 1024 * 1024
                && bytes <= (256 * 1024 * 1024 - total_bytes) as i64,
            "benchmark data exceeds fixture limit"
        );
        let values: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT value FROM (SELECT to_jsonb(t)::text AS value FROM public.\"{identifier}\" t) s
             ORDER BY value COLLATE \"C\" LIMIT 100001"
        ))
        .fetch_all(&mut *tx)
        .await?;
        ensure!(
            values.len() <= 100000,
            "benchmark table exceeds fixture limit"
        );
        hash.update((values.len() as u64).to_be_bytes());
        for value in &values {
            total_bytes = total_bytes.saturating_add(value.len());
            ensure!(
                value.len() <= 16 * 1024 * 1024 && total_bytes <= 256 * 1024 * 1024,
                "benchmark data exceeds fixture limit"
            );
            hash_field(&mut hash, value.as_bytes());
        }
        rows.insert(table, values.len());
    }
    tx.commit().await?;
    Ok(BenchmarkFingerprint {
        sha256: format!("{:x}", hash.finalize()),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fingerprint_tracks_rows_schema_and_reset_not_physical_order_or_timezone() {
        let Some(db) = crate::IsolatedDatabase::new().await else {
            return;
        };
        sqlx::raw_sql("CREATE TABLE r3_probe(id integer, value text, observed timestamptz);
            INSERT INTO r3_probe VALUES (2,'second','2026-01-01T00:00:00Z'),(1,'first','2026-01-02T00:00:00Z');")
            .execute(&db.pool).await.expect("prepare synthetic rows");
        let before = benchmark_fingerprint(&db.pool)
            .await
            .expect("fingerprint fixture");
        assert_eq!(before.rows.get("r3_probe"), Some(&2));
        assert!(before.rows.contains_key("memory_scores"));
        assert!(!before.rows.contains_key("auth_audit_events"));
        sqlx::raw_sql("SET TIME ZONE 'Pacific/Auckland';
            DELETE FROM r3_probe;
            INSERT INTO r3_probe VALUES (1,'first','2026-01-02T00:00:00Z'),(2,'second','2026-01-01T00:00:00Z');")
            .execute(&db.pool).await.expect("reorder fixture");
        assert_eq!(
            benchmark_fingerprint(&db.pool)
                .await
                .expect("reordered fingerprint"),
            before
        );
        let frozen = db.freeze().await.expect("freeze prepared fixture");
        let clone = frozen.fork().await.expect("clone fixture");
        assert_eq!(
            benchmark_fingerprint(&clone.pool)
                .await
                .expect("cloned fingerprint"),
            before
        );
        sqlx::query("UPDATE r3_probe SET value='changed' WHERE id=1")
            .execute(&clone.pool)
            .await
            .expect("tamper test row");
        assert_ne!(
            benchmark_fingerprint(&clone.pool)
                .await
                .expect("changed fingerprint"),
            before
        );
        clone.cleanup().await.expect("remove altered clone");
        let clone = frozen.fork().await.expect("fresh clone");
        sqlx::query("ALTER TABLE r3_probe ADD COLUMN nullable text")
            .execute(&clone.pool)
            .await
            .expect("change schema");
        assert_ne!(
            benchmark_fingerprint(&clone.pool)
                .await
                .expect("schema fingerprint"),
            before
        );
        clone.cleanup().await.expect("remove schema clone");
        let reader = frozen.fork_read_only().await.expect("read-only clone");
        assert_eq!(
            benchmark_fingerprint(&reader.pool)
                .await
                .expect("reader fingerprint"),
            before
        );
        reader.cleanup().await.expect("remove reader clone");
        frozen.cleanup().await.expect("remove template");
    }
}
