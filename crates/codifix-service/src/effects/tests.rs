// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn provider_intent_precedes_request_and_interrupted_execution_is_not_replayed() {
    use axum::{Json, Router, extract::State, routing::post};
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Fake {
        pool: sqlx::PgPool,
        host: String,
        received: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }
    async fn respond(State(fake): State<Fake>) -> Json<serde_json::Value> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM external_effects WHERE host_id=$1 AND status='pending'",
        )
        .bind(&fake.host)
        .fetch_one(&fake.pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "provider must observe the committed intent first");
        fake.received.notify_one();
        fake.release.notified().await;
        Json(serde_json::json!({"choices":[{"message":{"content":"{}"}}]}))
    }
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let host = Uuid::new_v4().to_string();
    let fake = Fake {
        pool: pool.clone(),
        host: host.clone(),
        received: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let url = format!("http://{address}/v1");
    let router = Router::new()
        .route("/v1/chat/completions", post(respond))
        .route("/v1/watchers/restart-local", post(respond))
        .with_state(fake.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let config = serde_json::from_value(serde_json::json!({
        "service":{"bind_addr":"127.0.0.1:0","api_token":"fixture"},
        "database":{"url":std::env::var("CODIFIX_TEST_DATABASE_URL").unwrap()},
        "offline":{"enabled":false},"cluster":{"enabled":false,"service_id":host},
        "reinforcement":{"enabled":false},"llm_audit":{"enabled":false},
        "llm":{"provider":"ollama","base_url":url,"model":"fixture"},"mcp":{"enabled":false}
    }))
    .unwrap();
    let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    let request = crate::llm::LlmStrictJsonRequest {
        project: "synthetic-unregistered",
        purpose: "provider-test",
        subject: "synthetic subject".into(),
        system_prompt: "private prompt must not enter operational journal",
        user_prompt: "private source must not enter operational journal".into(),
        max_output_tokens_cap: 10,
    };
    let call = crate::llm::call_llm_strict_json(&state, &request);
    tokio::pin!(call);
    tokio::select! {
        _=fake.received.notified()=>{},
        result=&mut call=>panic!("provider completed before release: {:?}",result.err()),
        _=tokio::time::sleep(std::time::Duration::from_secs(5))=>panic!("provider not reached"),
    }
    fake.release.notify_one();
    assert_eq!(call.await.unwrap().content, "{}");
    fake.release.notify_one();
    assert_eq!(
        crate::repository::handlers::project::summarize_resume_with_llm(
            &state,
            "synthetic-unregistered",
            "resume-summary",
            "private resume pack",
        )
        .await
        .unwrap(),
        "{}"
    );
    fake.received.notified().await;
    let source = state.clone();
    let task = tokio::spawn(async move {
        let request = crate::llm::LlmStrictJsonRequest {
            project: "synthetic-unregistered",
            purpose: "interrupted-provider",
            subject: "fixture".into(),
            system_prompt: "fixture",
            user_prompt: "fixture".into(),
            max_output_tokens_cap: 10,
        };
        crate::llm::call_llm_strict_json(&source, &request).await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), fake.received.notified())
        .await
        .unwrap();
    task.abort();
    assert!(matches!(task.await,Err(error) if error.is_cancelled()));
    sqlx::query("UPDATE external_effects SET reconcile_after=now()-interval '1 second' WHERE host_id=$1 AND status='pending'").bind(&host).execute(&pool).await.unwrap();
    reconcile(&host, &pool).await.unwrap();
    fake.release.notify_waiters();
    state.cluster.peers.lock().unwrap().insert(
        "remote-fixture".into(),
        crate::state::ClusterPeer {
            service_id: "remote-fixture".into(),
            advertise_addr: address,
            version: "fixture".into(),
            priority: 0,
            last_seen: chrono::Utc::now(),
        },
    );
    fake.release.notify_one();
    crate::repository::events::dispatch_watcher_restart(
        &state,
        &codifix_record::WatcherRestartRequest {
            project: "synthetic-unregistered".into(),
            watcher_id: "synthetic-watcher".into(),
            host_service_id: "remote-fixture".into(),
            agent_session_id: None,
        },
    )
    .await
    .unwrap();
    let rows: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM external_effects e WHERE host_id=$1")
            .bind(&host)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .any(|r| r["resource"] == "resume-summary" && r["status"] == "confirmed")
    );
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("private resume pack")
    );
    assert!(
        rows.iter()
            .any(|r| r["kind"] == "watcher_restart" && r["status"] == "confirmed")
    );
    assert!(rows.iter().any(|r| r["status"] == "confirmed"));
    assert!(rows.iter().any(|r| r["status"] == "uncertain"));
    assert!(
        !serde_json::to_string(&rows)
            .unwrap()
            .contains("private prompt")
    );
    server.abort();
    state.pool().unwrap().close().await;
    sqlx::query("DELETE FROM external_effect_outcomes WHERE effect_id IN (SELECT id FROM external_effects WHERE host_id=$1)").bind(&host).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM external_effects WHERE host_id=$1")
        .bind(host)
        .execute(&pool)
        .await
        .unwrap();
}

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

#[test]
fn atomic_file_replacement_confirms_creation_and_existing_target() {
    let root = std::env::temp_dir().join(format!("memory-effects-atomic-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("configuration with spaces.toml");
    atomic_replace(&path, b"first = true\n", Uuid::new_v4()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"first = true\n");
    atomic_replace(&path, b"second = true\n", Uuid::new_v4()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second = true\n");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn acknowledged_file_writes_are_serialized_and_failed_ack_is_not_rollback() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
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
