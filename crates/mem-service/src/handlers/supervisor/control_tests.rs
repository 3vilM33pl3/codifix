// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_record::{AuthMode, SupervisorArea, SupervisorPolicyRequest};
use mem_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct Fixture {
    db: IsolatedDatabase,
    state: AppState,
    store: SupervisorStore,
    instance: Uuid,
    revision: Uuid,
    admin: Uuid,
    token_id: Uuid,
    admin_token: String,
    project_token: String,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = IsolatedDatabase::new().await?;
        let instance = Uuid::new_v4();
        let config = serde_json::from_value(json!({
            "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-unused","read_only":true},
            "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
            "offline":{"enabled":false},"cluster":{"enabled":false},
            "reinforcement":{"enabled":false},"consolidation":{"enabled":false},
            "background_work":{"enabled":false},"mcp":{"enabled":false},
            "embeddings":{"enabled":false,"create_enabled":false},
            "supervisor":{"instance_id":instance,"area":"workshop"}
        }))
        .expect("synthetic config");
        let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
            .await
            .expect("state");
        state.config.auth.mode = AuthMode::MultiUser;
        state.config.service.read_only = false;
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'synthetic','Synthetic','/synthetic')").bind(project).execute(&db.pool).await.expect("project");
        let admin = Uuid::new_v4();
        let other = Uuid::new_v4();
        let token_id = Uuid::new_v4();
        let admin_token = Uuid::new_v4().to_string();
        let project_token = Uuid::new_v4().to_string();
        for (principal, id, token, global) in [
            (admin, token_id, &admin_token, Some("admin")),
            (other, Uuid::new_v4(), &project_token, None),
        ] {
            sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'service_token','Synthetic',$2)")
                .bind(principal).bind(global).execute(&db.pool).await.expect("principal");
            sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'admin')")
                .bind(Uuid::new_v4()).bind(principal).bind(project).execute(&db.pool).await.expect("project admin");
            sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'synthetic',$3,'synthetic')")
                .bind(id).bind(principal).bind(crate::auth::hash_secret(token)).execute(&db.pool).await.expect("token");
        }
        sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,clock_timestamp()+interval '1 hour')")
            .bind(Uuid::new_v4()).bind(admin).bind(crate::auth::hash_secret("synthetic-supervisor-session"))
            .bind(crate::auth::hash_secret("synthetic-csrf")).execute(&db.pool).await.expect("session");
        let revision = Uuid::new_v4();
        let store = SupervisorStore::new(db.pool.clone());
        store
            .bind_instance(instance, SupervisorArea::Workshop)
            .await
            .expect("binding");
        store
            .install_policy(SupervisorArea::Workshop, revision)
            .await
            .expect("paused policy");
        Some(Self {
            db,
            state,
            store,
            instance,
            revision,
            admin,
            token_id,
            admin_token,
            project_token,
        })
    }
    fn request(&self, enabled: bool) -> SupervisorPolicyRequest {
        SupervisorPolicyRequest {
            request_id: Uuid::new_v4(),
            instance_id: self.instance,
            policy_revision: self.revision,
            expected_enabled: !enabled,
            enabled,
        }
    }
    async fn server(&self, readonly: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let url = format!("http://{}", listener.local_addr().expect("address"));
        let mut state = self.state.clone();
        state.config.service.read_only = readonly;
        state.config.auth.public_base_url = Some(url.clone());
        let router = crate::routes::build_http_app(state);
        (
            url,
            tokio::spawn(async move {
                axum::serve(listener, router).await.expect("server");
            }),
        )
    }
    async fn cleanup(self) {
        self.state.pool().expect("pool").close().await;
        self.db.cleanup().await.expect("cleanup");
    }
}

#[tokio::test]
#[ignore = "requires built Web/lean CLI and installed Playwright Chromium"]
async fn supervisor_real_control_browser_cli_rehearsal() {
    let mut f = Fixture::new().await.expect("required disposable database");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository");
    assert!(root.join("web/dist/index.html").is_file());
    let binary = std::env::var("MEMORY_SUPERVISOR_REHEARSAL_CLI").expect("explicit built CLI");
    assert!(std::path::Path::new(&binary).is_absolute());
    f.state.web_root = Some(root.join("web/dist"));
    let (url, server) = f.server(false).await;
    let result = tokio::process::Command::new("node")
        .arg(root.join("web/tests/supervisor-control-rehearsal.mjs"))
        .env("MEMORY_SUPERVISOR_REHEARSAL_URL", url)
        .env(
            "MEMORY_SUPERVISOR_REHEARSAL_INSTANCE",
            f.instance.to_string(),
        )
        .env("MEMORY_SUPERVISOR_REHEARSAL_CLI", binary)
        .env("MEMORY_SUPERVISOR_REHEARSAL_TOKEN", &f.admin_token)
        .kill_on_drop(true)
        .output()
        .await;
    let enabled = f
        .store
        .policy(SupervisorArea::Workshop)
        .await
        .expect("policy")
        .enabled;
    let changes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auth_audit_events WHERE event_type='supervisor.policy.set'",
    )
    .fetch_one(&f.db.pool)
    .await
    .expect("committed controls");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_attempts")
        .fetch_one(&f.db.pool)
        .await
        .expect("no model work");
    server.abort();
    let _ = server.await;
    f.cleanup().await;
    let result = result.expect("Node rehearsal");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!enabled);
    assert_eq!(changes, 6);
    assert_eq!(attempts, 0);
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[tokio::test]
async fn supervisor_controls_are_global_csrf_protected_idempotent_and_revision_bound() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (url, server) = f.server(false).await;
    let endpoint = format!("{url}/v1/supervisor/policy");
    let client = reqwest::Client::new();
    let request = f.request(true);
    for token in [None, Some(f.project_token.as_str())] {
        let mut req = client.post(&endpoint).json(&request);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        assert_eq!(
            req.send().await.expect("denied").status(),
            if token.is_some() {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
    }
    let cookie = "memory_session=synthetic-supervisor-session; memory_csrf=synthetic-csrf";
    assert_eq!(
        client
            .post(&endpoint)
            .header(header::COOKIE, cookie)
            .json(&request)
            .send()
            .await
            .expect("missing csrf")
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header(header::COOKIE, cookie)
            .header("x-csrf-token", "synthetic-csrf")
            .header(header::ORIGIN, "https://wrong.invalid")
            .json(&request)
            .send()
            .await
            .expect("wrong origin")
            .status(),
        StatusCode::FORBIDDEN
    );
    let (a, b) = tokio::join!(
        client
            .post(&endpoint)
            .bearer_auth(&f.admin_token)
            .json(&request)
            .send(),
        client
            .post(&endpoint)
            .bearer_auth(&f.admin_token)
            .json(&request)
            .send()
    );
    let mut replays = 0;
    for response in [a, b] {
        let response = response.expect("control");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: Value = response.json().await.expect("response");
        replays += usize::from(body["replayed"] == true);
    }
    assert_eq!(replays, 1);
    assert!(
        f.store
            .policy(SupervisorArea::Workshop)
            .await
            .expect("enabled")
            .enabled
    );
    let mut altered = request.clone();
    altered.enabled = false;
    altered.expected_enabled = true;
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(&f.admin_token)
            .json(&altered)
            .send()
            .await
            .expect("altered replay")
            .status(),
        StatusCode::CONFLICT
    );
    let pause = f.request(false);
    assert_eq!(
        client
            .post(&endpoint)
            .header(header::COOKIE, cookie)
            .header("x-csrf-token", "synthetic-csrf")
            .header(header::ORIGIN, &url)
            .json(&pause)
            .send()
            .await
            .expect("valid browser pause")
            .status(),
        StatusCode::OK
    );
    // Replaying the old resume reports its receipt but does not undo a later pause.
    let response: Value = client
        .post(&endpoint)
        .bearer_auth(&f.admin_token)
        .json(&request)
        .send()
        .await
        .expect("historical replay")
        .json()
        .await
        .expect("receipt");
    assert_eq!(response["replayed"], true);
    assert!(
        !f.store
            .policy(SupervisorArea::Workshop)
            .await
            .expect("paused")
            .enabled
    );
    let audits: Vec<Value> = sqlx::query_scalar("SELECT metadata_json FROM auth_audit_events WHERE event_type='supervisor.policy.set' AND actor_principal_id=$1")
        .bind(f.admin).fetch_all(&f.db.pool).await.expect("protected actor audit");
    assert_eq!(audits.len(), 2);
    assert!(
        !serde_json::to_string(&audits)
            .expect("audit")
            .contains(&f.admin_token)
    );
    for wrong_instance in [true, false] {
        let mut invalid = f.request(true);
        if wrong_instance {
            invalid.instance_id = Uuid::new_v4();
        } else {
            invalid.policy_revision = Uuid::new_v4();
        }
        assert_eq!(
            client
                .post(&endpoint)
                .bearer_auth(&f.admin_token)
                .json(&invalid)
                .send()
                .await
                .expect("wrong identity")
                .status(),
            StatusCode::CONFLICT
        );
    }
    let stale = f.request(true);
    f.store
        .install_policy(SupervisorArea::Workshop, Uuid::new_v4())
        .await
        .expect("rotate paused");
    assert_eq!(
        client
            .post(&endpoint)
            .bearer_auth(&f.admin_token)
            .json(&stale)
            .send()
            .await
            .expect("stale revision")
            .status(),
        StatusCode::CONFLICT
    );
    server.abort();
    let _ = server.await;
    f.cleanup().await;
}

#[tokio::test]
async fn supervisor_controls_rollback_with_audit_and_recheck_revoked_authority() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (url, server) = f.server(false).await;
    let client = reqwest::Client::new();
    let endpoint = format!("{url}/v1/supervisor/policy");
    let request = f.request(true);
    sqlx::raw_sql("CREATE FUNCTION synthetic_fail_supervisor_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='supervisor.policy.set' THEN RAISE EXCEPTION 'SYNTHETIC_PRIVATE_ERROR'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER synthetic_fail_supervisor_audit AFTER INSERT ON auth_audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_fail_supervisor_audit();")
        .execute(&f.db.pool).await.expect("commit fault");
    let response = client
        .post(&endpoint)
        .bearer_auth(&f.admin_token)
        .json(&request)
        .send()
        .await
        .expect("fault response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !response
            .text()
            .await
            .expect("safe error")
            .contains("SYNTHETIC_PRIVATE_ERROR")
    );
    assert!(
        !f.store
            .policy(SupervisorArea::Workshop)
            .await
            .expect("rollback")
            .enabled
    );
    assert_eq!(
        f.store
            .journal(SupervisorArea::Workshop, 0, 100)
            .await
            .expect("journal")
            .len(),
        1
    );
    sqlx::query("DROP TRIGGER synthetic_fail_supervisor_audit ON auth_audit_events")
        .execute(&f.db.pool)
        .await
        .expect("restore audit");
    let (readonly, readserver) = f.server(true).await;
    assert_eq!(
        client
            .post(format!("{readonly}/v1/supervisor/policy"))
            .bearer_auth(&f.admin_token)
            .json(&request)
            .send()
            .await
            .expect("readonly")
            .status(),
        StatusCode::FORBIDDEN
    );
    let principal = crate::auth::resolve_principal(
        &f.state,
        crate::auth::Credential::ApiToken(f.admin_token.clone()),
    )
    .await
    .expect("authenticate")
    .expect("principal");
    sqlx::query("UPDATE auth_principals SET global_role='reader' WHERE id=$1")
        .bind(f.admin)
        .execute(&f.db.pool)
        .await
        .expect("revoke global permission after authentication");
    let result = super::control::control(
        State(f.state.clone()),
        Extension(principal.clone()),
        Json(request.clone()),
    )
    .await;
    assert_eq!(
        result
            .err()
            .expect("current permission denied")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_principals SET global_role='admin' WHERE id=$1")
        .bind(f.admin)
        .execute(&f.db.pool)
        .await
        .expect("restore synthetic admin");
    let mut readonly = f.state.clone();
    readonly.config.service.read_only = true;
    let mut evaluation = f.state.clone();
    evaluation.config.evaluation.semantic_time = Some(chrono::Utc::now());
    for denied in [readonly, evaluation] {
        let result = super::control::control(
            State(denied),
            Extension(principal.clone()),
            Json(request.clone()),
        )
        .await;
        assert_eq!(
            result
                .err()
                .expect("direct handler guard")
                .into_response()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(f.token_id)
        .execute(&f.db.pool)
        .await
        .expect("revoke token after authentication");
    let result =
        super::control::control(State(f.state.clone()), Extension(principal), Json(request)).await;
    assert_eq!(
        result
            .err()
            .expect("current credential denied")
            .into_response()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert!(
        !f.store
            .policy(SupervisorArea::Workshop)
            .await
            .expect("still paused")
            .enabled
    );
    server.abort();
    readserver.abort();
    let _ = server.await;
    let _ = readserver.await;
    f.cleanup().await;
}
