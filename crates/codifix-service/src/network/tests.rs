#![allow(clippy::unwrap_used)]
use super::*;
use codifix_record::AuthMode;
use codifix_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn http_requires_current_human_and_feature_opt_in() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let home = Uuid::new_v4();
    let cfg=serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused"},"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"consolidation":{"enabled":false},"background_work":{"enabled":false},"mcp":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"inbox":{"enabled":true,"home_instance_id":home}})).unwrap();
    let mut state = crate::runtime::build_state(cfg, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    state.config.auth.mode = AuthMode::MultiUser;
    let human = Uuid::new_v4();
    let other = Uuid::new_v4();
    let service = Uuid::new_v4();
    for (id, kind) in [
        (human, "human_oidc"),
        (other, "human_oidc"),
        (service, "service_token"),
    ] {
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,$2,'Synthetic','[\"memory_read\"]'::jsonb)").bind(id).bind(kind).execute(&db.pool).await.unwrap();
    }
    let token = format!("mld_{}", Uuid::new_v4());
    let other_token = format!("mld_{}", Uuid::new_v4());
    for (id, token) in [(human, &token), (other, &other_token)] {
        sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,session_kind,expires_at) VALUES($1,$2,$3,$4,'desktop',now()+interval '1 hour')").bind(Uuid::new_v4()).bind(id).bind(crate::auth::hash_secret(token)).bind(crate::auth::hash_secret("synthetic-unused-csrf")).execute(&db.pool).await.unwrap();
    }
    let service_token = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'test',$3,'test')").bind(Uuid::new_v4()).bind(service).bind(crate::auth::hash_secret(&service_token)).execute(&db.pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(
        axum::serve(listener, crate::routes::build_http_app(state.clone())).into_future(),
    );
    let client = reqwest::Client::new();
    let status_url = format!("{origin}/v1/network/status");
    assert_eq!(
        client.get(&status_url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&status_url)
            .bearer_auth(&service_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .get(&status_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["identity_linking_available"], false);
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO inbox_conversations(id,home_instance_id,principal_id,request_digest,title) VALUES($1,$2,$3,'test','Private conversation')").bind(id).bind(home).bind(human).execute(&db.pool).await.unwrap();
    let url = format!("{origin}/v1/inbox/conversations/{id}");
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_ne!(
        client
            .get(&url)
            .bearer_auth(&other_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{origin}/v1/network/oauth/client-metadata.json"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(human)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_ne!(
        client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    handle.abort();
    let _ = handle.await;
    state.config.inbox = None;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(
        axum::serve(listener, crate::routes::build_http_app(state.clone())).into_future(),
    );
    let response = client
        .get(format!("{origin}/v1/network/status"))
        .bearer_auth(&other_token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["enabled"], false);
    assert_eq!(
        client
            .get(format!("{origin}/v1/inbox/conversations"))
            .bearer_auth(&other_token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    handle.abort();
    let _ = handle.await;
    state.pool().unwrap().close().await;
    db.cleanup().await.unwrap();
}

#[test]
fn human_route_policy_rejects_machine_and_unknown_routes() {
    use crate::auth::{policy_for, read_only_request_allowed};
    use axum::http::Method;
    for path in [
        "/v1/network/status",
        "/v1/network/identity/link/00000000-0000-0000-0000-000000000001",
        "/v1/inbox/conversations",
    ] {
        assert!(policy_for(&Method::GET, path).is_some());
    }
    assert!(policy_for(&Method::POST, "/v1/inbox/arbitrary").is_none());
    assert!(!read_only_request_allowed(
        &Method::POST,
        "/v1/inbox/conversations"
    ));
    assert!(read_only_request_allowed(
        &Method::POST,
        "/v1/network/discover"
    ));
}
