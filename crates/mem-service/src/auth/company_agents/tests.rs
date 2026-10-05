// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::company_devices::tests as enrollment;
use crate::auth::device::tests::{Fixture, fixture};

struct Setup {
    f: Fixture,
    owner: enrollment::Owner,
    device: Uuid,
    scope: CompanyDeviceSelection,
    parent_secret: String,
}
async fn setup(destinations: Vec<OnboardingDestination>) -> Setup {
    // This suite never silently succeeds without its disposable PostgreSQL.
    let mut f = fixture()
        .await
        .expect("company agent tests require disposable PostgreSQL");
    let owner = enrollment::Owner::new_with_destinations(&f, destinations).await;
    enrollment::configure(&mut f, &owner);
    let (start, parent_secret, _) = enrollment::start(&f).await;
    let request = enrollment::request(&f, &start).await;
    let approved = enrollment::call(&f, &request).await.unwrap();
    Setup {
        f,
        owner,
        device: approved.device_id.unwrap(),
        scope: request.projects[0].clone(),
        parent_secret,
    }
}
fn destinations() -> Vec<OnboardingDestination> {
    vec![
        OnboardingDestination {
            name: "codex".into(),
            provider_id: "openai".into(),
            destination_id: "openai".into(),
        },
        OnboardingDestination {
            name: "claude".into(),
            provider_id: "anthropic".into(),
            destination_id: "anthropic".into(),
        },
    ]
}
fn request(s: &Setup, caller: CompanyAgentCaller) -> (CompanyAgentApproveRequest, String) {
    let secret = format!(
        "independent-agent-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let hash = crate::auth::hash_secret(&secret)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    (
        CompanyAgentApproveRequest {
            request_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            token_id: Uuid::new_v4(),
            caller,
            provider_id: caller.provider_id().into(),
            credential_sha256: hash,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            projects: vec![s.scope.clone()],
            confirmed: true,
        },
        secret,
    )
}
async fn call(s: &Setup, r: &CompanyAgentApproveRequest) -> Result<CompanyAgentReceipt, ApiError> {
    approve(
        State(s.f.state.clone()),
        Extension(s.f.human.clone()),
        Path(s.device),
        Json(r.clone()),
    )
    .await
    .map(|r| r.0)
}
async fn actor(s: &Setup, secret: &str) -> AuthenticatedPrincipal {
    crate::auth::resolve_principal(&s.f.state, crate::auth::Credential::ApiToken(secret.into()))
        .await
        .unwrap()
        .unwrap()
}
async fn close(s: Setup) {
    s.owner.close().await;
    s.f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn independent_siblings_have_no_generic_authority_and_only_exact_child_receipt() {
    let s = setup(destinations()).await;
    let (codex, secret) = request(&s, CompanyAgentCaller::Codex);
    let first = call(&s, &codex).await.unwrap();
    let (claude, other_secret) = request(&s, CompanyAgentCaller::Claude);
    let other = call(&s, &claude).await.unwrap();
    assert_eq!(json!(first), json!(call(&s, &codex).await.unwrap()));
    assert_eq!(first.projects[0].destination.provider_id, "openai");
    assert_eq!(other.projects[0].destination.provider_id, "anthropic");
    assert_ne!(first.principal_id, other.principal_id);
    assert_ne!(first.token_id, other.token_id);
    let child = actor(&s, &secret).await;
    let sibling = actor(&s, &other_secret).await;
    let parent = actor(&s, &s.parent_secret).await;
    assert!(child.global.is_empty());
    assert!(child.project_roles.is_empty());
    assert!(child.groups.is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth_project_memberships WHERE principal_id IN($1,$2)"
        )
        .bind(child.id)
        .bind(sibling.id)
        .fetch_one(&s.f.db.pool)
        .await
        .unwrap(),
        0
    );
    let Json(receipt) = child_receipt(
        State(s.f.state.clone()),
        Extension(child.clone()),
        Path(first.agent_id),
    )
    .await
    .unwrap();
    assert_eq!(json!(receipt), json!(first));
    assert!(
        child_receipt(
            State(s.f.state.clone()),
            Extension(child.clone()),
            Path(other.agent_id)
        )
        .await
        .is_err()
    );
    assert!(
        child_receipt(
            State(s.f.state.clone()),
            Extension(parent),
            Path(first.agent_id)
        )
        .await
        .is_err()
    );
    let app = crate::routes::build_http_app(s.f.state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    for path in [
        "/v1/auth/tokens",
        "/v1/auth/memberships",
        "/v1/admin/onboarding/users",
    ] {
        let response = client
            .get(format!("{base}{path}"))
            .bearer_auth(&secret)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = client
        .get(format!(
            "{base}/v1/onboarding/agents/{}/receipt",
            first.agent_id
        ))
        .bearer_auth(&secret)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let Json(list) = inspect(
        State(s.f.state.clone()),
        Extension(s.f.human.clone()),
        Path(s.device),
    )
    .await
    .unwrap();
    assert_eq!(list.agents.len(), 2);
    let Json(revoked) = revoke(
        State(s.f.state.clone()),
        Extension(s.f.human.clone()),
        Path((s.device, first.agent_id)),
    )
    .await
    .unwrap();
    assert!(revoked.revoked_at.is_some());
    let Json(repeated) = revoke(
        State(s.f.state.clone()),
        Extension(s.f.human.clone()),
        Path((s.device, first.agent_id)),
    )
    .await
    .unwrap();
    assert_eq!(json!(revoked), json!(repeated));
    assert!(call(&s, &codex).await.is_err());
    assert!(
        crate::auth::resolve_principal(&s.f.state, crate::auth::Credential::ApiToken(secret))
            .await
            .is_err()
    );
    assert!(
        child_receipt(
            State(s.f.state.clone()),
            Extension(sibling.clone()),
            Path(other.agent_id)
        )
        .await
        .is_ok()
    );
    for (mode, expected) in [
        (10, StatusCode::FORBIDDEN),
        (16, StatusCode::FORBIDDEN),
        (17, StatusCode::FORBIDDEN),
        (18, StatusCode::FORBIDDEN),
        (13, StatusCode::CONFLICT),
        (14, StatusCode::CONFLICT),
        (15, StatusCode::SERVICE_UNAVAILABLE),
    ] {
        s.owner.mode(mode);
        let error = child_receipt(
            State(s.f.state.clone()),
            Extension(sibling.clone()),
            Path(other.agent_id),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, expected, "direct mode {mode}");
        let response = client
            .get(format!(
                "{base}/v1/onboarding/agents/{}/receipt",
                other.agent_id
            ))
            .bearer_auth(&other_secret)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "actual HTTP mode {mode}");
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = response.text().await.unwrap();
        assert!(!body.contains(&other_secret));
        assert!(!body.contains("synthetic-delivery"));
    }
    s.owner.mode(0);
    sqlx::query("UPDATE onboarding_devices SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(s.device)
        .execute(&s.f.db.pool)
        .await
        .unwrap();
    assert!(
        crate::auth::resolve_principal(&s.f.state, crate::auth::Credential::ApiToken(other_secret))
            .await
            .is_err()
    );
    assert!(
        child_receipt(
            State(s.f.state.clone()),
            Extension(sibling),
            Path(other.agent_id)
        )
        .await
        .is_err()
    );
    server.abort();
    let _ = server.await;
    close(s).await;
}

#[tokio::test]
async fn changed_requests_offline_trust_destination_and_expiry_never_broaden_authority() {
    let s = setup(destinations()).await;
    let (valid, secret) = request(&s, CompanyAgentCaller::Codex);
    for field in [
        "provider",
        "destination",
        "trust",
        "offline",
        "revision",
        "digest",
        "expiry",
        "parent",
        "confirmation",
    ] {
        let mut r = valid.clone();
        match field {
            "provider" => r.provider_id = "anthropic".into(),
            "destination" => r.caller = CompanyAgentCaller::Claude,
            "trust" => r.projects[0].area = OnboardingArea::Trust,
            "offline" => r.projects[0].offline_confirmed = false,
            "revision" => r.projects[0].manifest_revision += 1,
            "digest" => r.projects[0].manifest_sha256 = "0".repeat(64),
            "expiry" => r.expires_at = Utc::now() + chrono::Duration::hours(25),
            "parent" => {
                r.credential_sha256 = crate::auth::hash_secret(&s.parent_secret)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            }
            _ => r.confirmed = false,
        };
        assert!(call(&s, &r).await.is_err(), "{field}");
    }
    call(&s, &valid).await.unwrap();
    let mut altered = valid.clone();
    altered.expires_at += chrono::Duration::minutes(1);
    assert_eq!(
        call(&s, &altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    altered = valid.clone();
    altered.credential_sha256 = "b".repeat(64);
    assert_eq!(
        call(&s, &altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    let mut registry = s.f.state.clone();
    registry
        .config
        .auth
        .company_onboarding
        .as_mut()
        .unwrap()
        .owners[0]
        .delivery_policy_revision = Some(2);
    assert!(
        approve(
            State(registry),
            Extension(s.f.human.clone()),
            Path(s.device),
            Json(valid.clone())
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
        .bind(s.f.human.id)
        .execute(&s.f.db.pool)
        .await
        .unwrap();
    assert!(call(&s, &valid).await.is_err());
    assert!(
        crate::auth::resolve_principal(&s.f.state, crate::auth::Credential::ApiToken(secret))
            .await
            .is_err()
    );
    close(s).await;
    let denied_scope = setup(vec![OnboardingDestination {
        name: "codex".into(),
        provider_id: "openai".into(),
        destination_id: "other-output".into(),
    }])
    .await;
    let (r, _) = request(&denied_scope, CompanyAgentCaller::Codex);
    assert!(call(&denied_scope, &r).await.is_err());
    close(denied_scope).await;
}

#[tokio::test]
async fn protected_audit_failure_rolls_back_provisioning_and_revocation() {
    let s = setup(destinations()).await;
    let (request, secret) = request(&s, CompanyAgentCaller::Codex);
    sqlx::query("CREATE FUNCTION fail_agent_audit() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type LIKE 'onboarding.company_agent.%' THEN RAISE EXCEPTION 'synthetic audit failure'; END IF; RETURN NEW; END $$").execute(&s.f.db.pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_agent_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION fail_agent_audit()").execute(&s.f.db.pool).await.unwrap();
    assert!(call(&s, &request).await.is_err());
    for (sql, id) in [
        (
            "SELECT count(*) FROM auth_principals WHERE id=$1",
            request.principal_id,
        ),
        (
            "SELECT count(*) FROM auth_service_tokens WHERE id=$1",
            request.token_id,
        ),
        (
            "SELECT count(*) FROM onboarding_company_agents WHERE agent_id=$1",
            request.agent_id,
        ),
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(sql)
                .bind(id)
                .fetch_one(&s.f.db.pool)
                .await
                .unwrap(),
            0
        );
    }
    sqlx::query("DROP TRIGGER fail_agent_audit ON auth_audit_events")
        .execute(&s.f.db.pool)
        .await
        .unwrap();
    let receipt = call(&s, &request).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_agent_audit BEFORE INSERT ON auth_audit_events FOR EACH ROW EXECUTE FUNCTION fail_agent_audit()").execute(&s.f.db.pool).await.unwrap();
    assert!(
        revoke(
            State(s.f.state.clone()),
            Extension(s.f.human.clone()),
            Path((s.device, receipt.agent_id))
        )
        .await
        .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM onboarding_company_agent_revocations WHERE agent_id=$1"
        )
        .bind(receipt.agent_id)
        .fetch_one(&s.f.db.pool)
        .await
        .unwrap(),
        0
    );
    let child = actor(&s, &secret).await;
    assert!(
        child_receipt(
            State(s.f.state.clone()),
            Extension(child),
            Path(receipt.agent_id)
        )
        .await
        .is_ok()
    );
    close(s).await;
}
