// SPDX-License-Identifier: AGPL-3.0-or-later
//! Verify an owner-prepared history without SQLx's CREATE TABLE IF NOT EXISTS.
//! This is not a schema-drift audit or permission to upgrade with live writers.
use super::*;

pub(super) async fn verify_history(pool: &PgPool) -> Result<()> {
    let applied: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
        "SELECT version,checksum,success FROM public._sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .context(
        "read prepared migration history; schema owner must initialize it before service startup",
    )?;
    let migrator = compatible_migrator();
    anyhow::ensure!(
        applied.len() == migrator.iter().count()
            && applied.iter().zip(migrator.iter()).all(
                |((version, checksum, success), expected)| {
                    *success
                        && *version == expected.version
                        && checksum.as_slice() == expected.checksum.as_ref()
                }
            ),
        "prepared migration history does not exactly match this release; stop writers and reconcile using the schema owner"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::ConnectOptions;

    #[tokio::test]
    async fn verify_only_bootstraps_as_non_owner_without_create_or_migration_writes() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let role = format!("prepared_runtime_{}", Uuid::new_v4().simple());
        let password = Uuid::new_v4().simple().to_string();
        sqlx::raw_sql(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}';
             REVOKE CREATE ON SCHEMA public FROM PUBLIC;
             GRANT USAGE ON SCHEMA public TO {role};
             GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO {role};
             GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO {role};
             REVOKE INSERT,UPDATE,DELETE ON public._sqlx_migrations FROM {role};"
        ))
        .execute(&db.pool)
        .await
        .unwrap();
        let options = db
            .pool
            .connect_options()
            .as_ref()
            .clone()
            .username(&role)
            .password(&password);
        let cfg: AppConfig = serde_json::from_value(serde_json::json!({
            "service":{},
            "database":{"url": options.to_url_lossy().to_string(), "migrations":"verify_only"},
            "offline":{"enabled":false},"cluster":{"enabled":false}
        }))
        .unwrap();
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        let runtime = connect_primary_pool(&cfg).await.unwrap();
        let can_create: bool =
            sqlx::query_scalar("SELECT has_schema_privilege(current_user,'public','CREATE')")
                .fetch_one(&runtime)
                .await
                .unwrap();
        assert!(!can_create);
        assert!(
            sqlx::query("CREATE TABLE public.forbidden_runtime_ddl(id integer)")
                .execute(&runtime)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE public._sqlx_migrations SET success=false")
                .execute(&runtime)
                .await
                .is_err()
        );
        let owner_count: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_principals")
            .fetch_one(&runtime)
            .await
            .unwrap();
        assert!(owner_count > 0, "normal application bootstrap still occurs");
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&runtime)
            .await
            .unwrap();
        assert_eq!(before, after);
        runtime.close().await;
        sqlx::raw_sql(&format!("DROP OWNED BY {role}; DROP ROLE {role};"))
            .execute(&db.pool)
            .await
            .unwrap();
        db.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn missing_dirty_changed_extra_and_absent_history_fail_closed_without_repair() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        verify_history(&db.pool).await.unwrap();
        let original: Vec<u8> =
            sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version=1")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        for sql in [
            "UPDATE _sqlx_migrations SET success=false WHERE version=1",
            "UPDATE _sqlx_migrations SET checksum='changed'::bytea WHERE version=1",
        ] {
            sqlx::query(sql).execute(&db.pool).await.unwrap();
            assert!(verify_history(&db.pool).await.is_err());
            sqlx::query("UPDATE _sqlx_migrations SET success=true,checksum=$1 WHERE version=1")
                .bind(&original)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(9999,'synthetic extra',true,''::bytea,0)").execute(&db.pool).await.unwrap();
        assert!(verify_history(&db.pool).await.is_err());
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=9999")
            .execute(&db.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=1")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(verify_history(&db.pool).await.is_err());
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version=1")
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        sqlx::query("DROP TABLE _sqlx_migrations")
            .execute(&db.pool)
            .await
            .unwrap();
        assert!(verify_history(&db.pool).await.is_err());
        db.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn prepared_history_errors_cannot_fall_back_to_a_relay() {
        let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        sqlx::query("UPDATE _sqlx_migrations SET success=false WHERE version=1")
            .execute(&db.pool)
            .await
            .unwrap();
        let cfg: AppConfig = serde_json::from_value(serde_json::json!({
            "service":{},
            "database":{"url":db.pool.connect_options().to_url_lossy().to_string(),"migrations":"verify_only"},
            "offline":{"enabled":false},"cluster":{"enabled":true}
        })).unwrap();
        assert!(build_state(cfg, Arc::new(Mutex::new(None))).await.is_err());
        let owners: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_principals")
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(
            owners, 0,
            "invalid history cannot bootstrap application state"
        );
        db.cleanup().await.unwrap();
    }
}
