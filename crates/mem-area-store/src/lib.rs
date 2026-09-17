// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted offline fresh-target import. No credential discovery, migrations,
//! service activation, source database access or external effects.

use mem_area::{AreaId, Column, MaterializedPartition, RecordRef};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

pub const TARGET_MIGRATION: i64 = 56;
pub const HEALTH_TARGET_MIGRATION: i64 = 57;
const TABLES: &[&str] = &[
    "public.projects",
    "public.sessions",
    "public.tasks",
    "public.raw_captures",
    "public.memory_entries",
    "public.memory_canonical_state",
    "public.memory_sources",
    "public.memory_source_checks",
    "public.memory_tags",
    "public.memory_relations",
    "public.project_commits",
];

/// Never retains a PostgreSQL error/source: failing-row details can contain
/// private source text. A caller must not turn these into unchecked retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("import identity or validated partition is invalid")]
    Invalid,
    #[error("target is not an empty, inactive installation")]
    NotFresh,
    #[error("target schema is not the exact supported migration/column catalog")]
    Schema,
    #[error("target already has a different immutable import identity")]
    Conflict,
    #[error("target rows or receipts do not match the reviewed partition")]
    Changed,
    #[error("private import transaction failed; inspect metadata and reconcile before retry")]
    Database,
}
impl From<sqlx::Error> for ImportError {
    fn from(_: sqlx::Error) -> Self {
        Self::Database
    }
}
type Result<T> = std::result::Result<T, ImportError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReceipt {
    pub id: Uuid,
    pub target_instance: Uuid,
    pub area: AreaId,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub partition_sha256: String,
    pub row_count: i64,
    pub projection_count: i64,
    /// True only after rechecking every target row and receipt, not just finding
    /// an old operation ID. No mutation is repeated on this path.
    pub already_applied: bool,
}

struct InputRow<'a> {
    record: &'a RecordRef,
    json: &'a str,
    source_sha: &'a str,
    wire_sha: &'a str,
    columns: &'a [Column],
    transformed: bool,
}

/// Operates only on an opaque partition returned by full materialization.
/// The operator must separately approve the plan, retain the protected archive,
/// provision the intended target, and stop all writers. A UUID is not proof of
/// host identity. This library never chooses a database or changes privileges.
pub async fn import_partition(
    pool: &PgPool,
    target_instance: Uuid,
    operation_id: Uuid,
    partition: &MaterializedPartition,
) -> Result<ImportReceipt> {
    if target_instance.is_nil()
        || operation_id.is_nil()
        || target_instance == partition.source_instance()
    {
        return Err(ImportError::Invalid);
    }
    let mut rows: Vec<InputRow<'_>> = partition
        .rows()
        .iter()
        .map(|row| InputRow {
            record: row.record(),
            json: row.row_json(),
            source_sha: row.source_sha256(),
            wire_sha: row.target_sha256(),
            columns: row.columns(),
            transformed: false,
        })
        .collect();
    for projection in partition.projected_sources() {
        rows.push(InputRow {
            record: &projection.binding().source.record,
            json: projection.target_row_json(),
            source_sha: &projection.binding().source.row_sha256,
            wire_sha: projection.target_sha256(),
            columns: projection.columns(),
            transformed: true,
        });
    }
    if rows
        .iter()
        .any(|r| !TABLES.contains(&r.record.table.as_str()) || hash(r.json) != r.wire_sha)
    {
        return Err(ImportError::Invalid);
    }
    rows.sort_by_key(|row| {
        (
            TABLES
                .iter()
                .position(|t| *t == row.record.table)
                .unwrap_or(usize::MAX),
            &row.record.key,
        )
    });
    let count = i64::try_from(rows.len()).map_err(|_| ImportError::Invalid)?;
    let projections =
        i64::try_from(partition.projected_sources().len()).map_err(|_| ImportError::Invalid)?;
    let mut tx = pool.begin().await?;
    sqlx::raw_sql("SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres'; SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex'; SET LOCAL search_path=pg_catalog,public; SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='60s'; SET LOCAL row_security=off;")
        .execute(&mut *tx).await?;
    // Do not acquire broad locks on an accidentally selected populated source.
    let has_receipt: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.area_import_receipts)")
            .fetch_one(&mut *tx)
            .await?;
    if !has_receipt
        && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM public.projects)")
            .fetch_one(&mut *tx)
            .await?
    {
        return Err(ImportError::NotFresh);
    }
    let head: Option<i64> =
        sqlx::query_scalar("SELECT max(version) FROM public._sqlx_migrations WHERE success")
            .fetch_one(&mut *tx)
            .await?;
    if !matches!(head, Some(TARGET_MIGRATION | HEALTH_TARGET_MIGRATION)) {
        return Err(ImportError::Schema);
    }
    let target_head = head.ok_or(ImportError::Schema)?;
    let valid_history: bool = sqlx::query_scalar("SELECT count(*)=$1 AND bool_and(success AND version BETWEEN 1 AND $1) FROM public._sqlx_migrations").bind(target_head).fetch_one(&mut *tx).await?;
    if !valid_history {
        return Err(ImportError::Schema);
    }
    let tables: Vec<String> = sqlx::query_scalar("SELECT n.nspname||'.'||c.relname FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p','f') AND n.nspname <> 'information_schema' AND n.nspname !~ '^pg_' ORDER BY 1")
        .fetch_all(&mut *tx).await?;
    let expected: std::collections::BTreeSet<String> = mem_area::KNOWN_TABLES
        .iter()
        .chain(mem_area::SUPERVISOR_TABLES_0053)
        .chain(mem_area::SUPERVISOR_TABLES_0054)
        .chain(mem_area::SUPERVISOR_TABLES_0055)
        .chain(mem_area::AREA_IMPORT_TABLES_0056)
        .chain(if target_head == HEALTH_TARGET_MIGRATION {
            mem_area::SUPERVISOR_TABLES_0057
        } else {
            &[]
        })
        .map(|(name, _)| (*name).into())
        .collect();
    if tables
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        != expected
    {
        return Err(ImportError::Schema);
    }
    inactive(&mut tx, &tables, has_receipt).await?;
    for table in &tables {
        if !table.starts_with("public.") || mem_area::table_strategy(table).is_none() {
            return Err(ImportError::Schema);
        }
        let name = table.strip_prefix("public.").ok_or(ImportError::Schema)?;
        identifier(name)?;
        sqlx::query(&format!(
            "LOCK TABLE public.\"{name}\" IN ACCESS EXCLUSIVE MODE"
        ))
        .execute(&mut *tx)
        .await?;
    }
    let existing = sqlx::query("SELECT * FROM public.area_import_receipts WHERE singleton=true")
        .fetch_optional(&mut *tx)
        .await?;
    let already_applied = existing.is_some();
    // Recovery belongs to the same still-inactive installation. A receipt is
    // not permission to reconcile an activated service under broad table locks.
    inactive(&mut tx, &tables, already_applied).await?;
    if let Some(existing) = existing {
        if existing.try_get::<Uuid, _>("id")? != operation_id
            || existing.try_get::<Uuid, _>("target_instance")? != target_instance
            || existing.try_get::<String, _>("area")? != area_name(partition.area())
            || existing.try_get::<Uuid, _>("source_instance")? != partition.source_instance()
            || existing.try_get::<Uuid, _>("snapshot_id")? != partition.snapshot_id()
            || existing.try_get::<String, _>("partition_sha256")? != partition.partition_sha256()
            || existing.try_get::<String, _>("inventory_sha256")? != partition.inventory_sha256()
            || existing.try_get::<String, _>("manifest_sha256")? != partition.manifest_sha256()
            || existing.try_get::<i64, _>("row_count")? != count
            || existing.try_get::<i64, _>("projection_count")? != projections
        {
            return Err(ImportError::Conflict);
        }
        if existing.try_get::<i32, _>("source_migration")?
            != i32::try_from(partition.source_migration_version())
                .map_err(|_| ImportError::Invalid)?
            || existing.try_get::<i32, _>("target_migration")? != target_head as i32
        {
            return Err(ImportError::Conflict);
        }
    } else {
        sqlx::query("INSERT INTO public.area_import_receipts(id,target_instance,area,source_instance,snapshot_id,inventory_sha256,manifest_sha256,partition_sha256,source_migration,target_migration,row_count,projection_count) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$12,$10,$11)")
            .bind(operation_id).bind(target_instance).bind(area_name(partition.area()))
            .bind(partition.source_instance()).bind(partition.snapshot_id())
            .bind(partition.inventory_sha256()).bind(partition.manifest_sha256()).bind(partition.partition_sha256())
            .bind(i32::try_from(partition.source_migration_version()).map_err(|_| ImportError::Invalid)?)
            .bind(count).bind(projections).bind(target_head as i32).execute(&mut *tx).await?;
    }
    // Source and target column catalogs must match before *any* source insert.
    // Materialization shares one catalog per table; do not issue a catalog
    // query for every row of a large partition.
    let mut checked_columns = std::collections::BTreeMap::new();
    for row in &rows {
        if let Some(previous) = checked_columns.insert(row.record.table.as_str(), row.columns) {
            if previous != row.columns {
                return Err(ImportError::Schema);
            }
        } else {
            check_columns(&mut tx, row).await?;
        }
    }
    if !already_applied {
        for row in &rows {
            insert_row(&mut tx, row).await?;
        }
        for projection in partition.projected_sources() {
            let binding = projection.binding();
            let target_hash = canonical_hash(&mut tx, projection.target_row_json()).await?;
            sqlx::query("INSERT INTO public.area_import_provenance(import_id,source_id,decision_id,decision_sha256,canonical_id,memory_version_id,source_sha256,original_version_sha256,archived_task_id,archived_task_sha256,target_sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
                .bind(operation_id).bind(parse_id(&binding.source.record.key)?)
                .bind(projection.decision_id()).bind(projection.decision_sha256())
                .bind(binding.canonical_id).bind(parse_id(&binding.memory_version.record.key)?)
                .bind(&binding.source.row_sha256).bind(&binding.memory_version.row_sha256)
                .bind(parse_id(&binding.archived_task.record.key)?).bind(&binding.archived_task.row_sha256)
                .bind(target_hash).execute(&mut *tx).await?;
        }
        // Transactional RESTART, unlike setval, rolls back on any later failure.
        let largest: Option<i64> =
            sqlx::query_scalar("SELECT max(id) FROM public.memory_source_checks")
                .fetch_one(&mut *tx)
                .await?;
        let next = largest
            .unwrap_or(0)
            .checked_add(1)
            .filter(|n| *n > 0)
            .ok_or(ImportError::Invalid)?;
        sqlx::query(&format!(
            "ALTER SEQUENCE public.memory_source_checks_id_seq RESTART WITH {next}"
        ))
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE public.loop_global_state SET kill_switch_enabled=true WHERE id=true")
            .execute(&mut *tx)
            .await?;
    }
    // Replays verify the complete transformation evidence, not only its count.
    for projection in partition.projected_sources() {
        let binding = projection.binding();
        let expected_target = canonical_hash(&mut tx, projection.target_row_json()).await?;
        let receipt = sqlx::query(
            "SELECT * FROM public.area_import_provenance WHERE import_id=$1 AND source_id=$2",
        )
        .bind(operation_id)
        .bind(parse_id(&binding.source.record.key)?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ImportError::Changed)?;
        if receipt.try_get::<Uuid, _>("decision_id")? != projection.decision_id()
            || receipt.try_get::<String, _>("decision_sha256")? != projection.decision_sha256()
            || receipt.try_get::<Uuid, _>("canonical_id")? != binding.canonical_id
            || receipt.try_get::<Uuid, _>("memory_version_id")?
                != parse_id(&binding.memory_version.record.key)?
            || receipt.try_get::<String, _>("source_sha256")? != binding.source.row_sha256
            || receipt.try_get::<String, _>("original_version_sha256")?
                != binding.memory_version.row_sha256
            || receipt.try_get::<Uuid, _>("archived_task_id")?
                != parse_id(&binding.archived_task.record.key)?
            || receipt.try_get::<String, _>("archived_task_sha256")?
                != binding.archived_task.row_sha256
            || receipt.try_get::<String, _>("target_sha256")? != expected_target
        {
            return Err(ImportError::Changed);
        }
    }
    for row in &rows {
        let target_hash = verify_row(&mut tx, row).await?;
        if already_applied {
            let receipt = sqlx::query("SELECT source_sha256,target_sha256,transformed FROM public.area_import_rows WHERE import_id=$1 AND table_name=$2 AND record_key=$3")
                .bind(operation_id).bind(&row.record.table).bind(&row.record.key).fetch_optional(&mut *tx).await?.ok_or(ImportError::Changed)?;
            if receipt.try_get::<String, _>("source_sha256")? != row.source_sha
                || receipt.try_get::<String, _>("target_sha256")? != target_hash
                || receipt.try_get::<bool, _>("transformed")? != row.transformed
            {
                return Err(ImportError::Changed);
            }
        } else {
            sqlx::query("INSERT INTO public.area_import_rows(import_id,table_name,record_key,source_sha256,target_sha256,transformed) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(operation_id).bind(&row.record.table).bind(&row.record.key)
                .bind(row.source_sha).bind(target_hash).bind(row.transformed).execute(&mut *tx).await?;
        }
    }
    for table in TABLES {
        let actual: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&mut *tx)
            .await?;
        let expected = rows.iter().filter(|row| row.record.table == *table).count();
        if actual != i64::try_from(expected).map_err(|_| ImportError::Invalid)? {
            return Err(ImportError::Changed);
        }
    }
    for (table, expected) in [
        ("area_import_rows", count),
        ("area_import_provenance", projections),
    ] {
        let actual: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM public.{table} WHERE import_id=$1"
        ))
        .bind(operation_id)
        .fetch_one(&mut *tx)
        .await?;
        if actual != expected {
            return Err(ImportError::Changed);
        }
    }
    tx.commit().await?;
    Ok(ImportReceipt {
        id: operation_id,
        target_instance,
        area: partition.area(),
        source_instance: partition.source_instance(),
        snapshot_id: partition.snapshot_id(),
        partition_sha256: partition.partition_sha256().into(),
        row_count: count,
        projection_count: projections,
        already_applied,
    })
}

async fn inactive(conn: &mut PgConnection, tables: &[String], replay: bool) -> Result<()> {
    for table in tables {
        if replay
            && (TABLES.contains(&table.as_str())
                || [
                    "public.area_import_receipts",
                    "public.area_import_rows",
                    "public.area_import_provenance",
                ]
                .contains(&table.as_str()))
        {
            continue;
        }
        let allowed = match table.as_str() {
            "public._sqlx_migrations" => continue,
            "public.supervisor_authorities" => "current_revision IS NOT NULL",
            "public.supervisor_journal_heads" => "false",
            "public.timeline_position" => "position <> 0",
            "public.loop_global_state" if replay => {
                "NOT kill_switch_enabled OR updated_by IS NOT NULL OR reason IS NOT NULL"
            }
            "public.loop_global_state" => "updated_by IS NOT NULL OR reason IS NOT NULL",
            _ => "true",
        };
        let any: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {table} WHERE {allowed})"
        ))
        .fetch_one(&mut *conn)
        .await?;
        if any {
            return Err(ImportError::NotFresh);
        }
    }
    for (table, expected) in [
        ("supervisor_authorities", 4_i64),
        ("supervisor_journal_heads", 4),
        ("timeline_position", 1),
        ("loop_global_state", 1),
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM public.{table}"))
            .fetch_one(&mut *conn)
            .await?;
        if count != expected {
            return Err(ImportError::NotFresh);
        }
    }
    Ok(())
}

async fn check_columns(conn: &mut PgConnection, row: &InputRow<'_>) -> Result<()> {
    let actual = sqlx::query("SELECT a.attname AS name,pg_catalog.format_type(a.atttypid,a.atttypmod) AS type,NOT a.attnotnull AS nullable FROM pg_catalog.pg_attribute a WHERE a.attrelid=$1::regclass AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attname")
        .bind(&row.record.table).fetch_all(conn).await?;
    let mut expected: Vec<_> = row.columns.iter().collect();
    expected.sort_by_key(|c| &c.name);
    if expected.len() != actual.len() {
        return Err(ImportError::Schema);
    }
    for (column, actual) in expected.iter().zip(actual.iter()) {
        identifier(&column.name)?;
        if actual.try_get::<String, _>("name")? != column.name
            || actual.try_get::<String, _>("type")? != column.data_type
            || actual.try_get::<bool, _>("nullable")? != column.nullable
        {
            return Err(ImportError::Schema);
        }
    }
    Ok(())
}

async fn insert_row(conn: &mut PgConnection, row: &InputRow<'_>) -> Result<()> {
    let columns = row
        .columns
        .iter()
        .map(|c| format!("\"{}\"", c.name))
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = format!(
        "INSERT INTO {} ({columns}) OVERRIDING SYSTEM VALUE SELECT {columns} FROM json_populate_record(NULL::{},$1::json)",
        row.record.table, row.record.table
    );
    if row.record.table == "public.memory_canonical_state" {
        let updates = row
            .columns
            .iter()
            .filter(|c| c.name != "canonical_id")
            .map(|c| format!("\"{}\"=EXCLUDED.\"{}\"", c.name, c.name))
            .collect::<Vec<_>>()
            .join(",");
        statement.push_str(&format!(
            " ON CONFLICT (canonical_id) DO UPDATE SET {updates}"
        ));
    }
    sqlx::query(&statement).bind(row.json).execute(conn).await?;
    Ok(())
}

async fn verify_row(conn: &mut PgConnection, row: &InputRow<'_>) -> Result<String> {
    let keys = match row.record.table.as_str() {
        "public.memory_tags" => &["memory_entry_id", "tag"][..],
        "public.memory_canonical_state" => &["canonical_id"][..],
        _ => &["id"][..],
    };
    let predicate = keys
        .iter()
        .map(|key| format!("t.\"{key}\"=x.\"{key}\""))
        .collect::<Vec<_>>()
        .join(" AND ");
    let record_key = match row.record.table.as_str() {
        "public.memory_tags" => "jsonb_build_array(t.memory_entry_id,t.tag)::text",
        "public.memory_canonical_state" => "t.canonical_id::text",
        _ => "t.id::text",
    };
    let statement = format!(
        "SELECT to_jsonb(t)::text AS bytes,to_jsonb(t)=$1::jsonb AS matches,{record_key} AS record_key FROM {} t,json_populate_record(NULL::{},$1::json) x WHERE {predicate}",
        row.record.table, row.record.table
    );
    let found = sqlx::query(&statement)
        .bind(row.json)
        .fetch_optional(conn)
        .await?
        .ok_or(ImportError::Changed)?;
    let target_hash = hash(&found.try_get::<String, _>("bytes")?);
    if found.try_get::<String, _>("record_key")? != row.record.key
        || !found.try_get::<bool, _>("matches")?
        || (!row.transformed && target_hash != row.source_sha)
    {
        return Err(ImportError::Changed);
    }
    Ok(target_hash)
}

async fn canonical_hash(conn: &mut PgConnection, json: &str) -> Result<String> {
    let canonical: String = sqlx::query_scalar("SELECT $1::jsonb::text")
        .bind(json)
        .fetch_one(conn)
        .await?;
    Ok(hash(&canonical))
}
fn identifier(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(ImportError::Schema);
    }
    Ok(())
}
fn parse_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| ImportError::Invalid)
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn area_name(area: AreaId) -> &'static str {
    match area {
        AreaId::Trust => "trust",
        AreaId::Homelab => "homelab",
        AreaId::Workshop => "workshop",
        AreaId::General => "general",
    }
}
