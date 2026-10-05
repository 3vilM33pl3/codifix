// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use axum::routing::{get, post};
use mem_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};

async fn state(db: &IsolatedDatabase) -> AppState {
    let config=serde_json::from_value(json!({
        "database":{"url":db.pool.connect_options().to_url_lossy().as_str()},
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-owner"},
        "auth":{"mode":"single_user","public_base_url":"http://127.0.0.1:4251",
            "oidc":{"issuer_url":"https://auth.example.test/application/o/memory"},
            "group_mappings":{"rules":[{"group":"reviewers","role":"reader","global":false,"project":"local"}]}},
        "offline":{"enabled":false},"cluster":{"enabled":false},"background_work":{"enabled":false},
        "automation":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},
        "consolidation":{"enabled":false},"reinforcement":{"enabled":false},
        "llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}
    })).unwrap();
    crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap()
}
async fn owner(state: &AppState) -> AuthenticatedPrincipal {
    super::super::resolve_principal(
        state,
        super::super::Credential::ApiToken("synthetic-owner".into()),
    )
    .await
    .unwrap()
    .unwrap()
}
fn identity(subject: &str) -> OnboardingUserRequest {
    OnboardingUserRequest {
        issuer: "https://auth.example.test/application/o/memory".into(),
        subject: subject.into(),
        display_name: "Synthetic human".into(),
        email: Some("human@example.test".into()),
    }
}
fn entry() -> OnboardingProjectEntry {
    OnboardingProjectEntry {
        area: OnboardingArea::Workshop,
        owner_instance_id: Uuid::new_v4(),
        repository_url: "https://github.com/example/project.git".into(),
        default_branch: "main".into(),
        offline_allowed: true,
        tools: vec![],
        destinations: vec![],
    }
}
async fn local_project(db: &IsolatedDatabase) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,'local','Local','/synthetic')",
    )
    .bind(id)
    .execute(&db.pool)
    .await
    .unwrap();
    id
}

#[test]
fn trust_and_unpinned_metadata_fail_closed() {
    let mut data = entry();
    assert!(validate_entry(&data).is_ok());
    data.area = OnboardingArea::Trust;
    assert!(validate_entry(&data).is_err());
    data.offline_allowed = false;
    assert!(validate_entry(&data).is_ok());
    data.repository_url = "https://password@github.com/example/project".into();
    assert!(validate_entry(&data).is_err());
    data.repository_url = "https://github.com/example/project".into();
    data.tools.push(OnboardingPinnedTool {
        name: "memory".into(),
        version: "latest".into(),
        url: "https://example.test/tool".into(),
        sha256: "a".repeat(64),
    });
    assert!(validate_entry(&data).is_err());
}

#[tokio::test]
async fn local_identity_effective_access_disable_and_oidc_cannot_reactivate() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let state = state(&db).await;
    let actor = owner(&state).await;
    let project = local_project(&db).await;
    let Json(created) = register_user(
        State(state.clone()),
        Extension(actor.clone()),
        Json(identity("stable-subject")),
    )
    .await
    .unwrap();
    let id = created.principal.id;
    let duplicate = register_user(
        State(state.clone()),
        Extension(actor.clone()),
        Json(identity("stable-subject")),
    )
    .await
    .unwrap_err();
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    let Json(other) = register_user(
        State(state.clone()),
        Extension(actor.clone()),
        Json(identity("different-subject-same-email")),
    )
    .await
    .unwrap();
    assert_ne!(id, other.principal.id);
    let membership = super::super::grant_membership(
        &db.pool,
        &actor,
        AuthMembershipGrantRequest {
            principal_id: id,
            project: "local".into(),
            role: AuthRole::Writer,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE auth_principals SET groups_json='[\"reviewers\"]' WHERE id=$1")
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    let Json(permissions) = access(State(state.clone()), Path(id)).await.unwrap();
    assert!(!permissions.disabled);
    assert_eq!(permissions.projects[0].grants.len(), 2);
    assert!(
        permissions.projects[0]
            .permissions
            .contains(&"activity_capture".into())
    );
    super::super::revoke_membership(&db.pool, &actor, membership.id)
        .await
        .unwrap();
    let Json(after_revoke) = access(State(state.clone()), Path(id)).await.unwrap();
    assert_eq!(after_revoke.projects[0].grants.len(), 1);
    assert_eq!(after_revoke.projects[0].grants[0].source, "oidc_group");
    assert!(
        after_revoke.projects[0]
            .permissions
            .contains(&"memory_read".into())
    );
    assert!(
        !after_revoke.projects[0]
            .permissions
            .contains(&"activity_capture".into())
    );
    let (session, _) = super::super::mint_web_session(&state, &db.pool, id)
        .await
        .unwrap();
    assert!(
        super::super::resolve_principal(
            &state,
            super::super::Credential::BrowserSession(session.clone())
        )
        .await
        .is_ok()
    );
    let token = super::super::create_service_token(
        &db.pool,
        &actor,
        AuthServiceTokenCreateRequest {
            name: "device".into(),
            project: "local".into(),
            role: AuthRole::Reader,
            expires_in_seconds: None,
        },
    )
    .await
    .unwrap();
    let device = Uuid::new_v4();
    sqlx::query("INSERT INTO onboarding_devices(id,principal_id,name,platform,service_token_id) VALUES($1,$2,'Mac','macos',$3)").bind(device).bind(id).bind(token.id).execute(&db.pool).await.unwrap();
    let Json(disabled) = disable_user(State(state.clone()), Extension(actor.clone()), Path(id))
        .await
        .unwrap();
    assert!(disabled.disabled_at.is_some());
    assert!(
        super::super::resolve_principal(&state, super::super::Credential::BrowserSession(session))
            .await
            .is_err()
    );
    assert!(
        super::super::resolve_principal(
            &state,
            super::super::Credential::ApiToken(token.token.unwrap())
        )
        .await
        .is_err()
    );
    let Json(permissions) = access(State(state.clone()), Path(id)).await.unwrap();
    assert!(permissions.disabled);
    assert!(permissions.projects[0].permissions.is_empty());
    let mut tx = db.pool.begin().await.unwrap();
    let result = super::super::oidc::upsert_human_principal_tx(
        &mut tx,
        &identity("").issuer,
        "stable-subject",
        "Changed",
        None,
        &["reviewers".into()],
    )
    .await;
    assert!(result.is_err());
    tx.rollback().await.unwrap();
    let disabled: bool =
        sqlx::query_scalar("SELECT disabled_at IS NOT NULL FROM auth_principals WHERE id=$1")
            .bind(id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(disabled);
    let grants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2",
    )
    .bind(id)
    .bind(project)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(grants, 0);
}

#[tokio::test]
async fn reviewed_catalog_cas_history_and_device_revoke() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let state = state(&db).await;
    let actor = owner(&state).await;
    let project = local_project(&db).await;
    let Json(first) = review_project(
        State(state.clone()),
        Extension(actor.clone()),
        Path(project),
        Json(OnboardingProjectReviewRequest {
            expected_revision: 0,
            entry: entry(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(first.revision, 1);
    let stale = review_project(
        State(state.clone()),
        Extension(actor.clone()),
        Path(project),
        Json(OnboardingProjectReviewRequest {
            expected_revision: 0,
            entry: entry(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.status, StatusCode::CONFLICT);
    let Json(second) = review_project(
        State(state.clone()),
        Extension(actor.clone()),
        Path(project),
        Json(OnboardingProjectReviewRequest {
            expected_revision: 1,
            entry: first.entry,
        }),
    )
    .await
    .unwrap();
    assert_eq!(second.revision, 2);
    assert!(
        sqlx::query("DELETE FROM onboarding_project_versions WHERE project_id=$1")
            .bind(project)
            .execute(&db.pool)
            .await
            .is_err()
    );
    let token = super::super::create_service_token(
        &db.pool,
        &actor,
        AuthServiceTokenCreateRequest {
            name: "device".into(),
            project: "local".into(),
            role: AuthRole::Reader,
            expires_in_seconds: None,
        },
    )
    .await
    .unwrap();
    let device = Uuid::new_v4();
    sqlx::query("INSERT INTO onboarding_devices(id,principal_id,name,platform,service_token_id) VALUES($1,$2,'Mac','macos',$3)").bind(device).bind(actor.id).bind(token.id).execute(&db.pool).await.unwrap();
    let Json(revoked) = revoke_device(State(state.clone()), Extension(actor), Path(device))
        .await
        .unwrap();
    assert!(revoked.revoked_at.is_some());
    assert!(
        super::super::resolve_principal(
            &state,
            super::super::Credential::ApiToken(token.token.unwrap())
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn fake_authentik_effects_are_bounded_sanitized_and_journaled() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let mut state = state(&db).await;
    let actor = owner(&state).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = observed.clone();
    let app=axum::Router::new()
        .route("/api/v3/core/users/",post(move|headers:axum::http::HeaderMap,Json(body):Json<Value>|{let seen=seen.clone();async move{assert_eq!(headers["authorization"],"Bearer synthetic-management-token");seen.lock().unwrap().push(body.clone());if body["username"]=="failure"{(StatusCode::BAD_REQUEST,Json(json!({"secret":"synthetic-do-not-leak"})))}else if body["username"]=="response-lost"{(StatusCode::CREATED,Json(json!({"effect_accepted_but_identifier_lost":true})))}else{(StatusCode::CREATED,Json(json!({"pk":42,"uid":"must-not-bind-oidc","token":"synthetic-do-not-leak"})))}}}))
        .route("/api/v3/core/users/{id}/",get(|Path(id):Path<i64>|async move{Json(if id==2 {json!({"pk":id,"path":"memory-onboarding"})}else if id==3{json!({"pk":id,"path":"memory-onboarding","is_superuser":"false"})}else{json!({"pk":id,"path":if id==42 {"memory-onboarding"} else {"unrelated-admins"},"is_superuser":id!=42})})}).patch(|Json(body):Json<Value>|async move{assert_eq!(body,json!({"is_active":false}));Json(json!({"pk":42}))}))
        .route("/api/v3/stages/invitation/invitations/",post(|Json(body):Json<Value>|async move{assert_eq!(body["single_use"],true);let name=body["name"].as_str().unwrap();assert!(name.len()<=50);assert!(name.bytes().all(|c|c.is_ascii_alphanumeric() || matches!(c,b'-'|b'_')));Json(json!({"pk":Uuid::new_v4()}))}));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let token_env = format!(
        "MEMORY_ONBOARDING_FAKE_AUTHENTIK_{}",
        Uuid::new_v4().simple()
    );
    // This fake provider test is self-contained and never reads a live token.
    unsafe { std::env::set_var(&token_env, "synthetic-management-token") };
    state.config.auth.authentik_management = mem_config::AuthentikManagementConfig {
        enabled: true,
        base_url: format!("http://{addr}"),
        token_env: token_env.clone(),
        invitation_flow: Some(Uuid::new_v4()),
        user_path: "memory-onboarding".into(),
    };
    let request = || OnboardingIdpUserRequest {
        username: "synthetic".into(),
        display_name: "Synthetic".into(),
        email: "human@example.test".into(),
    };
    let created = provider::operate(&state, &actor, "create", Some(request()), None)
        .await
        .unwrap();
    assert_eq!(created.outcome, "success");
    assert_eq!(created.provider_user_id, Some(42));
    assert_eq!(
        observed.lock().unwrap()[0]["attributes"]["memory_onboarding_operation"],
        created.id.to_string()
    );
    assert_eq!(created.provider_correlation, Some(created.id.to_string()));
    assert_eq!(created.request_sha256.as_ref().unwrap().len(), 64);
    let invite = provider::operate(&state, &actor, "invite", Some(request()), None)
        .await
        .unwrap();
    assert_eq!(invite.outcome, "success");
    assert!(invite.invitation_id.is_some());
    assert_eq!(
        invite.provider_correlation,
        Some(format!("memory-onboarding-{}", invite.id.simple()))
    );
    let Json(saved) = operation(State(state.clone()), Path(invite.id))
        .await
        .unwrap();
    assert_eq!(saved.invitation_id, invite.invitation_id);
    let disabled = provider::operate(&state, &actor, "disable", None, Some(42))
        .await
        .unwrap();
    assert_eq!(disabled.outcome, "success");
    let denied = provider::operate(&state, &actor, "disable", None, Some(1))
        .await
        .unwrap_err();
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    for invalid in [2, 3] {
        assert_eq!(
            provider::operate(&state, &actor, "disable", None, Some(invalid))
                .await
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
    }
    let mut fail = request();
    fail.username = "failure".into();
    let failed = provider::operate(&state, &actor, "create", Some(fail), None)
        .await
        .unwrap();
    assert_eq!(failed.outcome, "error");
    assert!(
        !serde_json::to_string(&failed)
            .unwrap()
            .contains("synthetic-do-not-leak")
    );
    assert_eq!(observed.lock().unwrap()[0]["groups"], json!([]));
    let audit:String=sqlx::query_scalar("SELECT COALESCE(string_agg(metadata_json::text,' '),'') FROM auth_audit_events WHERE event_type LIKE 'onboarding.idp.%'").fetch_one(&db.pool).await.unwrap();
    assert!(!audit.contains("synthetic-do-not-leak"));
    assert!(!audit.contains("human@example.test"));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_idp_outcomes")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
    let mut lost = request();
    lost.username = "response-lost".into();
    let accepted = provider::operate(&state, &actor, "create", Some(lost), None)
        .await
        .unwrap();
    assert_eq!(accepted.outcome, "uncertain");
    let Json(saved) = operation(State(state.clone()), Path(accepted.id))
        .await
        .unwrap();
    assert_eq!(saved.provider_correlation, Some(accepted.id.to_string()));
    assert_eq!(saved.request_sha256, accepted.request_sha256);
    sqlx::raw_sql("CREATE FUNCTION reject_test_idp_outcome() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic SQL failure'; END $$; CREATE TRIGGER reject_test_idp_outcome BEFORE INSERT ON onboarding_idp_outcomes FOR EACH ROW EXECUTE FUNCTION reject_test_idp_outcome();")
        .execute(&db.pool).await.unwrap();
    let uncertain = provider::operate(&state, &actor, "create", Some(request()), None)
        .await
        .unwrap_err();
    assert!(
        uncertain
            .message
            .contains("external effect may have occurred")
    );
    assert!(!uncertain.message.contains("synthetic SQL failure"));
    let Json(records) = operations(State(state.clone())).await.unwrap();
    let pending = records["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["outcome"] == "pending")
        .unwrap();
    assert_eq!(
        pending["provider_correlation"],
        observed.lock().unwrap().last().unwrap()["attributes"]["memory_onboarding_operation"]
    );
    assert!(uncertain.message.contains(pending["id"].as_str().unwrap()));
    assert!(
        records["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["outcome"] == "pending")
    );
    assert!(
        sqlx::query("DELETE FROM onboarding_idp_intents")
            .execute(&db.pool)
            .await
            .is_err()
    );
    server.abort();
    unsafe { std::env::remove_var(token_env) };
}

#[tokio::test]
async fn http_admin_policy_csrf_and_read_only_are_shared() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let state = state(&db).await;
    let actor = owner(&state).await;
    let (session, csrf) = super::super::mint_web_session(&state, &db.pool, actor.id)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::routes::build_http_app(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::new();
    let route = format!("{base}/v1/admin/onboarding/users");
    assert_eq!(
        client.get(&route).send().await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let inspected = client
        .get(&route)
        .bearer_auth("synthetic-owner")
        .send()
        .await
        .unwrap();
    assert_eq!(inspected.status(), StatusCode::OK);
    assert_eq!(inspected.headers()["cache-control"], "no-store");
    let cookie = format!("memory_session={session}; memory_csrf={csrf}");
    assert_eq!(
        client
            .post(&route)
            .header("cookie", &cookie)
            .json(&identity("csrf-rejected"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(&route)
            .header("cookie", &cookie)
            .header("origin", "http://127.0.0.1:4251")
            .header("x-csrf-token", &csrf)
            .json(&identity("csrf-accepted"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let mut read_only = state.clone();
    read_only.config.service.read_only = true;
    assert_eq!(
        register_user(
            State(read_only),
            Extension(actor),
            Json(identity("read-only-rejected"))
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM auth_principals WHERE subject IN ('csrf-rejected','read-only-rejected')").fetch_one(&db.pool).await.unwrap();
    assert_eq!(count, 0);
    server.abort();
}

#[tokio::test]
async fn local_audit_failure_rolls_back_user_and_catalog() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let state = state(&db).await;
    let actor = owner(&state).await;
    let project = local_project(&db).await;
    sqlx::raw_sql("CREATE FUNCTION reject_test_onboarding_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type LIKE 'onboarding.%' THEN RAISE EXCEPTION 'synthetic audit failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_test_onboarding_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION reject_test_onboarding_audit();")
        .execute(&db.pool).await.unwrap();
    assert!(
        register_user(
            State(state.clone()),
            Extension(actor.clone()),
            Json(identity("rolled-back"))
        )
        .await
        .is_err()
    );
    assert!(
        review_project(
            State(state),
            Extension(actor),
            Path(project),
            Json(OnboardingProjectReviewRequest {
                expected_revision: 0,
                entry: entry()
            })
        )
        .await
        .is_err()
    );
    let humans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM auth_principals WHERE subject='rolled-back'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(humans, 0);
    let versions: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_project_versions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(versions, 0);
}
