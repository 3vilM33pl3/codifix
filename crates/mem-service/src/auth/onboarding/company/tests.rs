// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;
use std::sync::{Arc, Mutex};

struct Fixture {
    db: IsolatedDatabase,
    state: AppState,
    actor: AuthenticatedPrincipal,
    admin: AuthenticatedPrincipal,
    request: CompanyGrantRequest,
    secret: String,
}
async fn fixture() -> Option<Fixture> {
    let db = IsolatedDatabase::new().await?;
    let company = Uuid::new_v4();
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    let delegate = Uuid::new_v4();
    let token = Uuid::new_v4();
    let admin = Uuid::new_v4();
    let secret = format!("synthetic-company-{}", Uuid::new_v4());
    let issuer = "https://auth.example.test/application/o/shared";
    let config=serde_json::from_value(json!({"database":{"url":db.pool.connect_options().to_url_lossy().as_str()},"area":{"name":"workshop","instance_id":instance},
        "service":{"bind_addr":"127.0.0.1:0","api_token":"synthetic-owner"},"auth":{"mode":"single_user","public_base_url":"https://company.example.test","oidc":{"issuer_url":issuer},
        "group_mappings":{"rules":[{"group":"readers","role":"reader","global":false,"project":"local"}]},
        "company_delegations":[{"company_id":company,"issuer":issuer,"principal_id":delegate,"token_id":token,"policy_revision":1,"projects":[{"project_id":project,"role_ceiling":"operator"}]}]},
        "offline":{"enabled":false},"cluster":{"enabled":false},"background_work":{"enabled":false},"automation":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"consolidation":{"enabled":false},"reinforcement":{"enabled":false},
        "llm":{"provider":"ollama","model":"disabled","base_url":"http://127.0.0.1:1"}})).unwrap();
    let mut state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,'local','Local','/synthetic')",
    )
    .bind(project)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,'service_token','Delegate','[]'::jsonb)").bind(delegate).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'Delegate',$3,'synthetic')").bind(token).bind(delegate).bind(crate::auth::hash_secret(&secret)).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,permissions_json) VALUES($1,$2,$3,'reader','[\"memory_read\",\"auth_manage\"]'::jsonb)").bind(Uuid::new_v4()).bind(delegate).bind(project).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_principals(id,kind,issuer,subject,display_name,global_permissions_json) VALUES($1,'human_oidc',$2,'Admin','Admin',$3)").bind(admin).bind(issuer).bind(json!(Permission::ALL)).execute(&db.pool).await.unwrap();
    let (session, _) = crate::auth::mint_web_session(&state, &db.pool, admin)
        .await
        .unwrap();
    state.config.auth.mode = AuthMode::MultiUser;
    let actor =
        crate::auth::resolve_principal(&state, crate::auth::Credential::ApiToken(secret.clone()))
            .await
            .unwrap()
            .unwrap();
    let admin =
        crate::auth::resolve_principal(&state, crate::auth::Credential::BrowserSession(session))
            .await
            .unwrap()
            .unwrap();
    let request = CompanyGrantRequest {
        company_id: company,
        operation_id: Uuid::new_v4(),
        owner_instance_id: instance,
        area: OnboardingArea::Workshop,
        project_id: project,
        issuer: issuer.into(),
        subject: "Before-first-login".into(),
        operation: CompanyGrantOperation::Grant,
        role: AuthRole::Writer,
        expected_grant_revision: 0,
        delegation_policy_revision: 1,
    };
    Some(Fixture {
        db,
        state,
        actor,
        admin,
        request,
        secret,
    })
}
async fn apply(
    f: &Fixture,
    request: CompanyGrantRequest,
) -> Result<Json<CompanyGrantResult>, ApiError> {
    delegated_grant(
        State(f.state.clone()),
        Extension(f.actor.clone()),
        Json(request),
    )
    .await
}

#[tokio::test]
async fn prelogin_owned_grants_replay_revoke_and_groups_are_distinct() {
    let Some(f) = fixture().await else { return };
    let Json(granted) = apply(&f, f.request.clone()).await.unwrap();
    assert_eq!(granted.receipt.grant_revision, 1);
    assert_eq!(granted.current_access.direct_role, Some(AuthRole::Writer));
    let human = granted.receipt.principal_id;
    sqlx::query("UPDATE auth_principals SET groups_json='[\"readers\"]' WHERE id=$1")
        .bind(human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let member: Uuid =
        sqlx::query_scalar("SELECT id FROM auth_project_memberships WHERE principal_id=$1")
            .bind(human)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(
        crate::auth::revoke_membership(&f.db.pool, &f.admin, member)
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    let mut revoke = f.request.clone();
    revoke.operation_id = Uuid::new_v4();
    revoke.expected_grant_revision = 1;
    revoke.operation = CompanyGrantOperation::Revoke;
    let Json(revoked) = apply(&f, revoke.clone()).await.unwrap();
    assert_eq!(revoked.receipt.grant_revision, 2);
    assert!(!revoked.receipt.direct_grant_active);
    assert_eq!(revoked.current_access.direct_role, None);
    assert_eq!(revoked.current_access.group_roles, vec![AuthRole::Reader]);
    assert!(
        revoked
            .current_access
            .permissions
            .contains(&"memory_read".into())
    );
    assert!(
        !revoked
            .current_access
            .permissions
            .contains(&"activity_capture".into())
    );
    let Json(replay) = apply(&f, f.request.clone()).await.unwrap();
    assert_eq!(replay.receipt, granted.receipt);
    assert_eq!(replay.current_access.direct_role, None);
    assert_eq!(
        crate::auth::grant_membership(
            &f.db.pool,
            &f.admin,
            AuthMembershipGrantRequest {
                principal_id: human,
                project: "local".into(),
                role: AuthRole::Reader
            }
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    let mut altered = f.request.clone();
    altered.role = AuthRole::Reader;
    assert_eq!(
        apply(&f, altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(human)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let Json(replay) = apply(&f, revoke).await.unwrap();
    assert!(replay.current_access.disabled);
    assert!(replay.current_access.permissions.is_empty());
    let mut fresh = f.request.clone();
    fresh.operation_id = Uuid::new_v4();
    fresh.expected_grant_revision = 2;
    assert_eq!(
        apply(&f, fresh).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn exact_namespace_token_project_ceiling_and_local_ownership_fail_closed() {
    let Some(mut f) = fixture().await else { return };
    let mut alias = f.request.clone();
    alias.issuer.push('/');
    assert_eq!(
        apply(&f, alias).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    let mut elevated = f.request.clone();
    elevated.role = AuthRole::Admin;
    assert_eq!(
        apply(&f, elevated).await.unwrap_err().status,
        StatusCode::BAD_REQUEST
    );
    f.state.config.auth.company_delegations[0].projects[0].role_ceiling = AuthRole::Reader;
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.state.config.auth.company_delegations[0].projects[0].role_ceiling = AuthRole::Operator;
    let saved = f.actor.token_id;
    f.actor.token_id = Some(Uuid::new_v4());
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.actor.token_id = saved;
    sqlx::query("UPDATE auth_principals SET global_permissions_json=$2 WHERE id=$1")
        .bind(f.actor.id)
        .bind(json!(Permission::ALL))
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_project_memberships SET permissions_json='[\"memory_read\"]'::jsonb WHERE principal_id=$1").bind(f.actor.id).execute(&f.db.pool).await.unwrap();
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_project_memberships SET permissions_json='[\"memory_read\",\"auth_manage\"]'::jsonb WHERE principal_id=$1").bind(f.actor.id).execute(&f.db.pool).await.unwrap();
    let human = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,issuer,subject,display_name) VALUES($1,'human_oidc',$2,$3,'Local human')").bind(human).bind(&f.request.issuer).bind(&f.request.subject).execute(&f.db.pool).await.unwrap();
    crate::auth::grant_membership(
        &f.db.pool,
        &f.admin,
        AuthMembershipGrantRequest {
            principal_id: human,
            project: "local".into(),
            role: AuthRole::Reader,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    let mut trust = f.request.clone();
    trust.area = OnboardingArea::Trust;
    assert_eq!(
        apply(&f, trust).await.unwrap_err().status,
        StatusCode::BAD_REQUEST
    );
    f.state.config.service.read_only = true;
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.state.config.service.read_only = false;
    f.state.config.auth.oidc.issuer_url.push('/');
    f.state.config.auth.company_delegations[0].issuer.push('/');
    let mut noncanonical = f.request.clone();
    noncanonical.issuer.push('/');
    assert_eq!(
        apply(&f, noncanonical).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn concurrent_absent_grants_and_failed_audit_are_atomic() {
    let Some(f) = fixture().await else { return };
    let (a, b) = tokio::join!(apply(&f, f.request.clone()), apply(&f, f.request.clone()));
    assert_eq!(a.unwrap().0.receipt, b.unwrap().0.receipt);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM onboarding_company_receipts")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let mut first = f.request.clone();
    first.operation_id = Uuid::new_v4();
    first.subject = "concurrent-new-subject".into();
    let mut second = first.clone();
    second.operation_id = Uuid::new_v4();
    let (a, b) = tokio::join!(apply(&f, first), apply(&f, second));
    assert!(a.is_ok() != b.is_ok());
    assert_eq!(
        a.err().or_else(|| b.err()).unwrap().status,
        StatusCode::CONFLICT
    );
    sqlx::query("CREATE FUNCTION deny_company_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic audit failure'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER deny_company_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION deny_company_audit()").execute(&f.db.pool).await.unwrap();
    let mut fresh = f.request.clone();
    fresh.operation_id = Uuid::new_v4();
    fresh.subject = "audit-rollback".into();
    assert!(apply(&f, fresh.clone()).await.is_err());
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE subject=$1)")
            .bind(&fresh.subject)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(!exists);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn coordinator_intent_survives_lost_reply_and_reconciles_current_rights() {
    let Some(mut f) = fixture().await else { return };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let state = f.state.clone();
    let actor = f.actor.clone();
    let secret = f.secret.clone();
    let state2 = state.clone();
    let actor2 = actor.clone();
    let app=axum::Router::new().route("/v1/onboarding/delegated-grants",axum::routing::post(move|headers:axum::http::HeaderMap,Json(request):Json<CompanyGrantRequest>|{let state=state.clone();let actor=actor.clone();let secret=secret.clone();async move{
        assert_eq!(headers["authorization"],format!("Bearer {secret}"));
        // Prove no coordinator credential locks span this HTTP call.
        let mut probe=state.pool().unwrap().begin().await.unwrap();sqlx::query("SELECT id FROM auth_web_sessions FOR UPDATE NOWAIT").execute(&mut *probe).await.unwrap();probe.rollback().await.unwrap();
        let result=delegated_grant(State(state),Extension(actor),Json(request)).await.unwrap();
        assert_eq!(result.0.receipt.grant_revision,1);
        Json(json!({"accepted_but_response_lost":"synthetic-secret-never-return"}))
    }}))
    .route("/v1/onboarding/delegated-grants/operations/{operation_id}",axum::routing::get(move|Path(id):Path<Uuid>,Query(q):Query<CompanyQuery>|{let state=state2.clone();let actor=actor2.clone();async move{delegated_status(State(state),Extension(actor),Path(id),Query(q)).await}}));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let env = format!("MEMORY_COMPANY_FAKE_{}", Uuid::new_v4().simple());
    unsafe { std::env::set_var(&env, &f.secret) };
    f.state.config.auth.company_onboarding = Some(mem_config::CompanyOnboardingConfig {
        company_id: f.request.company_id,
        owners: vec![mem_config::CompanyOwnerConfig {
            owner_instance_id: f.request.owner_instance_id,
            area: f.request.area,
            base_url: format!("http://{addr}"),
            token_env: env.clone(),
            issuer: f.request.issuer.clone(),
            delegation_policy_revision: 1,
        }],
    });
    let Json(uncertain) = company_operation(
        State(f.state.clone()),
        Extension(f.admin.clone()),
        Json(f.request.clone()),
    )
    .await
    .unwrap();
    assert_eq!(uncertain.outcome, "uncertain");
    assert!(uncertain.receipt.is_none());
    let Json(confirmed) = company_reconcile(
        State(f.state.clone()),
        Extension(f.admin.clone()),
        Path(f.request.operation_id),
    )
    .await
    .unwrap();
    assert_eq!(confirmed.outcome, "confirmed");
    assert!(confirmed.current_access.is_some());
    let Json(history) = company_status(State(f.state.clone()), Path(f.request.operation_id))
        .await
        .unwrap();
    assert!(history.receipt.is_some());
    assert!(history.current_access.is_none());
    let mut altered = f.request.clone();
    altered.role = AuthRole::Reader;
    assert_eq!(
        company_operation(
            State(f.state.clone()),
            Extension(f.admin.clone()),
            Json(altered)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::CONFLICT
    );
    let journal: Value =
        sqlx::query_scalar("SELECT to_jsonb(o) FROM onboarding_company_outcomes o")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(
        !journal
            .to_string()
            .contains("synthetic-secret-never-return")
    );
    assert!(!journal.to_string().contains(&f.secret));
    sqlx::query("CREATE FUNCTION deny_company_outcome() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic outcome failure'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER deny_company_outcome BEFORE INSERT OR UPDATE ON onboarding_company_outcomes FOR EACH ROW EXECUTE FUNCTION deny_company_outcome()").execute(&f.db.pool).await.unwrap();
    let mut failure = f.request.clone();
    failure.operation_id = Uuid::new_v4();
    failure.subject = "outcome-write-failed".into();
    let failed = company_operation(
        State(f.state.clone()),
        Extension(f.admin.clone()),
        Json(failure.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(failed.message.contains(&failure.operation_id.to_string()));
    let durable: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM onboarding_company_intents WHERE operation_id=$1)",
    )
    .bind(failure.operation_id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert!(durable);
    sqlx::query("DROP TRIGGER deny_company_outcome ON onboarding_company_outcomes")
        .execute(&f.db.pool)
        .await
        .unwrap();
    let Json(recovered) = company_reconcile(
        State(f.state.clone()),
        Extension(f.admin.clone()),
        Path(failure.operation_id),
    )
    .await
    .unwrap();
    assert_eq!(recovered.outcome, "confirmed");
    server.abort();
    unsafe { std::env::remove_var(env) };
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn http_delegate_requires_current_enrolled_token_not_global_browser() {
    let Some(f) = fixture().await else { return };
    let app = crate::routes::build_http_app(f.state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let request = || {
        client
            .post(format!("http://{address}/v1/onboarding/delegated-grants"))
            .bearer_auth(&f.secret)
            .json(&f.request)
            .send()
    };
    let response = request().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        delegated_grant(
            State(f.state.clone()),
            Extension(f.admin.clone()),
            Json(f.request.clone())
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=now() WHERE id=$1")
        .bind(f.actor.token_id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(request().await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        apply(&f, f.request.clone()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    server.abort();
    f.db.cleanup().await.unwrap();
}
