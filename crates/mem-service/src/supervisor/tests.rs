// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_record::SupervisorArea;
use mem_test_support::IsolatedDatabase;
use serde_json::json;
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

async fn observed(pool: &sqlx::PgPool, expected: i64) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM supervisor_health_observations")
                    .fetch_one(pool)
                    .await
                    .expect("synthetic observations");
            if count == expected {
                break;
            }
            assert!(count < expected, "duplicate observation");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("worker should observe after reconnect");
}

#[tokio::test]
async fn health_worker_is_opt_in_stops_on_drop_and_recovers_database_connection() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let instance = Uuid::new_v4();
    let config = serde_json::from_value(json!({
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused"},
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "offline":{"enabled":false},"cluster":{"enabled":false},
        "reinforcement":{"enabled":false},"consolidation":{"enabled":false},
        "background_work":{"enabled":false},"mcp":{"enabled":false},
        "embeddings":{"enabled":false,"create_enabled":false},
        "llm":{"provider":"ollama","base_url":"http://127.0.0.1:1","model":"disabled-synthetic"},
        "supervisor":{"instance_id":instance,"area":"workshop","deterministic_checks":true}
    }))
    .expect("synthetic configuration");
    let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .expect("synthetic state");
    let store = SupervisorStore::new(db.pool.clone());
    let revision = Uuid::new_v4();
    store
        .bind_instance(instance, SupervisorArea::Workshop)
        .await
        .expect("bind");
    store
        .install_policy(SupervisorArea::Workshop, revision)
        .await
        .expect("policy");
    store
        .set_enabled(SupervisorArea::Workshop, revision, true)
        .await
        .expect("enable checks");
    for guard in ["absent", "disabled", "readonly", "evaluation", "relay"] {
        let mut denied = state.clone();
        match guard {
            "absent" => denied.config.supervisor = None,
            "disabled" => {
                denied
                    .config
                    .supervisor
                    .as_mut()
                    .expect("identity")
                    .deterministic_checks = false
            }
            "readonly" => denied.config.service.read_only = true,
            "evaluation" => denied.config.evaluation.semantic_time = Some(chrono::Utc::now()),
            "relay" => denied.role = crate::ServiceRole::Relay,
            _ => unreachable!(),
        }
        assert!(start(denied).is_none(), "{guard}");
    }
    observed(&db.pool, 0).await;
    let pool = state
        .pool
        .write()
        .expect("pool lock")
        .take()
        .expect("connection");
    let worker = start(state.clone()).expect("explicit opt-in");
    // An offline start must wait for the pool; never bootstrap an alternate DB.
    tokio::time::sleep(Duration::from_millis(50)).await;
    observed(&db.pool, 0).await;
    state.set_pool(pool.clone());
    observed(&db.pool, 1).await;
    drop(worker);
    tokio::task::yield_now().await;
    let worker = start(state.clone()).expect("restart");
    tokio::time::sleep(Duration::from_millis(100)).await;
    observed(&db.pool, 1).await;
    drop(worker);
    tokio::task::yield_now().await;
    sqlx::query(
        "UPDATE supervisor_health_schedule SET next_due_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&db.pool)
    .await
    .expect("synthetic next period");
    let worker = start(state.clone()).expect("restart when due");
    observed(&db.pool, 2).await;
    drop(worker);
    tokio::task::yield_now().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_attempts")
            .fetch_one(&db.pool)
            .await
            .expect("no model calls"),
        0
    );
    pool.close().await;
    db.cleanup().await.expect("cleanup");
}
