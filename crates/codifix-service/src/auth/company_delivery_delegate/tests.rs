// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::{Credential, resolve_principal};
use axum::http::StatusCode;

async fn fixture() -> Option<crate::auth::device::tests::Fixture> {
    let mut f = crate::auth::device::tests::fixture().await?;
    sqlx::query("UPDATE auth_principals SET global_permissions_json=$2 WHERE id=$1")
        .bind(f.human.id)
        .bind(json!(Permission::STANDARD))
        .execute(&f.db.pool)
        .await
        .unwrap();
    let area = f.state.config.area.as_ref().unwrap();
    f.state.config.auth.company_delivery_delegations =
        vec![codifix_config::CompanyDeliveryDelegationConfig {
            company_id: uuid::Uuid::new_v4(),
            owner_instance_id: area.instance_id,
            area: OnboardingArea::Workshop,
            issuer: f.state.config.auth.oidc.issuer_url.clone(),
            principal_id: uuid::Uuid::new_v4(),
            token_id: uuid::Uuid::new_v4(),
            policy_revision: 1,
            projects: vec![f.project],
        }];
    Some(f)
}
fn request(f: &crate::auth::device::tests::Fixture) -> CompanyDeliveryDelegateProvisionRequest {
    let c = &f.state.config.auth.company_delivery_delegations[0];
    CompanyDeliveryDelegateProvisionRequest {
        request_id: uuid::Uuid::new_v4(),
        company_id: c.company_id,
        owner_instance_id: c.owner_instance_id,
        area: c.area,
        issuer: c.issuer.clone(),
        principal_id: c.principal_id,
        token_id: c.token_id,
        delivery_policy_revision: c.policy_revision,
        name: "Synthetic delivery-only delegate".into(),
        projects: c.projects.clone(),
        expires_in_days: 30,
        confirmed: true,
    }
}
fn denied<T>(result: Result<T, ApiError>) -> StatusCode {
    result.err().expect("must reject").status
}

#[tokio::test]
async fn delivery_delegate_issues_once_and_generic_policy_never_accepts_custom_capability() {
    let Some(f) = fixture().await else { return };
    let request = request(&f);
    let (a, b) = tokio::join!(
        provision(&f.state, &f.human, &request),
        provision(&f.state, &f.human, &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.credential_issued, b.credential_issued);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.permissions, vec!["project_scoped_delivery"]);
    let raw = a.token.or(b.token).unwrap();
    let actor = resolve_principal(&f.state, Credential::ApiToken(raw.clone()))
        .await
        .unwrap()
        .unwrap();
    assert!(actor.global.is_empty());
    assert_eq!(
        actor.permissions_for_project("local").names(),
        vec!["project_scoped_delivery"]
    );
    assert!(actor.permissions_for_project("other").is_empty());
    for p in Permission::STANDARD {
        assert!(!actor.has_anywhere(p));
    }
    let replay = provision(&f.state, &f.human, &request).await.unwrap();
    assert!(replay.replayed && !replay.credential_issued && replay.token.is_none());
    let response = list(State(f.state.clone()), Extension(f.human.clone()))
        .await
        .unwrap()
        .into_response();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 32768)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["delegates"][0]["status"], "active");
    assert_eq!(
        value["delegates"][0]["permissions"],
        json!(["project_scoped_delivery"])
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(&raw));
    assert!(!String::from_utf8_lossy(&bytes).contains("token_hash"));
    assert!(!String::from_utf8_lossy(&bytes).contains("credential_commitment"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    for route in [
        "/v1/projects/local/memories",
        "/v1/auth/principals",
        "/v1/auth/tokens",
        "/v1/auth/memberships",
        "/v1/admin/onboarding/company/delegates",
        "/v1/admin/onboarding/company/delivery-delegates",
    ] {
        let response = client
            .get(format!("{origin}{route}"))
            .bearer_auth(&raw)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{route}");
    }
    for (route, body) in [
        (
            "/v1/auth/tokens",
            json!({"name":"must-not-mint","project":"local","role":"reader"}),
        ),
        (
            "/v1/auth/memberships",
            json!({"principal_id":f.human.id,"project":"local","role":"admin"}),
        ),
        (
            "/v1/projects/local/bundle/export",
            json!({"project":"local"}),
        ),
    ] {
        let response = client
            .post(format!("{origin}{route}"))
            .bearer_auth(&raw)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{route}");
    }
    server.abort();
    let _ = server.await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delivery_delegate_http_enforces_browser_csrf_and_one_time_receipt() {
    let Some(mut f) = fixture().await else { return };
    let request = request(&f);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    f.state.config.auth.public_base_url = Some(origin.clone());
    let (secret, csrf) = crate::auth::mint_web_session(&f.state, &f.db.pool, f.human.id)
        .await
        .unwrap();
    let cookie = format!("memory_session={secret}; memory_csrf={csrf}");
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let url = format!("{origin}/v1/admin/onboarding/company/delivery-delegates");
    let missing = client
        .post(&url)
        .header("cookie", &cookie)
        .header("origin", &origin)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    for first in [true, false] {
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
        let receipt: CompanyDeliveryDelegateProvisionResponse = response.json().await.unwrap();
        assert_eq!(receipt.credential_issued, first);
        assert_eq!(receipt.token.is_some(), first);
        assert_eq!(receipt.replayed, !first);
    }
    server.abort();
    let _ = server.await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delivery_delegate_replays_fail_closed_after_scope_identity_or_authority_changes() {
    let Some(mut f) = fixture().await else { return };
    let first = request(&f);
    provision(&f.state, &f.human, &first).await.unwrap();
    for modify in [
        (|r: &mut CompanyDeliveryDelegateProvisionRequest| r.delivery_policy_revision += 1)
            as fn(&mut CompanyDeliveryDelegateProvisionRequest),
        |r| r.projects.push(uuid::Uuid::new_v4()),
        |r| r.owner_instance_id = uuid::Uuid::new_v4(),
        |r| r.area = OnboardingArea::Homelab,
        |r| r.issuer.push('/'),
        |r| r.name.push_str("changed"),
        |r| r.expires_in_days = 1,
        |r| r.request_id = uuid::Uuid::new_v4(),
    ] {
        let mut changed = first.clone();
        modify(&mut changed);
        assert_eq!(
            denied(provision(&f.state, &f.human, &changed).await),
            StatusCode::CONFLICT
        );
    }
    for change in ["revoked", "disabled", "permission", "expired"] {
        let c = &mut f.state.config.auth.company_delivery_delegations[0];
        c.principal_id = uuid::Uuid::new_v4();
        c.token_id = uuid::Uuid::new_v4();
        let request = request(&f);
        provision(&f.state, &f.human, &request).await.unwrap();
        match change {
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
            "permission" => {
                sqlx::query(
                    "UPDATE auth_project_memberships SET permissions_json=$2 WHERE principal_id=$1",
                )
                .bind(request.principal_id)
                .bind(json!(Permission::STANDARD))
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
    }
    f.state.config.auth.company_delivery_delegations[0].owner_instance_id = uuid::Uuid::new_v4();
    assert!(f.state.config.validate_company_delivery_owner().is_err());
    assert_eq!(
        denied(provision(&f.state, &f.human, &first).await),
        StatusCode::FORBIDDEN
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delivery_delegate_confirmation_mode_and_protected_audit_are_transactional() {
    let Some(mut f) = fixture().await else { return };
    let request = request(&f);
    let mut unconfirmed = request.clone();
    unconfirmed.confirmed = false;
    assert_eq!(
        denied(provision(&f.state, &f.human, &unconfirmed).await),
        StatusCode::BAD_REQUEST
    );
    f.state.config.auth.mode = AuthMode::SingleUser;
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::FORBIDDEN
    );
    f.state.config.auth.mode = AuthMode::MultiUser;
    f.state.config.service.read_only = true;
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::FORBIDDEN
    );
    f.state.config.service.read_only = false;
    sqlx::query("CREATE FUNCTION synthetic_delivery_delegate_audit_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic audit unavailable'; END $$").execute(&f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER synthetic_delivery_delegate_audit_fail BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION synthetic_delivery_delegate_audit_fail()").execute(&f.db.pool).await.unwrap();
    assert_eq!(
        denied(provision(&f.state, &f.human, &request).await),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1) OR EXISTS(SELECT 1 FROM auth_service_tokens WHERE id=$2)").bind(request.principal_id).bind(request.token_id).fetch_one(&f.db.pool).await.unwrap();
    assert!(!exists);
    f.db.cleanup().await.unwrap();
}
