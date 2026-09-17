// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_record::{AuthMode, SupervisorArea};
use mem_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Explicit opt-in: built web assets, Chromium and the lean CLI are prerequisites.
/// Nothing is created in an existing application DB or deployment.
#[tokio::test]
#[ignore = "requires built Web/lean CLI and installed Playwright Chromium"]
async fn supervisor_real_browser_cli_rehearsal() {
    let db = IsolatedDatabase::new()
        .await
        .expect("required disposable database");
    let instance = Uuid::new_v4();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository");
    assert!(
        root.join("web/dist/index.html").is_file(),
        "build Web assets first"
    );
    let binary = std::env::var("MEMORY_SUPERVISOR_REHEARSAL_CLI").expect("explicit built lean CLI");
    assert!(std::path::Path::new(&binary).is_absolute());
    let config=serde_json::from_value(json!({
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-supervisor-rehearsal","read_only":true,"web_root":root.join("web/dist")},
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "offline":{"enabled":false},"cluster":{"enabled":false},
        "reinforcement":{"enabled":false},"consolidation":{"enabled":false},
        "background_work":{"enabled":false},"mcp":{"enabled":false},
        "embeddings":{"enabled":false,"create_enabled":false},
        "llm":{"provider":"ollama","base_url":"http://127.0.0.1:1","model":"disabled-synthetic"},
        "supervisor":{"instance_id":instance,"area":"workshop"}
    })).expect("synthetic config");
    let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .expect("state");
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'memory','Synthetic supervisor rehearsal','/synthetic')")
        .bind(Uuid::new_v4()).execute(&db.pool).await.expect("project");
    let store = SupervisorStore::new(db.pool.clone());
    store
        .bind_instance(instance, SupervisorArea::Workshop)
        .await
        .expect("bind");
    store
        .install_policy(SupervisorArea::Workshop, Uuid::new_v4())
        .await
        .expect("paused policy");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    state.config.auth.public_base_url = Some(url.clone());
    let router = crate::routes::build_http_app(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .expect("server");
    });
    let result = tokio::process::Command::new("node")
        .arg(root.join("web/tests/supervisor-real-rehearsal.mjs"))
        .env("MEMORY_SUPERVISOR_REHEARSAL_URL", &url)
        .env("MEMORY_SUPERVISOR_REHEARSAL_INSTANCE", instance.to_string())
        .env("MEMORY_SUPERVISOR_REHEARSAL_CLI", binary)
        .kill_on_drop(true)
        .output()
        .await;
    let actions: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_actions")
        .fetch_one(&db.pool)
        .await
        .expect("no work");
    let journal = store
        .journal(SupervisorArea::Workshop, 0, 100)
        .await
        .expect("unchanged journal");
    task.abort();
    let _ = task.await;
    if let Ok(pool) = state.pool() {
        pool.close().await;
    }
    db.cleanup().await.expect("cleanup");
    let output = result.expect("Node rehearsal");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(actions, 0);
    assert_eq!(journal.len(), 1);
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

#[tokio::test]
async fn supervisor_http_requires_global_access_and_never_claims_live_execution() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let instance = Uuid::new_v4();
    let config = serde_json::from_value(json!({
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused","read_only":true},
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "offline":{"enabled":false},"cluster":{"enabled":false},
        "reinforcement":{"enabled":false},"consolidation":{"enabled":false},
        "background_work":{"enabled":false},"mcp":{"enabled":false},
        "supervisor":{"instance_id":instance,"area":"workshop"}
    }))
    .expect("synthetic config");
    let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .expect("synthetic state");
    state.config.auth.mode = AuthMode::MultiUser;
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Synthetic supervisor project','/synthetic')")
        .bind(project).bind(project.to_string()).execute(&db.pool).await.expect("project");
    let mut tokens = Vec::new();
    for global in [false, true] {
        let principal = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'service_token','Synthetic reader',$2)")
            .bind(principal).bind(global.then_some("reader")).execute(&db.pool).await.expect("principal");
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'admin')")
            .bind(Uuid::new_v4()).bind(principal).bind(project).execute(&db.pool).await.expect("membership");
        sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'synthetic',$3,'synthetic')")
            .bind(Uuid::new_v4()).bind(principal).bind(crate::auth::hash_secret(&token)).execute(&db.pool).await.expect("token");
        tokens.push(token);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let url = format!(
        "http://{}/v1/supervisor",
        listener.local_addr().expect("address")
    );
    state.config.auth.public_base_url = Some(url.trim_end_matches("/v1/supervisor").into());
    let router = crate::routes::build_http_app(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server");
    });
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(&url)
            .send()
            .await
            .expect("unauthenticated")
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(&tokens[0])
            .send()
            .await
            .expect("project only")
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .get(&url)
        .bearer_auth(&tokens[1])
        .send()
        .await
        .expect("global");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value = response.json().await.expect("metadata");
    assert_eq!(body["read_only"], true);
    assert_eq!(body["execution"], "not_integrated");
    assert!(body["snapshot"]["area"].is_null());
    let store = SupervisorStore::new(db.pool.clone());
    assert!(store.instance_binding().await.expect("binding").is_none());
    store
        .bind_instance(instance, SupervisorArea::Workshop)
        .await
        .expect("bind");
    let revision = Uuid::new_v4();
    store
        .install_policy(SupervisorArea::Workshop, revision)
        .await
        .expect("policy");
    store
        .set_enabled(SupervisorArea::Workshop, revision, true)
        .await
        .expect("enable policy only");
    let body: Value = client
        .get(&url)
        .bearer_auth(&tokens[1])
        .send()
        .await
        .expect("refresh")
        .json()
        .await
        .expect("metadata");
    assert_eq!(body["snapshot"]["policy"]["enabled"], true);
    assert_eq!(body["execution"], "not_integrated");
    for suffix in [
        "?after=-1",
        "?after=%2B1",
        "?after=",
        "?limit=101",
        "?area=trust",
    ] {
        assert_eq!(
            client
                .get(format!("{url}{suffix}"))
                .bearer_auth(&tokens[1])
                .send()
                .await
                .expect("invalid filter")
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    let action_count: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_actions")
        .fetch_one(&db.pool)
        .await
        .expect("no work created");
    assert_eq!(action_count, 0);
    task.abort();
    let _ = task.await;
    if let Ok(pool) = state.pool() {
        pool.close().await;
    }
    db.cleanup().await.expect("cleanup");
}
