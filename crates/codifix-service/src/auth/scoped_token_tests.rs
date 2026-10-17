// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scoped machine authority must not depend on enabling browser OIDC.
use super::*;
use axum::http::StatusCode;
use codifix_record::{AuthRole, AuthServiceTokenCreateRequest, Permission};
use codifix_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[tokio::test]
async fn single_user_scoped_tokens_http_mcp_and_browser() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let config = serde_json::from_value(json!({
        "service":{"bind_addr":address.to_string(),"api_token":"synthetic-local-owner"},
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "auth":{"mode":"single_user","public_base_url":base},
        "offline":{"enabled":false},"cluster":{"enabled":false},
        "background_work":{"enabled":false},"automation":{"enabled":false},
        "embeddings":{"enabled":false,"create_enabled":false},
        "mcp":{"enabled":true,"http_enabled":true,"require_token":true},
        "consolidation":{"enabled":false},"reinforcement":{"enabled":false},
        "llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}
    }))
    .unwrap();
    let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    for slug in ["allowed", "other"] {
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,$2,'/synthetic')")
            .bind(Uuid::new_v4())
            .bind(slug)
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let owner = resolve_principal(&state, Credential::ApiToken("synthetic-local-owner".into()))
        .await
        .unwrap()
        .unwrap();
    let issued = create_service_token(
        &db.pool,
        &owner,
        AuthServiceTokenCreateRequest {
            name: "synthetic scoped MCP".into(),
            project: "allowed".into(),
            role: AuthRole::Reader,
            expires_in_seconds: Some(3600),
        },
    )
    .await
    .unwrap();
    let token = issued.token.as_ref().unwrap();
    let scoped = resolve_principal(&state, Credential::ApiToken(token.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(scoped.credential_source, CredentialSource::ServiceToken);
    assert_eq!(scoped.token_id, Some(issued.id));
    assert!(!scoped.has_anywhere(Permission::AuthManage));
    let mut no_owner_token = state.clone();
    no_owner_token.api_token.clear();
    assert!(
        resolve_principal(&no_owner_token, Credential::ApiToken(String::new()))
            .await
            .is_err()
    );
    let app = crate::routes::build_http_app(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    for (project, expected) in [
        ("allowed", StatusCode::OK),
        ("other", StatusCode::FORBIDDEN),
    ] {
        let response = client
            .get(format!("{base}/v1/projects/{project}/memories"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    assert_eq!(
        client
            .get(format!("{base}/v1/auth/tokens"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":codifix_mcp::MCP_PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"synthetic-auth-test","version":"1"}}});
    let response = client
        .post(format!("{base}/mcp"))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let session = response
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let _ = response.bytes().await.unwrap();
    let response = client
        .post(format!("{base}/mcp"))
        .bearer_auth(token)
        .header("Accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    for (id, project, allowed) in [(2, "allowed", true), (3, "other", false)] {
        let response = client.post(format!("{base}/mcp")).bearer_auth(token).header("Accept","application/json, text/event-stream").header("mcp-session-id",&session).json(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"memory_list_memories","arguments":{"project":project}}})).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.unwrap();
        let payload: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
            body.lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
                .find(|value| value["id"] == id)
                .expect("MCP stream contains the matching JSON-RPC response")
        });
        if allowed {
            assert!(payload.get("error").is_none(), "{payload}");
            assert_ne!(payload["result"]["isError"], true);
        } else {
            assert!(
                payload.get("error").is_some() || payload["result"]["isError"] == true,
                "{payload}"
            );
        }
    }
    for (query, value) in [
        (
            "UPDATE auth_service_tokens SET expires_at=now()-interval '1 second' WHERE id=$1",
            true,
        ),
        (
            "UPDATE auth_service_tokens SET expires_at=now()+interval '1 hour',revoked_at=now() WHERE id=$1",
            true,
        ),
        (
            "UPDATE auth_service_tokens SET revoked_at=NULL WHERE id=$1",
            false,
        ),
    ] {
        sqlx::query(query)
            .bind(issued.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let response = client
            .post(format!("{base}/mcp"))
            .bearer_auth(token)
            .header("Accept", "application/json, text/event-stream")
            .json(&init)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if value {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::OK
            }
        );
        let response = client
            .get(format!("{base}/v1/projects/allowed/memories"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if value {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::OK
            }
        );
    }
    sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
        .bind(scoped.id)
        .execute(&db.pool)
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!("{base}/v1/projects/allowed/memories"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{base}/mcp"))
            .bearer_auth(token)
            .header("Accept", "application/json, text/event-stream")
            .json(&init)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(scoped.id)
        .execute(&db.pool)
        .await
        .unwrap();
    for credential in [token.as_str(), "mlt_unknown_synthetic"] {
        let response = client
            .post(format!("{base}/mcp"))
            .bearer_auth(credential)
            .header("Accept", "application/json, text/event-stream")
            .json(&init)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            client
                .get(format!("{base}/v1/auth/me"))
                .bearer_auth(credential)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(
            resolve_principal(&state, Credential::ApiToken(credential.into()))
                .await
                .is_err()
        );
    }
    // Invalid supplied credentials never fall back to anonymous read access.
    assert_eq!(
        client
            .get(format!("{base}/v1/projects/other/memories"))
            .bearer_auth("unknown")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{base}/v1/projects/other/memories"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let response = client
        .post(format!("{base}/v1/auth/session/bootstrap"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find(|h| h.starts_with("memory_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(
        client
            .get(format!("{base}/v1/auth/me"))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        resolve_principal(&state, Credential::ApiToken("synthetic-local-owner".into()))
            .await
            .unwrap()
            .unwrap()
            .credential_source,
        CredentialSource::LocalApiToken
    );
    server.abort();
    let _ = server.await;
    state.pool().unwrap().close().await;
    db.cleanup().await.unwrap();
}
