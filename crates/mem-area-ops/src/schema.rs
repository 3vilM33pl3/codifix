// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fresh schema only, never application bootstrap or an existing-target upgrade.
pub mod gateway;
use crate::{Failure, Input, Result, TARGET_LIMIT, Target, hash, json, valid_digest};
use serde::Serialize;
use sqlx::{
    Connection, PgConnection, Row,
    migrate::{Migrate, Migrator},
};
use std::{borrow::Cow, path::PathBuf, time::Duration};

const HEAD: i64 = 70;
// Same compatibility rule as mem-service::runtime and mem-test-support. Preserve
// the applied checksum; migration 16 removes the obsolete dimensionless index.
const VECTOR_COMPAT: &str = r#"
CREATE EXTENSION IF NOT EXISTS vector;

DROP INDEX IF EXISTS idx_memory_chunks_embedding_hnsw;

ALTER TABLE memory_chunks
    DROP COLUMN IF EXISTS embedding;

ALTER TABLE memory_chunks
    ADD COLUMN IF NOT EXISTS embedding vector;
"#;

fn migrations() -> Migrator {
    let mut migrator = sqlx::migrate!("../../migrations");
    migrator.migrations = Cow::Owned(
        migrator
            .iter()
            .cloned()
            .map(|mut migration| {
                if migration.version == 4 {
                    migration.sql = Cow::Borrowed(VECTOR_COMPAT);
                }
                migration
            })
            .collect(),
    );
    migrator
}

#[derive(Debug, Serialize)]
pub struct SchemaPlan {
    pub status: &'static str,
    pub migration_head: i64,
    pub migrations: usize,
    pub schema_sha256: String,
    pub creates_credentials: bool,
    pub starts_services: bool,
}

fn describe(migrator: &Migrator) -> Result<SchemaPlan> {
    let mut identity = Vec::new();
    for (index, migration) in migrator.iter().enumerate() {
        if migration.version != index as i64 + 1 || migration.no_tx {
            return Err(Failure::Schema);
        }
        // Bind both historic applied identity and the exact executable SQL.
        identity.extend_from_slice(&migration.version.to_be_bytes());
        identity.extend_from_slice(&migration.checksum);
        identity.extend_from_slice(hash(migration.sql.as_bytes()).as_bytes());
    }
    if migrator.iter().count() != HEAD as usize {
        return Err(Failure::Schema);
    }
    Ok(SchemaPlan {
        status: "schema_plan_only_no_database_connection",
        migration_head: HEAD,
        migrations: HEAD as usize,
        schema_sha256: hash(&identity),
        creates_credentials: false,
        starts_services: false,
    })
}

/// Metadata only. The compiled migration head cannot silently grow on upgrade.
pub fn plan() -> Result<SchemaPlan> {
    describe(&migrations())
}

pub struct Initialization {
    pub target: PathBuf,
    pub target_sha256: String,
    pub schema_sha256: String,
    pub execute: bool,
    pub writers_stopped: bool,
    pub fresh_database: bool,
}

/// Confirmation flags attest operator fencing; they do not prove it. This
/// deliberately refuses replays into any initialized database. An ambiguous
/// acknowledgement requires inspection, never automatic drop/reinitialization.
pub async fn initialize(request: &Initialization) -> Result<SchemaPlan> {
    let migrator = migrations();
    let mut report = describe(&migrator)?;
    if !request.execute
        || !request.writers_stopped
        || !request.fresh_database
        || !valid_digest(&request.target_sha256)
        || report.schema_sha256 != request.schema_sha256
    {
        return Err(Failure::Target);
    }
    let mut file = Input::open(&request.target, TARGET_LIMIT)?;
    let (target, digest): (Target, _) = json(&mut file, TARGET_LIMIT)?;
    if digest != request.target_sha256 {
        return Err(Failure::Binding);
    }
    let options = target.options()?;
    let mut connection = tokio::time::timeout(
        Duration::from_secs(10),
        PgConnection::connect_with(&options),
    )
    .await
    .map_err(|_| Failure::Connection)?
    .map_err(|_| Failure::Connection)?;
    file.unchanged()?;
    let outcome = tokio::time::timeout(
        Duration::from_secs(120),
        prepare(&mut connection, &migrator),
    )
    .await
    .map_err(|_| Failure::Schema)?;
    let _ = connection.close().await;
    outcome?;
    report.status = "fresh_schema_initialized_only_not_imported_or_activated";
    Ok(report)
}

async fn prepare(connection: &mut PgConnection, migrator: &Migrator) -> Result<()> {
    let mut transaction = connection.begin().await.map_err(|_| Failure::Schema)?;
    sqlx::raw_sql("SET LOCAL search_path=public,pg_catalog; SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='60s';")
        .execute(&mut *transaction).await.map_err(|_| Failure::Schema)?;
    let locked: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('memory-area-fresh-schema-v1',0))",
    )
    .persistent(false)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| Failure::Schema)?;
    if !locked {
        return Err(Failure::NotEmpty);
    }
    // Refuse before running migration 1 (which contains IF NOT EXISTS). No
    // pre-existing extension except PostgreSQL's default plpgsql is accepted.
    // Do not mistake an empty table, custom function or another schema for an
    // empty database, and do not terminate other clients to make this pass.
    // Namespace dependencies also catch standalone collations, operators and
    // text-search objects that are absent from pg_class/pg_proc/pg_type.
    let occupied: bool = sqlx::query_scalar(r#"
SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datid=(SELECT oid FROM pg_database WHERE datname=current_database()) AND pid<>pg_backend_pid())
OR EXISTS(SELECT 1 FROM pg_namespace WHERE nspname NOT IN ('public','information_schema') AND nspname !~ '^pg_')
OR EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_depend d JOIN pg_namespace n ON d.refobjid=n.oid WHERE d.refclassid='pg_namespace'::regclass AND n.nspname='public')
OR EXISTS(SELECT 1 FROM pg_extension WHERE extname<>'plpgsql')
OR EXISTS(SELECT 1 FROM pg_event_trigger)
OR EXISTS(SELECT 1 FROM pg_largeobject_metadata)
OR EXISTS(SELECT 1 FROM pg_foreign_server)
OR EXISTS(SELECT 1 FROM pg_foreign_data_wrapper)
OR EXISTS(SELECT 1 FROM pg_default_acl)
OR EXISTS(SELECT 1 FROM pg_publication)
OR EXISTS(SELECT 1 FROM pg_subscription WHERE subdbid=(SELECT oid FROM pg_database WHERE datname=current_database()))
"#).persistent(false).fetch_one(&mut *transaction).await.map_err(|_| Failure::Schema)?;
    if occupied {
        return Err(Failure::NotEmpty);
    }
    transaction
        .ensure_migrations_table()
        .await
        .map_err(|_| Failure::Schema)?;
    for migration in migrator.iter() {
        if migration.no_tx {
            return Err(Failure::Schema);
        }
        // SQLx creates a nested savepoint; the outer transaction covers every
        // migration, its history and validation. Never run service bootstrap.
        transaction
            .apply(migration)
            .await
            .map_err(|_| Failure::Schema)?;
    }
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .persistent(false)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| Failure::Schema)?;
    for table in tables {
        if !table
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(Failure::Schema);
        }
        let expected = match table.as_str() {
            "_sqlx_migrations" => HEAD,
            "supervisor_authorities" | "supervisor_journal_heads" => 4,
            "timeline_position" | "loop_global_state" => 1,
            _ => 0,
        };
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM public.\"{table}\""))
            .persistent(false)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|_| Failure::Schema)?;
        if count != expected {
            return Err(Failure::Schema);
        }
    }
    let history = sqlx::query(
        "SELECT version,checksum,success FROM public._sqlx_migrations ORDER BY version",
    )
    .persistent(false)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| Failure::Schema)?;
    if history.len() != HEAD as usize {
        return Err(Failure::Schema);
    }
    for (row, migration) in history.iter().zip(migrator.iter()) {
        if row
            .try_get::<i64, _>("version")
            .map_err(|_| Failure::Schema)?
            != migration.version
            || row
                .try_get::<Vec<u8>, _>("checksum")
                .map_err(|_| Failure::Schema)?
                .as_slice()
                != migration.checksum.as_ref()
            || !row
                .try_get::<bool, _>("success")
                .map_err(|_| Failure::Schema)?
        {
            return Err(Failure::Schema);
        }
    }
    transaction.commit().await.map_err(|_| Failure::Schema)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_preserves_original_history_and_rejects_new_head() {
        let original = sqlx::migrate!("../../migrations");
        let compatible = migrations();
        for (original, actual) in original.iter().zip(compatible.iter()) {
            assert_eq!(original.checksum, actual.checksum);
            if original.version == 4 {
                assert!(original.sql.contains("CREATE INDEX"));
                assert!(!actual.sql.contains("CREATE INDEX"));
            } else {
                assert_eq!(original.sql, actual.sql);
            }
        }
        let mut changed = migrations();
        changed.migrations.to_mut()[0].no_tx = true;
        assert!(describe(&changed).is_err());
        changed.migrations.to_mut()[0].no_tx = false;
        changed.migrations.to_mut().pop();
        assert!(describe(&changed).is_err());
    }

    #[tokio::test]
    async fn late_migration_failure_rolls_back_every_schema_and_history_write() {
        let Some(admin) = mem_test_support::migrated_pool().await else {
            return;
        };
        let name = format!("memory_schema_fault_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let options = admin.connect_options().as_ref().clone().database(&name);
        let mut connection = PgConnection::connect_with(&options).await.unwrap();
        let mut broken = migrations();
        broken.migrations.to_mut()[54].sql = Cow::Borrowed("SELECT 1/0");
        assert!(matches!(
            prepare(&mut connection, &broken).await,
            Err(Failure::Schema)
        ));
        // A follow-up query drains SQLx's queued outer rollback.
        let tables: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pg_tables WHERE schemaname='public'")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(tables, 0);
        let extensions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname<>'plpgsql'")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(extensions, 0);
        // The same pristine target can be prepared after a confirmed rollback.
        prepare(&mut connection, &migrations()).await.unwrap();
        connection.close().await.unwrap();
        sqlx::query(&format!("DROP DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
    }
}
