use super::*;
use mem_test_support::IsolatedDatabase;
use serde_json::json;
use sqlx::ConnectOptions;
use uuid::Uuid;

#[tokio::test]
async fn service_opt_in_rejects_other_areas_readonly_evaluation_relay_and_unsafe_manifest() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let config = serde_json::from_value(json!({
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused"},
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "offline":{"enabled":false},"cluster":{"enabled":false},
        "reinforcement":{"enabled":false},"consolidation":{"enabled":false},
        "background_work":{"enabled":false},"mcp":{"enabled":false},
        "embeddings":{"enabled":false,"create_enabled":false},
        "llm":{"provider":"ollama","base_url":"http://127.0.0.1:1","model":"disabled-synthetic"},
        "supervisor":{"instance_id":Uuid::new_v4(),"area":"workshop",
            "model_work":{"manifest":"/nonexistent/synthetic-work.json","sha256":"0".repeat(64)}}
    }))
    .expect("synthetic configuration");
    let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .expect("state");
    for reason in [
        "not-configured",
        "trust",
        "homelab",
        "general",
        "readonly",
        "evaluation",
        "relay",
    ] {
        let mut denied = state.clone();
        denied.supervisor_model = Status::default();
        match reason {
            "not-configured" => {
                denied
                    .config
                    .supervisor
                    .as_mut()
                    .expect("identity")
                    .model_work = None
            }
            "trust" => {
                denied.config.supervisor.as_mut().expect("identity").area = SupervisorArea::Trust
            }
            "homelab" => {
                denied.config.supervisor.as_mut().expect("identity").area = SupervisorArea::Homelab
            }
            "general" => {
                denied.config.supervisor.as_mut().expect("identity").area = SupervisorArea::General
            }
            "readonly" => denied.config.service.read_only = true,
            "evaluation" => denied.config.evaluation.semantic_time = Some(chrono::Utc::now()),
            "relay" => denied.role = crate::ServiceRole::Relay,
            _ => unreachable!(),
        }
        assert!(start(denied.clone()).is_none(), "{reason}");
        if reason != "not-configured" {
            assert_eq!(
                denied
                    .supervisor_model
                    .snapshot()
                    .expect("denied status")
                    .state,
                Phase::Stopped
            );
        }
    }
    let worker = start(state.clone()).expect("configured, validates asynchronously");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if state
                .supervisor_model
                .snapshot()
                .is_some_and(|s| s.state == Phase::InvalidConfiguration)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("invalid manifest visible before any credential access");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_attempts")
            .fetch_one(&db.pool)
            .await
            .expect("attempts"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_actions")
            .fetch_one(&db.pool)
            .await
            .expect("actions"),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_policies")
            .fetch_one(&db.pool)
            .await
            .expect("no bootstrap"),
        0
    );
    drop(worker);
    db.cleanup().await.expect("cleanup");
}
