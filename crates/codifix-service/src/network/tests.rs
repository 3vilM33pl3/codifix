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
    let history_url = format!("{url}/messages?before_sequence=100&limit=20");
    let response = client
        .get(&history_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let page: Value = response.json().await.unwrap();
    assert_eq!(page["messages"], json!([]));
    assert_eq!(page["has_more"], false);
    assert_ne!(
        client
            .get(&history_url)
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

/// Exercise the post-broker transaction boundary with a real protected Unix
/// socket and session revocation/unlink occurring during the external poll.
#[cfg(unix)]
#[tokio::test]
async fn identity_start_cleanup_and_session_bound_completion_survive_retries() {
    use crate::auth::CredentialSource;
    use codifix_record::{AuthPrincipalKind, PermissionSet};
    use std::{
        collections::BTreeMap,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let home = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Identity fixture')").bind(owner).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,session_kind,expires_at) VALUES($1,$2,$3,$4,'desktop',now()+interval '1 hour')").bind(session).bind(owner).bind(crate::auth::hash_secret(&Uuid::new_v4().to_string())).bind(vec![0u8;32]).execute(&db.pool).await.unwrap();
    let dir = std::env::temp_dir().join(format!("codifix-inbox-identity-{}", Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("broker.sock");
    let cfg=serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused"},"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"consolidation":{"enabled":false},"background_work":{"enabled":false},"mcp":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"inbox":{"enabled":true,"home_instance_id":home,"identity_broker_socket":socket}})).unwrap();
    let state = crate::runtime::build_state(cfg, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    let principal = AuthenticatedPrincipal {
        id: owner,
        kind: AuthPrincipalKind::HumanOidc,
        display_name: "Fixture".into(),
        email: None,
        issuer: None,
        subject: None,
        groups: vec![],
        global_role: None,
        global: PermissionSet::EMPTY,
        project_roles: BTreeMap::new(),
        credential_source: CredentialSource::NativeSession,
        token_id: None,
        session_id: Some(session),
        session_csrf_hash: None,
    };
    let mode = Arc::new(Mutex::new("complete".to_owned()));
    let polls = Arc::new(AtomicUsize::new(0));
    let acks = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o660)).unwrap();
    let pool = db.pool.clone();
    let mode2 = mode.clone();
    let polls2 = polls.clone();
    let acks2 = acks.clone();
    let broker = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let (header_end, length) = loop {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..i]);
                    let length = headers
                        .lines()
                        .find_map(|s| {
                            s.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|s| s.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= i + 4 + length {
                        break (i + 4, length);
                    }
                }
            };
            let path = String::from_utf8_lossy(&bytes[..header_end])
                .lines()
                .next()
                .unwrap()
                .to_owned();
            let body: Value =
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            let mut response_status = "200 OK";
            let result = if path.contains("/begin ") {
                if mode2.lock().unwrap().as_str() == "begin_failed" {
                    response_status = "400 Bad Request";
                    json!({"error":"unresolvable_handle"})
                } else {
                    json!({"authorization_url":"https://issuer.example.org/authorize?request_uri=synthetic","expires_at":chrono::Utc::now()+chrono::Duration::minutes(5)})
                }
            } else if path.contains("/poll ") {
                polls2.fetch_add(1, Ordering::SeqCst);
                let current = mode2.lock().unwrap().clone();
                let attempt: Uuid =
                    serde_json::from_value(body["binding"]["attempt_id"].clone()).unwrap();
                if current == "cancelled" {
                    sqlx::query("UPDATE inbox_identity_attempts SET status='cancelled',authorization_url=NULL WHERE id=$1").bind(attempt).execute(&pool).await.unwrap();
                }
                if current == "revoked" {
                    sqlx::query("UPDATE auth_web_sessions SET revoked_at=now() WHERE id=$1")
                        .bind(session)
                        .execute(&pool)
                        .await
                        .unwrap();
                }
                json!({"status":"complete","did":"did:plc:syntheticverifiedidentity","handle":"person.example.org","pds":"https://pds.example.org","verified_at":chrono::Utc::now(),"cleanup":"revoked_and_discarded"})
            } else {
                acks2.fetch_add(1, Ordering::SeqCst);
                json!({"ok":true})
            };
            let response = serde_json::to_vec(&result).unwrap();
            let header = format!(
                "HTTP/1.1 {response_status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response.len()
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&response).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });
    *mode.lock().unwrap() = "begin_failed".into();
    for _ in 0..4 {
        assert!(
            link(
                State(state.clone()),
                Extension(principal.clone()),
                Json(Identifier {
                    identifier: "invalid.example.org".into()
                }),
            )
            .await
            .is_err()
        );
    }
    let failed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inbox_identity_attempts WHERE principal_id=$1 AND status='failed'",
    )
    .bind(owner)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(failed, 4);
    *mode.lock().unwrap() = "complete".into();
    let started = link(
        State(state.clone()),
        Extension(principal.clone()),
        Json(Identifier {
            identifier: "person.example.org".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(started.0["status"], "pending");
    // A corrected request is admitted immediately; no ten-minute quota wait.
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inbox_identity_attempts WHERE principal_id=$1 AND status='pending'",
    )
    .bind(owner)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(pending, 1);
    let unlinked = unlink(State(state.clone()), Extension(principal.clone()))
        .await
        .unwrap();
    assert_eq!(unlinked.0["unlinked"], true);
    for scenario in ["conflict", "complete", "cancelled", "revoked"] {
        let conflicting_owner = Uuid::new_v4();
        if scenario == "conflict" {
            sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Other human')").bind(conflicting_owner).execute(&db.pool).await.unwrap();
            sqlx::query("INSERT INTO inbox_identity_links(home_instance_id,principal_id,did,pds) VALUES($1,$2,'did:plc:syntheticverifiedidentity','https://pds.example.org')").bind(home).bind(conflicting_owner).execute(&db.pool).await.unwrap();
        }
        *mode.lock().unwrap() = scenario.into();
        let id = Uuid::new_v4();
        let b = binding(&state, &principal, id).unwrap();
        sqlx::query("INSERT INTO inbox_identity_attempts(id,home_instance_id,principal_id,session_binding,status,expires_at) VALUES($1,$2,$3,$4,'pending',now()+interval '5 minutes')").bind(id).bind(home).bind(owner).bind(b.session_binding).execute(&db.pool).await.unwrap();
        let mut wrong_session = principal.clone();
        wrong_session.session_id = Some(Uuid::new_v4());
        let before_poll = polls.load(Ordering::SeqCst);
        assert!(
            link_poll(State(state.clone()), Extension(wrong_session), Path(id))
                .await
                .is_err()
        );
        assert_eq!(polls.load(Ordering::SeqCst), before_poll);
        let result = link_poll(State(state.clone()), Extension(principal.clone()), Path(id)).await;
        if scenario == "complete" {
            let body = result.unwrap().0;
            assert_eq!(body["status"], "complete");
            let previous = polls.load(Ordering::SeqCst);
            let again = link_poll(State(state.clone()), Extension(principal.clone()), Path(id))
                .await
                .unwrap();
            assert_eq!(again.0["status"], "complete");
            assert_eq!(polls.load(Ordering::SeqCst), previous);
            sqlx::query("DELETE FROM inbox_identity_links WHERE principal_id=$1")
                .bind(owner)
                .execute(&db.pool)
                .await
                .unwrap();
        } else if scenario == "cancelled" {
            assert_eq!(result.unwrap().0["status"], "cancelled");
        } else {
            assert!(result.is_err());
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM inbox_identity_links WHERE principal_id=$1")
                .bind(owner)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        if scenario == "conflict" {
            sqlx::query("DELETE FROM inbox_identity_links WHERE principal_id=$1")
                .bind(conflicting_owner)
                .execute(&db.pool)
                .await
                .unwrap();
        }
    }
    assert_eq!(acks.load(Ordering::SeqCst), 2);
    broker.abort();
    let _ = broker.await;
    state.pool().unwrap().close().await;
    db.cleanup().await.unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
