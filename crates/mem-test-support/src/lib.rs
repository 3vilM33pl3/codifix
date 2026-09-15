// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

const PGVECTOR_MIGRATION_WITHOUT_LEGACY_HNSW: &str = r#"
CREATE EXTENSION IF NOT EXISTS vector;

DROP INDEX IF EXISTS idx_memory_chunks_embedding_hnsw;

ALTER TABLE memory_chunks
    DROP COLUMN IF EXISTS embedding;

ALTER TABLE memory_chunks
    ADD COLUMN IF NOT EXISTS embedding vector;
"#;

pub fn unique_project_slug(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

pub async fn migrated_pool() -> Option<PgPool> {
    let configured = std::env::var_os("MEMORY_LAYER_TEST_DATABASE_URL").is_some();
    match try_migrated_pool().await {
        Ok(pool) => Some(pool),
        Err(error) if configured || require_database() => panic!("{error:#}"),
        Err(_) => None,
    }
}

pub async fn try_migrated_pool() -> Result<PgPool> {
    let database_url = std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").with_context(
        || "MEMORY_LAYER_TEST_DATABASE_URL must point at a PostgreSQL test database with pgvector",
    )?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("connect to MEMORY_LAYER_TEST_DATABASE_URL")?;
    run_migrations(&pool).await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    compatible_migrator()
        .run(pool)
        .await
        .context("run Memory Layer migrations")
}

/// Database-wide claimers must not consume another test's queued work. Requires
/// CREATEDB on the explicitly configured disposable PostgreSQL test instance.
pub struct IsolatedDatabase {
    pub pool: PgPool,
    admin: PgPool,
    name: String,
    reader_role: Option<String>,
}

/// Prepared, disconnected test database. PostgreSQL clones its complete schema
/// and data without rerunning migrations or preparation. Not a production backup
/// or campaign approval mechanism; its privileged admin pool remains trusted.
pub struct FrozenDatabase {
    admin: PgPool,
    name: String,
}

impl FrozenDatabase {
    pub async fn fork(&self) -> Result<IsolatedDatabase> {
        let name = format!("memory_fixture_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name} TEMPLATE {}", self.name))
            .execute(&self.admin)
            .await
            .context("clone prepared disposable database")?;
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_with(
                self.admin
                    .connect_options()
                    .as_ref()
                    .clone()
                    .database(&name),
            )
            .await?;
        Ok(IsolatedDatabase {
            pool,
            admin: self.admin.clone(),
            name,
            reader_role: None,
        })
    }

    /// Give each clone its own login, denied CONNECT to the other clones. The
    /// pool can read benchmark data but only write permitted operational journals.
    pub async fn fork_read_only(&self) -> Result<IsolatedDatabase> {
        let mut db = self.fork().await?;
        let role = format!("r3_reader_{}", Uuid::new_v4().simple());
        let password = Uuid::new_v4().simple().to_string();
        // All interpolated values are generated UUID-based identifiers, not
        // supplied SQL or external credentials.
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}';
             REVOKE CONNECT ON DATABASE {} FROM PUBLIC;
             GRANT CONNECT ON DATABASE {} TO {role};
             GRANT USAGE ON SCHEMA public TO {role};
             GRANT SELECT ON ALL TABLES IN SCHEMA public TO {role};
             GRANT INSERT,UPDATE ON external_effects,external_effect_outcomes,
                 foreground_executions,auth_audit_events TO {role};",
            db.name, db.name
        ))
        .execute(&db.pool)
        .await?;
        let options = db
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .username(&role)
            .password(&password);
        db.pool.close().await;
        db.pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await?;
        db.reader_role = Some(role);
        Ok(db)
    }

    pub async fn cleanup(self) -> Result<()> {
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.name))
            .execute(&self.admin)
            .await?;
        self.admin.close().await;
        Ok(())
    }
}

impl IsolatedDatabase {
    /// Close owned connections before freezing; no connection killing or
    /// template operation targets a caller-supplied database name.
    pub async fn freeze(self) -> Result<FrozenDatabase> {
        anyhow::ensure!(
            self.reader_role.is_none(),
            "freeze a preparation database, not a reader clone"
        );
        self.pool.close().await;
        sqlx::query(&format!(
            "ALTER DATABASE {} ALLOW_CONNECTIONS false",
            self.name
        ))
        .execute(&self.admin)
        .await?;
        Ok(FrozenDatabase {
            admin: self.admin,
            name: self.name,
        })
    }
    pub async fn new() -> Option<Self> {
        let admin = migrated_pool().await?;
        let name = format!("memory_fixture_{}", Uuid::new_v4().simple());
        // Identifier is generated here from ASCII prefix and UUID, never input.
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .expect("create isolated disposable test database");
        let options = admin.connect_options().as_ref().clone().database(&name);
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .expect("connect isolated test database");
        run_migrations(&pool)
            .await
            .expect("migrate isolated test database");
        Some(Self {
            pool,
            admin,
            name,
            reader_role: None,
        })
    }

    pub async fn cleanup(self) -> Result<()> {
        self.pool.close().await;
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.name))
            .execute(&self.admin)
            .await?;
        if let Some(role) = self.reader_role {
            sqlx::query(&format!("DROP ROLE {role}"))
                .execute(&self.admin)
                .await?;
        }
        // A prepared snapshot and its other forks can share this admin pool.
        // Dropping this handle must not close those live users' connections.
        drop(self.admin);
        Ok(())
    }
}

fn compatible_migrator() -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("../../migrations");
    migrator.migrations = std::borrow::Cow::Owned(
        migrator
            .migrations
            .iter()
            .cloned()
            .map(|mut migration| {
                if migration.version == 4 {
                    migration.sql =
                        std::borrow::Cow::Borrowed(PGVECTOR_MIGRATION_WITHOUT_LEGACY_HNSW);
                }
                migration
            })
            .collect(),
    );
    migrator
}

pub async fn cleanup_project(pool: &PgPool, slug: &str) -> Result<()> {
    sqlx::query("DELETE FROM projects WHERE slug = $1")
        .bind(slug)
        .execute(pool)
        .await
        .with_context(|| format!("cleanup test project {slug}"))?;
    Ok(())
}

fn require_database() -> bool {
    std::env::var("MEMORY_LAYER_TEST_REQUIRE_DB").is_ok_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn frozen_database_resets_reinforcement_and_rejects_new_source_connections() {
        let Some(db) = super::IsolatedDatabase::new().await else {
            return;
        };
        let project = uuid::Uuid::new_v4();
        let canonical = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'r3-snapshot','Synthetic','/synthetic')")
            .bind(project).execute(&db.pool).await.expect("snapshot fixture operation");
        sqlx::query("INSERT INTO memory_scores(canonical_id,project_id,activation,access_count) VALUES($1,$2,4.5,7)")
            .bind(canonical).bind(project).execute(&db.pool).await.expect("snapshot fixture operation");
        let before: String = sqlx::query_scalar("SELECT row_to_json(s)::text FROM memory_scores s")
            .fetch_one(&db.pool)
            .await
            .expect("snapshot fixture operation");
        let original_options = db.pool.connect_options().as_ref().clone();
        let frozen = db.freeze().await.expect("snapshot fixture operation");
        assert!(
            sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(2))
                .connect_with(original_options)
                .await
                .is_err()
        );
        let first = frozen.fork().await.expect("snapshot fixture operation");
        let first_state: String =
            sqlx::query_scalar("SELECT row_to_json(s)::text FROM memory_scores s")
                .fetch_one(&first.pool)
                .await
                .expect("snapshot fixture operation");
        assert_eq!(first_state, before);
        sqlx::query("UPDATE memory_scores SET activation=99,access_count=999")
            .execute(&first.pool)
            .await
            .expect("snapshot fixture operation");
        first.cleanup().await.expect("snapshot fixture operation");
        let second = frozen.fork().await.expect("snapshot fixture operation");
        let restored: String =
            sqlx::query_scalar("SELECT row_to_json(s)::text FROM memory_scores s")
                .fetch_one(&second.pool)
                .await
                .expect("snapshot fixture operation");
        assert_eq!(restored, before);
        second.cleanup().await.expect("snapshot fixture operation");
        let reader_a = frozen
            .fork_read_only()
            .await
            .expect("snapshot fixture operation");
        let reader_b = frozen
            .fork_read_only()
            .await
            .expect("snapshot fixture operation");
        let unchanged: String =
            sqlx::query_scalar("SELECT row_to_json(s)::text FROM memory_scores s")
                .fetch_one(&reader_a.pool)
                .await
                .expect("snapshot fixture operation");
        assert_eq!(unchanged, before);
        let denied = sqlx::query("UPDATE memory_scores SET activation=0")
            .execute(&reader_a.pool)
            .await
            .expect_err("reader must not modify reinforcement");
        assert_eq!(
            denied
                .as_database_error()
                .expect("snapshot fixture operation")
                .code()
                .as_deref(),
            Some("42501")
        );
        let cross_condition = reader_a
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .database(&reader_b.name);
        assert!(
            sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(std::time::Duration::from_secs(2))
                .connect_with(cross_condition)
                .await
                .is_err()
        );
        reader_a
            .cleanup()
            .await
            .expect("snapshot fixture operation");
        // Cleaning another clone must not close the shared admin pool.
        reader_b
            .cleanup()
            .await
            .expect("snapshot fixture operation");
        frozen.cleanup().await.expect("snapshot fixture operation");
    }
    #[test]
    fn compatible_pgvector_migration_preserves_checksum() {
        let original = sqlx::migrate!("../../migrations");
        let compatible = super::compatible_migrator();
        let original_migration = original
            .iter()
            .find(|migration| migration.version == 4)
            .expect("original migration 4");
        let compatible_migration = compatible
            .iter()
            .find(|migration| migration.version == 4)
            .expect("compatible migration 4");

        assert!(original_migration.sql.contains("CREATE INDEX"));
        assert!(!compatible_migration.sql.contains("CREATE INDEX"));
        assert_eq!(compatible_migration.checksum, original_migration.checksum);
    }
}
