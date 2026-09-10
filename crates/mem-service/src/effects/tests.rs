// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[test]
fn project_policy_cannot_write_another_root_or_follow_an_escaping_symlink() {
    let root = std::env::temp_dir().join(format!("memory-policy-roots-{}", Uuid::new_v4()));
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    assert!(crate::handlers::curation::project_policy_path(&project, &other).is_err());
    assert!(crate::handlers::curation::project_policy_path(&project, &project).is_ok());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&other, project.join(".agents")).unwrap();
        assert!(crate::handlers::curation::project_policy_path(&project, &project).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn acknowledged_file_writes_are_serialized_and_failed_ack_is_not_rollback() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let root = std::env::temp_dir().join(format!("memory-effects-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    let host = Uuid::new_v4().to_string();
    let context = MutationContext::internal("file-test");
    let (a, b) = tokio::join!(
        replace_file(&pool, &host, None, &path, &context, |s| Ok(format!(
            "{s}a = 1\n"
        ))),
        replace_file(&pool, &host, None, &path, &context, |s| Ok(format!(
            "{s}b = 2\n"
        )))
    );
    a.unwrap().require_confirmed().unwrap();
    b.unwrap().require_confirmed().unwrap();
    let actual = std::fs::read_to_string(&path).unwrap();
    assert!(actual.contains("a = 1") && actual.contains("b = 2"));
    // On the writer connection only, reject the post-write outcome record.
    let failing=sqlx::postgres::PgPoolOptions::new().max_connections(1)
        .after_connect(|conn,_|Box::pin(async move {
            sqlx::query("CREATE TEMP TABLE external_effect_outcomes (LIKE public.external_effect_outcomes INCLUDING DEFAULTS, CHECK(FALSE))")
                .execute(conn).await?;
            Ok(())
        })).connect_with((*pool.connect_options()).clone()).await.unwrap();
    let changed = replace_file(&failing, &host, None, &path, &context, |_| {
        Ok("value = 'synthetic-private-input'\n".into())
    })
    .await
    .unwrap();
    assert!(changed.applied);
    assert!(!changed.acknowledged);
    assert!(
        changed
            .require_confirmed()
            .unwrap_err()
            .message
            .contains("has not been rolled back")
    );
    let id = changed.effect.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM external_effects WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "pending"
    );
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM external_effects e WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!stored.to_string().contains("synthetic-private-input"));
    sqlx::query(
        "UPDATE external_effects SET reconcile_after=now()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    reconcile(&host, &pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM external_effects WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "confirmed"
    );
    let mut invalid = context.clone();
    invalid.actor_id = "bad\0actor".into();
    assert!(
        replace_file(&pool, &host, None, &path, &invalid, |_| Ok(
            "must not write".into()
        ))
        .await
        .is_err()
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("synthetic-private-input")
    );
    failing.close().await;
    sqlx::query("DELETE FROM external_effect_outcomes WHERE effect_id IN (SELECT id FROM external_effects WHERE host_id=$1)").bind(&host).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM external_effects WHERE host_id=$1")
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
