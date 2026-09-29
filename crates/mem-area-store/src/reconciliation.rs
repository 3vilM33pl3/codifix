// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reviewed merge into an existing installation. Runtime authority is never imported.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowDecision {
    pub table: String,
    pub key: String,
    pub source_sha256: String,
    pub target_sha256: String,
    pub destination_sha256: Option<String>,
    pub conflict: bool,
    /// Explicit operator resolution, permitted only for existing non-version rows.
    #[serde(default)]
    pub keep_destination: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub format_version: u32,
    pub target_instance: Uuid,
    pub area: AreaId,
    pub source_instance: Uuid,
    pub snapshot_id: Uuid,
    pub inventory_sha256: String,
    pub manifest_sha256: String,
    pub partition_sha256: String,
    pub destination_sha256: String,
    pub rows: Vec<RowDecision>,
}
impl Preview {
    pub fn digest(&self) -> Result<String> {
        serde_json::to_string(self)
            .map(|s| hash(&s))
            .map_err(|_| ImportError::Invalid)
    }
    pub fn conflicts(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.conflict && !r.keep_destination)
            .count()
    }
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub operation_id: Uuid,
    pub preview_sha256: String,
    pub result_sha256: String,
    pub inserted: i64,
    pub already_applied: bool,
}

fn inputs(partition: &MaterializedPartition) -> Result<Vec<InputRow<'_>>> {
    let mut rows: Vec<_> = partition
        .rows()
        .iter()
        .map(|r| InputRow {
            record: r.record(),
            json: r.row_json(),
            source_sha: r.source_sha256(),
            wire_sha: r.target_sha256(),
            columns: r.columns(),
            transformed: false,
        })
        .chain(partition.projected_sources().iter().map(|r| InputRow {
            record: &r.binding().source.record,
            json: r.target_row_json(),
            source_sha: &r.binding().source.row_sha256,
            wire_sha: r.target_sha256(),
            columns: r.columns(),
            transformed: true,
        }))
        .collect();
    if rows
        .iter()
        .any(|r| !TABLES.contains(&r.record.table.as_str()) || hash(r.json) != r.wire_sha)
    {
        return Err(ImportError::Invalid);
    }
    rows.sort_by_key(|r| {
        (
            TABLES.iter().position(|t| *t == r.record.table),
            &r.record.key,
        )
    });
    Ok(rows)
}

async fn setup(conn: &mut PgConnection, target: Uuid, area: AreaId) -> Result<()> {
    sqlx::raw_sql("SET LOCAL timezone='UTC'; SET LOCAL datestyle='ISO, YMD'; SET LOCAL intervalstyle='postgres'; SET LOCAL extra_float_digits=3; SET LOCAL bytea_output='hex'; SET LOCAL search_path=pg_catalog,public; SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='60s'; SET LOCAL row_security=off;").execute(&mut *conn).await?;
    let head: Option<i64> =
        sqlx::query_scalar("SELECT max(version) FROM public._sqlx_migrations WHERE success")
            .persistent(false)
            .fetch_one(&mut *conn)
            .await?;
    if head != Some(RECONCILIATION_TARGET_MIGRATION) {
        return Err(ImportError::Schema);
    }
    // Require an established installation binding, not just a caller supplied UUID.
    let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.area_import_receipts WHERE target_instance=$1 AND area=$2) OR EXISTS(SELECT 1 FROM public.supervisor_instance_binding WHERE instance_id=$1 AND area=$2)")
        .persistent(false).bind(target).bind(area_name(area)).fetch_one(&mut *conn).await?;
    if !bound {
        return Err(ImportError::Invalid);
    }
    Ok(())
}

async fn destination(conn: &mut PgConnection) -> Result<String> {
    // All knowledge rows, including destination-only histories/dependencies, are
    // bound. Stream rows rather than constructing an unbounded SQL aggregate.
    let mut digest = Sha256::new();
    for table in TABLES {
        digest.update(table.as_bytes());
        let mut last: Option<String> = None;
        loop {
            let rows: Vec<String> = sqlx::query_scalar(&format!("SELECT to_jsonb(t)::text FROM {table} t WHERE ($1::text IS NULL OR to_jsonb(t)::text COLLATE \"C\" > $1 COLLATE \"C\") ORDER BY to_jsonb(t)::text COLLATE \"C\" LIMIT 256"))
                .persistent(false).bind(&last).fetch_all(&mut *conn).await?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                digest.update((row.len() as u64).to_be_bytes());
                digest.update(row.as_bytes());
            }
            last = rows.last().cloned();
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn current_json(conn: &mut PgConnection, row: &InputRow<'_>) -> Result<Option<String>> {
    let keys = match row.record.table.as_str() {
        "public.memory_tags" => &["memory_entry_id", "tag"][..],
        "public.memory_canonical_state" => &["canonical_id"][..],
        _ => &["id"][..],
    };
    let predicate = keys
        .iter()
        .map(|k| format!("t.\"{k}\"=x.\"{k}\""))
        .collect::<Vec<_>>()
        .join(" AND ");
    let bytes: Option<String> = sqlx::query_scalar(&format!("SELECT to_jsonb(t)::text FROM {} t,json_populate_record(NULL::{},$1::json) x WHERE {predicate}", row.record.table,row.record.table))
        .persistent(false).bind(row.json).fetch_optional(conn).await?;
    Ok(bytes)
}

async fn build(
    conn: &mut PgConnection,
    target: Uuid,
    partition: &MaterializedPartition,
    rows: &[InputRow<'_>],
) -> Result<Preview> {
    let mut decisions = Vec::new();
    let mut catalogs = std::collections::BTreeMap::new();
    for row in rows {
        match catalogs.insert(&row.record.table, row.columns) {
            Some(previous) if previous != row.columns => return Err(ImportError::Schema),
            None => check_columns(conn, row).await?,
            _ => {}
        }
        let target_sha256 = canonical_hash(conn, row.json).await?;
        let existing = current_json(conn, row).await?.map(|b| hash(&b));
        let mut conflict = existing.as_ref().is_some_and(|s| *s != target_sha256);
        if row.record.table == "public.memory_entries" && existing.is_none() {
            // New versions may extend an existing history but never fill/rewrite
            // an older slot or collide with another canonical/version identity.
            let behind: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.memory_entries t,json_populate_record(NULL::public.memory_entries,$1::json) x WHERE t.canonical_id=x.canonical_id AND (t.version_no>=x.version_no OR t.project_id<>x.project_id))")
                .persistent(false).bind(row.json).fetch_one(&mut *conn).await?;
            conflict |= behind;
        }
        decisions.push(RowDecision {
            table: row.record.table.clone(),
            key: row.record.key.clone(),
            source_sha256: row.source_sha.into(),
            target_sha256,
            destination_sha256: existing,
            conflict,
            keep_destination: false,
        });
    }
    Ok(Preview {
        format_version: 1,
        target_instance: target,
        area: partition.area(),
        source_instance: partition.source_instance(),
        snapshot_id: partition.snapshot_id(),
        inventory_sha256: partition.inventory_sha256().into(),
        manifest_sha256: partition.manifest_sha256().into(),
        partition_sha256: partition.partition_sha256().into(),
        destination_sha256: destination(conn).await?,
        rows: decisions,
    })
}

pub async fn preview(
    pool: &PgPool,
    target: Uuid,
    partition: &MaterializedPartition,
) -> Result<Preview> {
    if target.is_nil() || target == partition.source_instance() {
        return Err(ImportError::Invalid);
    }
    let rows = inputs(partition)?;
    let mut tx = pool.begin().await?;
    query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    setup(&mut tx, target, partition.area()).await?;
    let result = build(&mut tx, target, partition, &rows).await?;
    tx.commit().await?;
    Ok(result)
}

/// `writers_stopped` is an operator attestation, not a persistent writer fence.
/// Table locks serialize this operation; callers must retain their external
/// fence until all routing is switched. No pool/user credentials are discovered.
pub async fn reconcile(
    pool: &PgPool,
    operation: Uuid,
    partition: &MaterializedPartition,
    expected: &Preview,
    writers_stopped: bool,
) -> Result<Receipt> {
    if !writers_stopped
        || operation.is_nil()
        || expected.target_instance.is_nil()
        || expected.target_instance == partition.source_instance()
        || expected.conflicts() != 0
    {
        return Err(ImportError::Invalid);
    }
    let rows = inputs(partition)?;
    let mut tx = pool.begin().await?;
    setup(&mut tx, expected.target_instance, partition.area()).await?;
    // Deterministic lock order, shared by every reconciler. Runtime stores are
    // untouched and are not imported from historical source material.
    for table in TABLES
        .iter()
        .copied()
        .chain(["public.area_reconciliation_receipts"])
    {
        query(&format!("LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE"))
            .execute(&mut *tx)
            .await?;
    }
    let digest = expected.digest()?;
    if let Some(receipt) = query("SELECT * FROM public.area_reconciliation_receipts WHERE id=$1")
        .bind(operation)
        .fetch_optional(&mut *tx)
        .await?
    {
        if receipt.try_get::<String, _>("preview_sha256")? != digest
            || expected.partition_sha256 != partition.partition_sha256()
            || expected.inventory_sha256 != partition.inventory_sha256()
            || expected.manifest_sha256 != partition.manifest_sha256()
            || expected.snapshot_id != partition.snapshot_id()
            || expected.source_instance != partition.source_instance()
        {
            return Err(ImportError::Conflict);
        }
        let actual = destination(&mut tx).await?;
        if receipt.try_get::<String, _>("result_sha256")? != actual {
            return Err(ImportError::Changed);
        }
        for (row, decision) in rows.iter().zip(&expected.rows) {
            if !decision.keep_destination {
                verify_row(&mut tx, row).await?;
            }
        }
        tx.commit().await?;
        return Ok(Receipt {
            operation_id: operation,
            preview_sha256: digest,
            result_sha256: actual,
            inserted: receipt.try_get("inserted")?,
            already_applied: true,
        });
    }
    let mut actual = build(&mut tx, expected.target_instance, partition, &rows).await?;
    for (found, reviewed) in actual.rows.iter_mut().zip(&expected.rows) {
        if reviewed.keep_destination {
            if !found.conflict
                || found.destination_sha256.is_none()
                || found.table == "public.memory_entries"
            {
                return Err(ImportError::Invalid);
            }
            found.keep_destination = true;
        }
    }
    if &actual != expected {
        return Err(ImportError::Changed);
    }
    let mut inserted = 0;
    // Save explicitly retained canonical state before version-insert triggers.
    let mut retained_states = std::collections::BTreeMap::new();
    for (row, decision) in rows.iter().zip(&actual.rows) {
        if decision.keep_destination && row.record.table == "public.memory_canonical_state" {
            retained_states.insert(
                &row.record.key,
                current_json(&mut tx, row)
                    .await?
                    .ok_or(ImportError::Changed)?,
            );
        }
    }
    for (row, decision) in rows.iter().zip(&actual.rows) {
        if decision.keep_destination {
            if let Some(json) = retained_states
                .get(&row.record.key)
                .filter(|_| row.record.table == "public.memory_canonical_state")
            {
                let retained = InputRow {
                    json,
                    record: row.record,
                    source_sha: row.source_sha,
                    wire_sha: row.wire_sha,
                    columns: row.columns,
                    transformed: row.transformed,
                };
                insert_row(&mut tx, &retained).await?;
            }
            continue;
        }
        if decision.destination_sha256.is_none()
            || row.record.table == "public.memory_canonical_state"
        {
            // Canonical rows are restored after version-insert triggers. They
            // can only equal the preview or be new; conflicting state is denied.
            insert_row(&mut tx, row).await?;
            if decision.destination_sha256.is_none() {
                inserted += 1;
            }
        }
    }
    for (row, decision) in rows.iter().zip(&actual.rows) {
        if decision.keep_destination {
            if current_json(&mut tx, row).await?.map(|b| hash(&b)) != decision.destination_sha256 {
                return Err(ImportError::Changed);
            }
        } else {
            verify_row(&mut tx, row).await?;
        }
    }
    // RESTART is transactional; preserve an already higher allocation watermark.
    let (maximum, last, called): (i64, i64, bool) = sqlx::query_as("SELECT COALESCE((SELECT max(id) FROM public.memory_source_checks),0),last_value,is_called FROM public.memory_source_checks_id_seq")
        .persistent(false).fetch_one(&mut *tx).await?;
    if last < maximum || (last == maximum && !called) {
        let next = maximum.checked_add(1).ok_or(ImportError::Invalid)?;
        query(&format!(
            "ALTER SEQUENCE public.memory_source_checks_id_seq RESTART WITH {next}"
        ))
        .execute(&mut *tx)
        .await?;
    }
    let result = destination(&mut tx).await?;
    query("INSERT INTO public.area_reconciliation_receipts(id,target_instance,area,preview_sha256,result_sha256,preview_json,inserted) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(operation).bind(expected.target_instance).bind(area_name(partition.area())).bind(&digest).bind(&result).bind(serde_json::to_value(expected).map_err(|_| ImportError::Invalid)?).bind(inserted).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Receipt {
        operation_id: operation,
        preview_sha256: digest,
        result_sha256: result,
        inserted,
        already_applied: false,
    })
}
