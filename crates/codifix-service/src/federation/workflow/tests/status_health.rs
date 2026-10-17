// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use axum::{Router, routing::get};
use tokio::sync::Notify;

#[tokio::test]
async fn status_rechecks_credentials_project_membership_and_grant_after_gateway_wait() {
    for change in ["token", "session", "membership", "grant"] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        f.grant(f.reader_id, true).await;
        let entered = Arc::new(Notify::new());
        let released = Arc::new(Notify::new());
        let observed = entered.clone();
        let resume = released.clone();
        let router = Router::new().route(
            "/v1/status",
            get(move || {
                let observed = observed.clone();
                let resume = resume.clone();
                async move {
                    observed.notify_one();
                    resume.notified().await;
                    StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        );
        let (url, task) = serve(router).await;
        let client = GatewayClient::new(
            Endpoint::new(&url, true).unwrap(),
            "synthetic-gateway-status-credential".repeat(2),
            f.runtime.space,
            f.runtime.authority,
            f.security.trust(),
        )
        .unwrap();
        let sync = InstanceSync::new(f.pool.clone(), client, f.runtime.instance)
            .await
            .unwrap();
        *f.state.federation.write().unwrap() = Some(Arc::new(ConfiguredInstance {
            sync,
            ..(*f.runtime).clone()
        }));
        let mut request = reqwest::Client::new().get(f.path(""));
        let session_id = Uuid::new_v4();
        if change == "session" {
            let session = format!("synthetic-session-{}", Uuid::new_v4());
            sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,clock_timestamp()+interval '1 hour')")
                .bind(session_id).bind(f.reader_id).bind(crate::auth::hash_secret(&session))
                .bind(crate::auth::hash_secret("synthetic-csrf")).execute(&f.pool).await.unwrap();
            request = request.header("cookie", format!("memory_session={session}"));
        } else {
            request = request.bearer_auth(&f.reader);
        }
        let response = tokio::spawn(async move { request.send().await.unwrap() });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        match change {
            "token" => {
                sqlx::query("UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE principal_id=$1").bind(f.reader_id).execute(&f.pool).await.unwrap();
            }
            "session" => {
                sqlx::query(
                    "UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1",
                )
                .bind(session_id)
                .execute(&f.pool)
                .await
                .unwrap();
            }
            "membership" => {
                sqlx::query(
                    "DELETE FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2",
                )
                .bind(f.reader_id)
                .bind(f.runtime.project)
                .execute(&f.pool)
                .await
                .unwrap();
            }
            "grant" => {
                sqlx::query("UPDATE federation_local_grants SET enabled=false WHERE principal_id=$1 AND space=$2").bind(f.reader_id).bind(f.runtime.space.as_uuid()).execute(&f.pool).await.unwrap();
            }
            _ => unreachable!(),
        }
        released.notify_one();
        let response = response.await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{change}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = response.text().await.unwrap();
        assert!(!body.contains("retained_through"));
        assert!(!body.contains("NEVER_SHARE"));
        task.abort();
    }
}

#[tokio::test]
async fn status_remains_readable_in_read_only_mode_without_claiming_unknown_gateway_health() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.grant(f.reader_id, true).await;
    let mut readonly = f.state.clone();
    readonly.config.service.read_only = true;
    let (url, task) = serve(crate::routes::build_http_app(readonly)).await;
    let response = reqwest::Client::new()
        .get(format!(
            "{url}/v1/projects/{}/federation/{}",
            f.slug,
            f.runtime.space.as_uuid()
        ))
        .bearer_auth(&f.reader)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["gateway"]["credentials"]["state"], "unknown");
    assert_eq!(body["gateway"]["credentials"]["publication_usable"], false);
    task.abort();
}
