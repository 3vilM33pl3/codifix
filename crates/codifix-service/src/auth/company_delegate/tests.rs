// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::{Credential, resolve_principal};

async fn fixture() -> Option<crate::auth::device::tests::Fixture> {
    let mut f = crate::auth::device::tests::fixture().await?;
    sqlx::query("UPDATE auth_principals SET global_permissions_json=$2 WHERE id=$1")
        .bind(f.human.id)
        .bind(json!(Permission::ALL))
        .execute(&f.db.pool)
        .await
        .unwrap();
    configure(&mut f);
    Some(f)
}
fn configure(f: &mut crate::auth::device::tests::Fixture) {
    f.state.config.auth.company_delegations = vec![codifix_config::CompanyDelegationConfig {
        company_id: Uuid::new_v4(),
        principal_id: Uuid::new_v4(),
        token_id: Uuid::new_v4(),
        issuer: f.state.config.auth.oidc.issuer_url.clone(),
        policy_revision: 1,
        projects: vec![codifix_config::CompanyProjectDelegation {
            project_id: f.project,
            role_ceiling: AuthRole::Writer,
        }],
    }];
}
fn request(f: &crate::auth::device::tests::Fixture) -> CompanyDelegateProvisionRequest {
    let view = plan(&f.state, &f.state.config.auth.company_delegations[0]).unwrap();
    CompanyDelegateProvisionRequest {
        request_id: Uuid::new_v4(),
        company_id: view.company_id,
        owner_instance_id: view.owner_instance_id,
        area: view.area,
        issuer: view.issuer,
        principal_id: view.principal_id,
        token_id: view.token_id,
        delegation_policy_revision: view.delegation_policy_revision,
        name: "Synthetic restricted company delegate".into(),
        projects: view.projects,
        expires_in_days: 30,
        confirmed: true,
    }
}
fn denied<T>(result: Result<T, ApiError>) -> StatusCode {
    result.err().expect("must reject").status
}

#[tokio::test]
async fn config_bound_delegate_issues_once_without_global_or_export_authority() {
    let Some(f) = fixture().await else { return };
    let request = request(&f);
    let (a, b) = tokio::join!(
        provision(&f.state, &f.human, &request),
        provision(&f.state, &f.human, &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_ne!(a.credential_issued, b.credential_issued);
    let raw = a.token.or(b.token).unwrap();
    let hash: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM auth_service_tokens WHERE id=$1")
            .bind(request.token_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(hash, crate::auth::hash_secret(&raw));
    let metadata: Value =
        sqlx::query_scalar("SELECT metadata_json FROM auth_audit_events WHERE id=$1")
            .bind(request.request_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(!metadata.to_string().contains(&raw));
    assert_ne!(metadata["credential_commitment"], json!(hash));
    let principal = resolve_principal(&f.state, Credential::ApiToken(raw.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.id, request.principal_id);
    assert_eq!(principal.token_id, Some(request.token_id));
    assert!(principal.global.is_empty());
    assert_eq!(
        principal.permissions_for_project("local").names(),
        vec!["memory_read", "auth_manage"]
    );
    assert!(principal.permissions_for_project("other").is_empty());
    let replay = provision(&f.state, &f.human, &request).await.unwrap();
    assert!(replay.replayed);
    assert!(!replay.credential_issued);
    assert!(replay.token.is_none());
    let response = list(State(f.state.clone()), Extension(f.human.clone()))
        .await
        .unwrap()
        .into_response();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let view: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(view["delegates"][0]["status"], "active");
    assert!(!String::from_utf8_lossy(&bytes).contains(&raw));
    assert!(!String::from_utf8_lossy(&bytes).contains("token_hash"));
    assert!(!String::from_utf8_lossy(&bytes).contains("credential_commitment"));
    let grant = CompanyGrantRequest {
        company_id: request.company_id,
        operation_id: Uuid::new_v4(),
        owner_instance_id: request.owner_instance_id,
        area: request.area,
        project_id: f.project,
        issuer: request.issuer.clone(),
        subject: "before-first-login-exact-subject".into(),
        operation: CompanyGrantOperation::Grant,
        role: AuthRole::Writer,
        expected_grant_revision: 0,
        delegation_policy_revision: 1,
    };
    let _ = crate::auth::onboarding::company::delegated_grant(
        State(f.state.clone()),
        Extension(principal.clone()),
        Json(grant.clone()),
    )
    .await
    .unwrap();
    let mut beyond = grant;
    beyond.subject = "other-unregistered-subject".into();
    beyond.operation_id = Uuid::new_v4();
    beyond.role = AuthRole::Operator;
    assert_eq!(
        denied(
            crate::auth::onboarding::company::delegated_grant(
                State(f.state.clone()),
                Extension(principal),
                Json(beyond)
            )
            .await
        ),
        StatusCode::FORBIDDEN
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn changed_configuration_existing_rows_and_revocation_are_never_restored() {
    let Some(mut f) = fixture().await else { return };
    let first = request(&f);
    provision(&f.state, &f.human, &first).await.unwrap();
    for modify in [
        (|r: &mut CompanyDelegateProvisionRequest| r.name.push_str(" changed"))
            as fn(&mut CompanyDelegateProvisionRequest),
        |r| r.token_id = Uuid::new_v4(),
        |r| r.principal_id = Uuid::new_v4(),
        |r| r.owner_instance_id = Uuid::new_v4(),
        |r| r.delegation_policy_revision += 1,
        |r| r.projects[0].role_ceiling = AuthRole::Operator,
        |r| r.expires_in_days = 1,
        |r| r.issuer.push('/'),
    ] {
        let mut changed = first.clone();
        modify(&mut changed);
        assert_eq!(
            denied(provision(&f.state, &f.human, &changed).await),
            StatusCode::CONFLICT
        );
    }
    let mut changed = first.clone();
    changed.request_id = Uuid::new_v4();
    assert_eq!(
        denied(provision(&f.state, &f.human, &changed).await),
        StatusCode::CONFLICT
    );
    for state in ["revoked", "disabled", "permissions", "broadened", "expired"] {
        configure(&mut f);
        let request = request(&f);
        provision(&f.state, &f.human, &request).await.unwrap();
        match state {
            "revoked" => {
                sqlx::query("UPDATE auth_service_tokens SET revoked_at=now() WHERE id=$1")
                    .bind(request.token_id)
                    .execute(&f.db.pool)
                    .await
                    .unwrap();
            }
            "disabled" => {
                sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
                    .bind(request.principal_id)
                    .execute(&f.db.pool)
                    .await
                    .unwrap();
            }
            "permissions" => {
                sqlx::query("DELETE FROM auth_project_memberships WHERE principal_id=$1")
                    .bind(request.principal_id)
                    .execute(&f.db.pool)
                    .await
                    .unwrap();
            }
            "broadened" => {
                sqlx::query(
                    "UPDATE auth_project_memberships SET permissions_json=$2 WHERE principal_id=$1",
                )
                .bind(request.principal_id)
                .bind(json!(Permission::ALL))
                .execute(&f.db.pool)
                .await
                .unwrap();
            }
            _ => {
                sqlx::query("UPDATE auth_service_tokens SET expires_at=now()-interval '1 second' WHERE id=$1").bind(request.token_id).execute(&f.db.pool).await.unwrap();
            }
        }
        assert_eq!(
            denied(provision(&f.state, &f.human, &request).await),
            StatusCode::CONFLICT
        );
        let issued: i64 =
            sqlx::query_scalar("SELECT count(*) FROM auth_service_tokens WHERE principal_id=$1")
                .bind(request.principal_id)
                .fetch_one(&f.db.pool)
                .await
                .unwrap();
        assert_eq!(issued, 1);
    }
    configure(&mut f);
    let request = request(&f);
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,issuer,subject) VALUES($1,'human_oidc','Existing human',$2,'existing-human-subject')").bind(request.principal_id).bind(&request.issuer).execute(&f.db.pool).await.unwrap();
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::CONFLICT
    );
    let kind: String = sqlx::query_scalar("SELECT kind FROM auth_principals WHERE id=$1")
        .bind(request.principal_id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(kind, "human_oidc");
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delegate_actor_expired_during_lock_wait_cannot_issue_credentials() {
    let Some(f) = fixture().await else { return };
    let request = request(&f);
    let (secret, _) = crate::auth::mint_web_session(&f.state, &f.db.pool, f.human.id)
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
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:company-delegate-provision',0))",
    )
    .execute(&mut *blocker)
    .await
    .unwrap();
    let state = f.state.clone();
    let submitted = request.clone();
    let task = tokio::spawn(async move { provision(&state, &actor, &submitted).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))")
                .fetch_one(&f.db.pool).await.unwrap();
            if waiting { break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    sqlx::query("SELECT pg_sleep(1.1)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    blocker.commit().await.unwrap();
    assert_eq!(denied(task.await.unwrap()), StatusCode::FORBIDDEN);
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1) OR EXISTS(SELECT 1 FROM auth_service_tokens WHERE id=$2)")
        .bind(request.principal_id).bind(request.token_id)
        .fetch_one(&f.db.pool).await.unwrap();
    assert!(!exists);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn audit_failure_confirmation_and_invalid_policy_do_not_create_credentials() {
    let Some(mut f) = fixture().await else { return };
    let request = request(&f);
    let mut unconfirmed = request.clone();
    unconfirmed.confirmed = false;
    assert_eq!(
        denied(provision(&f.state, &f.human, &unconfirmed).await),
        StatusCode::BAD_REQUEST
    );
    f.state.config.auth.company_delegations[0].projects[0].role_ceiling = AuthRole::Admin;
    assert!(f.state.config.auth.validate_company_delegations().is_err());
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::FORBIDDEN
    );
    f.state.config.auth.company_delegations[0].projects[0].role_ceiling = AuthRole::Writer;
    f.state.config.service.read_only = true;
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::FORBIDDEN
    );
    f.state.config.service.read_only = false;
    f.state.config.area.as_mut().unwrap().name = "trust".into();
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::FORBIDDEN
    );
    f.state.config.area.as_mut().unwrap().name = "workshop".into();
    sqlx::query("CREATE FUNCTION synthetic_delegate_audit_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic protected audit unavailable'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER synthetic_delegate_audit_fail BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION synthetic_delegate_audit_fail()").execute(&f.db.pool).await.unwrap();
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let principal: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_principals WHERE id=$1")
        .bind(request.principal_id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(principal, 0);
    let token: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_service_tokens WHERE id=$1")
        .bind(request.token_id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(token, 0);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delegate_http_requires_global_administrator_csrf_and_no_store() {
    let Some(mut f) = fixture().await else { return };
    let request = request(&f);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    f.state.config.auth.public_base_url = Some(origin.clone());
    let (session, csrf) = crate::auth::mint_web_session(&f.state, &f.db.pool, f.human.id)
        .await
        .unwrap();
    let router = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let url = format!("{origin}/v1/admin/onboarding/company/delegates");
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .post(&url)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let cookie = format!("codifix_session={session}; codifix_csrf={csrf}");
    assert_eq!(
        client
            .post(&url)
            .header("cookie", &cookie)
            .header("origin", &origin)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .post(&url)
        .header("cookie", &cookie)
        .header("origin", &origin)
        .header("x-csrf-token", &csrf)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let issued: CompanyDelegateProvisionResponse = response.json().await.unwrap();
    let raw = issued.token.unwrap();
    assert_eq!(
        client
            .get(&url)
            .header("x-api-token", &raw)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = client
        .get(&url)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert!(!body.contains(&raw));
    server.abort();
    f.db.cleanup().await.unwrap();
}
