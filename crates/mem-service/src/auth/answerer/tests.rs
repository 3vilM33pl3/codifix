// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::{Credential, resolve_principal};
use mem_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};

struct Fixture {
    db: IsolatedDatabase,
    state: AppState,
    actor: AuthenticatedPrincipal,
    project: Uuid,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = IsolatedDatabase::new().await?;
        let config = serde_json::from_value(json!({
            "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-answerer-admin"},
            "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
            "offline":{"enabled":false},"cluster":{"enabled":false},
            "background_work":{"enabled":false},"automation":{"enabled":false},
            "embeddings":{"enabled":false,"create_enabled":false},"mcp":{"enabled":false},
            "consolidation":{"enabled":false},"reinforcement":{"enabled":false},
            "llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}
        }))
        .unwrap();
        let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        let actor = resolve_principal(
            &state,
            Credential::ApiToken("synthetic-answerer-admin".into()),
        )
        .await
        .unwrap()
        .unwrap();
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'homelab','Synthetic Homelab','/synthetic')")
            .bind(project).execute(&db.pool).await.unwrap();
        Some(Self {
            db,
            state,
            actor,
            project,
        })
    }
    fn request(&self) -> ExpertiseAnswererRequest {
        ExpertiseAnswererRequest {
            request_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            project: "homelab".into(),
            name: "Homelab deterministic answerer".into(),
            confirmed: true,
        }
    }
    async fn cleanup(self) {
        self.state.pool().unwrap().close().await;
        self.db.cleanup().await.unwrap();
    }
}

#[tokio::test]
async fn answerer_exact_concurrent_replay_has_no_login_or_implicit_consent() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let request = f.request();
    let (a, b) = tokio::join!(
        provision(&f.state, &f.actor, &request),
        provision(&f.state, &f.actor, &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert!(!a.credential_issued && !b.credential_issued);
    assert_eq!(a.project_id, f.project);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_audit_events WHERE id=$1")
        .bind(request.request_id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for table in ["auth_service_tokens", "auth_web_sessions"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {table} WHERE principal_id=$1"
        ))
        .bind(request.principal_id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'other','Synthetic Other','/synthetic')")
        .bind(other).execute(&f.db.pool).await.unwrap();
    for table in [
        "federation_expertise_collections",
        "federation_expertise_collection_grants",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "identity creation cannot imply collection consent"
        );
    }
    let mut tx = f.db.pool.begin().await.unwrap();
    let permissions = crate::auth::background_permissions(
        &f.state,
        &mut tx,
        &mem_store::jobs::JobActor::Principal(request.principal_id),
        f.project,
    )
    .await
    .unwrap();
    assert_eq!(permissions.names(), vec!["memory_read", "bundle_export"]);
    assert_eq!(
        crate::auth::background_permissions(
            &f.state,
            &mut tx,
            &mem_store::jobs::JobActor::Principal(request.principal_id),
            other
        )
        .await
        .unwrap()
        .names(),
        Vec::<String>::new()
    );
    tx.rollback().await.unwrap();
    let mut altered = request.clone();
    altered.name = "Changed answerer".into();
    assert_eq!(
        provision(&f.state, &f.actor, &altered)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    altered = request.clone();
    altered.request_id = Uuid::new_v4();
    assert_eq!(
        provision(&f.state, &f.actor, &altered)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    altered = request.clone();
    altered.principal_id = Uuid::new_v4();
    assert_eq!(
        provision(&f.state, &f.actor, &altered)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
        .bind(request.principal_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        provision(&f.state, &f.actor, &request)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM auth_project_memberships WHERE principal_id=$1")
            .bind(request.principal_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "retry cannot restore a revoked membership");
    f.cleanup().await;
}

#[tokio::test]
async fn answerer_audit_failure_rolls_back_identity_and_project_access() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    sqlx::raw_sql("CREATE FUNCTION reject_answerer_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='expertise.answerer.provision' THEN RAISE EXCEPTION 'synthetic fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_answerer_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION reject_answerer_audit();")
        .execute(&f.db.pool).await.unwrap();
    let request = f.request();
    assert_eq!(
        provision(&f.state, &f.actor, &request)
            .await
            .unwrap_err()
            .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    for table in [
        "auth_principals",
        "auth_project_memberships",
        "auth_audit_events",
    ] {
        let column = if table == "auth_project_memberships" {
            "principal_id"
        } else {
            "id"
        };
        let id = if table == "auth_audit_events" {
            request.request_id
        } else {
            request.principal_id
        };
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE {column}=$1"))
                .bind(id)
                .fetch_one(&f.db.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    f.cleanup().await;
}

#[tokio::test]
async fn answerer_replay_rejects_changed_scope_and_current_authority() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let request = f.request();
    provision(&f.state, &f.actor, &request).await.unwrap();
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'other','Synthetic Other','/synthetic')").bind(other).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'reader')").bind(Uuid::new_v4()).bind(request.principal_id).bind(other).execute(&f.db.pool).await.unwrap();
    assert_eq!(
        provision(&f.state, &f.actor, &request)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    f.state.config.service.read_only = true;
    assert_eq!(
        provision(&f.state, &f.actor, &f.request())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    f.state.config.service.read_only = false;
    f.state.config.evaluation.semantic_time = Some(chrono::Utc::now());
    assert_eq!(
        provision(&f.state, &f.actor, &f.request())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    f.state.config.evaluation.semantic_time = None;
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.actor.id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(
        provision(&f.state, &f.actor, &f.request())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
    f.cleanup().await;
}

#[tokio::test]
async fn answerer_http_requires_global_authority_confirmation_and_csrf() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    f.state.config.auth.public_base_url = Some(origin.clone());
    let (session, csrf) = crate::auth::mint_web_session(&f.state, &f.db.pool, f.actor.id)
        .await
        .unwrap();
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("{origin}/v1/auth/expertise-answerers");
    let request = f.request();
    assert_eq!(
        client
            .post(&url)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(&url)
            .header(
                "cookie",
                format!("memory_session={session}; memory_csrf={csrf}")
            )
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut bad = json!(request);
    bad["permissions"] = json!(["system_admin"]);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth("synthetic-answerer-admin")
            .json(&bad)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    bad = json!(request);
    bad["confirmed"] = json!(false);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth("synthetic-answerer-admin")
            .json(&bad)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let response = client
        .post(&url)
        .header(
            "cookie",
            format!("memory_session={session}; memory_csrf={csrf}"),
        )
        .header("origin", &origin)
        .header("x-csrf-token", &csrf)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: ExpertiseAnswererResponse = response.json().await.unwrap();
    assert!(!body.credential_issued);
    // Even a valid session with exact-project administrator rights is not an
    // installation administrator. No second identity may be provisioned.
    sqlx::query("UPDATE auth_project_memberships SET role='admin',permissions_json=NULL WHERE principal_id=$1")
        .bind(request.principal_id).execute(&f.db.pool).await.unwrap();
    let (limited, limited_csrf) =
        crate::auth::mint_web_session(&f.state, &f.db.pool, request.principal_id)
            .await
            .unwrap();
    assert_eq!(
        client
            .post(&url)
            .header(
                "cookie",
                format!("memory_session={limited}; memory_csrf={limited_csrf}")
            )
            .header("origin", &origin)
            .header("x-csrf-token", &limited_csrf)
            .json(&f.request())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    server.abort();
    let _ = server.await;
    f.cleanup().await;
}

#[tokio::test]
async fn answerer_expired_credential_after_lock_wait_rolls_back() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (secret, _) = crate::auth::mint_web_session(&f.state, &f.db.pool, f.actor.id)
        .await
        .unwrap();
    let actor = resolve_principal(&f.state, Credential::BrowserSession(secret))
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE auth_web_sessions SET expires_at=clock_timestamp()+interval '1 second' WHERE id=$1",
    )
    .bind(actor.session_id)
    .execute(&f.db.pool)
    .await
    .unwrap();
    let mut blocker = f.db.pool.begin().await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:expertise-answerer-provision',0))",
    )
    .execute(&mut *blocker)
    .await
    .unwrap();
    let state = f.state.clone();
    let request = f.request();
    let submitted = request.clone();
    let task = tokio::spawn(async move { provision(&state, &actor, &submitted).await });
    tokio::time::timeout(std::time::Duration::from_secs(3),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(&f.db.pool).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    sqlx::query("SELECT pg_sleep(1.1)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1)")
            .bind(request.principal_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(!exists);
    f.cleanup().await;
}
