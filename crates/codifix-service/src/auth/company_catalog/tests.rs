// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use axum::{Json, Router, http::HeaderMap, routing::post};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Mode {
    Valid,
    WrongHuman,
    WrongOwner,
    WrongDigest,
    WrongRoles,
    Duplicate,
    Unavailable,
    Oversized,
    DisableHuman,
    RevokeSession,
    ChangeIdentity,
}
#[derive(Clone)]
struct Fake {
    mode: Mode,
    project: OnboardingProject,
    seen: Arc<Mutex<Vec<(OwnerDeviceCatalogRequest, HeaderMap)>>>,
    pool: PgPool,
    human: Uuid,
    session: Uuid,
}
async fn handle(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(request): Json<OwnerDeviceCatalogRequest>,
) -> Response {
    fake.seen.lock().unwrap().push((request.clone(), headers));
    if matches!(fake.mode, Mode::Unavailable) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "SENTINEL-private-owner-body",
        )
            .into_response();
    }
    if matches!(fake.mode, Mode::Oversized) {
        return "SENTINEL".repeat(OWNER_BYTES / 8 + 1).into_response();
    }
    match fake.mode {
        Mode::DisableHuman => {
            sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
                .bind(fake.human)
                .execute(&fake.pool)
                .await
                .unwrap();
        }
        Mode::RevokeSession => {
            sqlx::query("UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1")
                .bind(fake.session)
                .execute(&fake.pool)
                .await
                .unwrap();
        }
        Mode::ChangeIdentity => {
            sqlx::query(
                "UPDATE auth_principals SET subject='different-current-subject' WHERE id=$1",
            )
            .bind(fake.human)
            .execute(&fake.pool)
            .await
            .unwrap();
        }
        _ => {}
    }
    let mut bound = request.clone();
    if matches!(fake.mode, Mode::WrongHuman) {
        bound.subject = "other-human".into();
    }
    let mut manifest = fake.project.clone();
    manifest.entry.area = request.area;
    manifest.entry.owner_instance_id = request.owner_instance_id;
    if matches!(fake.mode, Mode::WrongOwner) {
        manifest.entry.owner_instance_id = Uuid::new_v4();
    }
    let mut item = OwnerDeviceCatalogProject {
        curated_contributions_available: false,
        manifest_sha256: digest(&manifest).unwrap(),
        project: OnboardingDeviceReviewProject {
            curated_contributions_allowed: false,
            manifest,
            allowed_roles: vec![AuthRole::Reader],
            offline_available: true,
        },
    };
    if matches!(fake.mode, Mode::WrongDigest) {
        item.manifest_sha256 = "0".repeat(64);
    }
    if matches!(fake.mode, Mode::WrongRoles) {
        item.project.allowed_roles = vec![AuthRole::Reader, AuthRole::Writer];
    }
    let mut projects = vec![item.clone()];
    if matches!(fake.mode, Mode::Duplicate) {
        projects.push(item);
    }
    Json(OwnerDeviceCatalogResponse {
        request_sha256: digest(&bound).unwrap(),
        projects,
        observed_at: Utc::now(),
    })
    .into_response()
}
struct OwnerFixture {
    server: tokio::task::JoinHandle<()>,
    env: String,
    secret: String,
    seen: Arc<Mutex<Vec<(OwnerDeviceCatalogRequest, HeaderMap)>>>,
    config: codifix_config::CompanyOwnerConfig,
}
impl OwnerFixture {
    async fn new(
        f: &crate::auth::device::tests::Fixture,
        area: OnboardingArea,
        mode: Mode,
    ) -> Self {
        let row=sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY revision DESC LIMIT 1").bind(f.project).fetch_one(&f.db.pool).await.unwrap();
        let project = crate::auth::onboarding::project_row(&row).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let fake = Fake {
            mode,
            project,
            seen: seen.clone(),
            pool: f.db.pool.clone(),
            human: f.human.id,
            session: f.human.session_id.unwrap(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/v1/onboarding/company-delivery/catalog", post(handle))
            .with_state(fake);
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let env = format!("MEMORY_CATALOG_FAKE_{}", Uuid::new_v4().simple());
        let secret = format!("synthetic-delivery-only-{}", Uuid::new_v4());
        // Unique per synthetic test; no live environment reference is used.
        unsafe { std::env::set_var(&env, &secret) };
        let config = codifix_config::CompanyOwnerConfig {
            owner_instance_id: Uuid::new_v4(),
            area,
            base_url: format!("http://{addr}"),
            token_env: "MUST_NOT_USE_MANAGEMENT_CREDENTIAL".into(),
            issuer: f.state.config.auth.oidc.issuer_url.clone(),
            delegation_policy_revision: 1,
            delivery_token_env: Some(env.clone()),
            delivery_policy_revision: Some(1),
        };
        Self {
            server,
            env,
            secret,
            seen,
            config,
        }
    }
    async fn close(self) {
        self.server.abort();
        let _ = self.server.await;
        unsafe { std::env::remove_var(&self.env) };
    }
}
fn configure(f: &mut crate::auth::device::tests::Fixture, owners: &[&OwnerFixture]) -> Uuid {
    let company = Uuid::new_v4();
    f.state.config.auth.company_onboarding = Some(codifix_config::CompanyOnboardingConfig {
        company_id: company,
        owners: owners.iter().map(|o| o.config.clone()).collect(),
    });
    company
}
fn denied<T>(result: Result<T, ApiError>) -> ApiError {
    result.err().expect("must reject")
}

#[tokio::test]
async fn authorized_catalog_uses_fresh_human_and_delivery_only_transport_retains_composite_scopes()
{
    let Some(mut f) = crate::auth::device::tests::fixture().await else {
        return;
    };
    let one = OwnerFixture::new(&f, OnboardingArea::Workshop, Mode::Valid).await;
    let two = OwnerFixture::new(&f, OnboardingArea::Homelab, Mode::Valid).await;
    let company = configure(&mut f, &[&one, &two]);
    f.state
        .config
        .auth
        .company_onboarding
        .as_mut()
        .unwrap()
        .owners
        .push(codifix_config::CompanyOwnerConfig {
            owner_instance_id: Uuid::new_v4(),
            area: OnboardingArea::Trust,
            base_url: "https://must-not-contact-trust.example".into(),
            token_env: "MUST_NOT_USE_TRUST_MANAGEMENT".into(),
            issuer: f.state.config.auth.oidc.issuer_url.clone(),
            delegation_policy_revision: 1,
            delivery_token_env: None,
            delivery_policy_revision: None,
        });
    let mut actor = f.human.clone();
    f.state.config.auth.oidc.issuer_url.push('/');
    actor.subject = Some("stale-or-forged-cached-subject".into());
    actor.issuer = Some("https://cached-wrong.example".into());
    let response = catalog(State(f.state.clone()), Extension(actor))
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), AGGREGATE_BYTES)
        .await
        .unwrap();
    let value: CompanyDeviceCatalogResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value.company_id, company);
    assert_eq!(value.projects.len(), 2);
    assert_eq!(
        value.projects[0].project.manifest.project_id,
        value.projects[1].project.manifest.project_id
    );
    assert_eq!(
        value.projects[0].project.manifest.project,
        value.projects[1].project.manifest.project
    );
    assert_ne!(
        value.projects[0].project.manifest.entry.owner_instance_id,
        value.projects[1].project.manifest.entry.owner_instance_id
    );
    for owner in [&one, &two] {
        let seen = owner.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let (request, headers) = &seen[0];
        assert_eq!(request.company_id, company);
        assert_eq!(request.subject, "Human");
        assert_eq!(
            request.issuer,
            codifix_config::oidc_principal_namespace(&f.state.config.auth.oidc.issuer_url)
        );
        assert_eq!(headers["authorization"], format!("Bearer {}", owner.secret));
        for absent in [
            "cookie",
            "x-api-token",
            "x-csrf-token",
            "forwarded",
            "x-forwarded-for",
        ] {
            assert!(!headers.contains_key(absent));
        }
        assert!(!String::from_utf8_lossy(&bytes).contains(&owner.secret));
        assert!(!String::from_utf8_lossy(&bytes).contains(&owner.env));
    }
    one.close().await;
    two.close().await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn unverified_owner_responses_never_return_partial_catalog_or_provider_body() {
    let Some(mut f) = crate::auth::device::tests::fixture().await else {
        return;
    };
    for mode in [
        Mode::WrongHuman,
        Mode::WrongOwner,
        Mode::WrongDigest,
        Mode::WrongRoles,
        Mode::Duplicate,
        Mode::Unavailable,
        Mode::Oversized,
    ] {
        let good = OwnerFixture::new(&f, OnboardingArea::Workshop, Mode::Valid).await;
        let bad = OwnerFixture::new(&f, OnboardingArea::Homelab, mode).await;
        configure(&mut f, &[&good, &bad]);
        let error = denied(catalog(State(f.state.clone()), Extension(f.human.clone())).await);
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        let response = error.into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 32768)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(!body.contains("SENTINEL"));
        assert!(!body.contains(&good.secret));
        assert!(!body.contains("repository_url"));
        good.close().await;
        bad.close().await;
    }
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn human_and_session_changes_during_unlocked_owner_http_fence_metadata_delivery() {
    let Some(mut f) = crate::auth::device::tests::fixture().await else {
        return;
    };
    for mode in [
        Mode::DisableHuman,
        Mode::RevokeSession,
        Mode::ChangeIdentity,
    ] {
        sqlx::query("UPDATE auth_principals SET disabled_at=NULL,subject='Human' WHERE id=$1")
            .bind(f.human.id)
            .execute(&f.db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE auth_web_sessions SET revoked_at=NULL WHERE id=$1")
            .bind(f.human.session_id)
            .execute(&f.db.pool)
            .await
            .unwrap();
        let owner = OwnerFixture::new(&f, OnboardingArea::Workshop, mode).await;
        configure(&mut f, &[&owner]);
        // Callback writes would block if aggregation retained its SQL locks.
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            catalog(State(f.state.clone()), Extension(f.human.clone())),
        )
        .await
        .expect("no SQL locks across owner HTTP");
        let error = denied(result);
        assert_eq!(
            error.status,
            if matches!(mode, Mode::ChangeIdentity) {
                StatusCode::CONFLICT
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
        owner.close().await;
    }
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn catalog_registry_and_browser_only_policy_are_fail_closed_and_opt_in() {
    let Some(mut f) = crate::auth::device::tests::fixture().await else {
        return;
    };
    let owner = OwnerFixture::new(&f, OnboardingArea::Workshop, Mode::Valid).await;
    configure(&mut f, &[&owner]);
    let mut old = owner.config.clone();
    old.delivery_token_env = None;
    old.delivery_policy_revision = None;
    f.state
        .config
        .auth
        .company_onboarding
        .as_mut()
        .unwrap()
        .owners = vec![old];
    let response = catalog(State(f.state.clone()), Extension(f.human.clone()))
        .await
        .unwrap();
    let value: CompanyDeviceCatalogResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 32768)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(value.projects.is_empty());
    assert!(owner.seen.lock().unwrap().is_empty());
    configure(&mut f, &[&owner]);
    f.state
        .config
        .auth
        .company_onboarding
        .as_mut()
        .unwrap()
        .owners[0]
        .delivery_policy_revision = None;
    assert_eq!(
        denied(catalog(State(f.state.clone()), Extension(f.human.clone())).await).status,
        StatusCode::FORBIDDEN
    );
    configure(&mut f, &[&owner]);
    let source = f
        .state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .unwrap()
        .owners[0]
        .clone();
    for _ in 0..3 {
        let mut other = source.clone();
        other.owner_instance_id = Uuid::new_v4();
        f.state
            .config
            .auth
            .company_onboarding
            .as_mut()
            .unwrap()
            .owners
            .push(other);
    }
    assert_eq!(
        denied(catalog(State(f.state.clone()), Extension(f.human.clone())).await).status,
        StatusCode::FORBIDDEN
    );
    configure(&mut f, &[&owner]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let url = format!("http://{address}/v1/onboarding/company/projects");
    assert_eq!(
        client.get(&url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let bearer = crate::auth::create_service_token(
        &f.db.pool,
        &f.human,
        AuthServiceTokenCreateRequest {
            name: "Synthetic ordinary reader".into(),
            project: "local".into(),
            role: AuthRole::Reader,
            expires_in_seconds: Some(60),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        client
            .get(&url)
            .bearer_auth(bearer.token.unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    server.abort();
    let _ = server.await;
    owner.close().await;
    f.db.cleanup().await.unwrap();
}
