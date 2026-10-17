// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::auth::device::tests::{Fixture, fixture};
use axum::{Router, extract::ConnectInfo, http::HeaderMap, routing::post};
use codifix_federation::{
    publication::{InstanceId, SpaceId},
    subscription::ProjectScope,
};
use codifix_federation_runtime::GatewaySecurity;
use codifix_federation_store::SpacePolicy;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Clone)]
struct Fake {
    manifest: OnboardingProject,
    mode: Arc<AtomicU8>,
    seen: Arc<Mutex<Vec<(String, HeaderMap, serde_json::Value)>>>,
    receipts: Arc<Mutex<BTreeMap<Uuid, OwnerDeviceApprovalReceipt>>>,
    keys: Vec<codifix_federation_store::ReceiptKey>,
    pool: sqlx::PgPool,
    human: Uuid,
    session: Uuid,
    space: SpaceId,
    authority: InstanceId,
}
async fn fake_catalog(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(request): Json<OwnerDeviceCatalogRequest>,
) -> Json<OwnerDeviceCatalogResponse> {
    fake.seen
        .lock()
        .unwrap()
        .push(("catalog".into(), headers, json!(request)));
    Json(OwnerDeviceCatalogResponse {
        request_sha256: company_catalog::digest(&request).unwrap(),
        observed_at: Utc::now(),
        projects: vec![OwnerDeviceCatalogProject {
            curated_contributions_available: true,
            manifest_sha256: company_catalog::digest(&fake.manifest).unwrap(),
            project: OnboardingDeviceReviewProject {
                curated_contributions_allowed: false,
                manifest: fake.manifest,
                allowed_roles: vec![AuthRole::Reader],
                offline_available: true,
            },
        }],
    })
}
async fn fake_approve(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(request): Json<OwnerDeviceApprovalRequest>,
) -> Result<Json<OwnerDeviceApprovalReceipt>, StatusCode> {
    fake.seen
        .lock()
        .unwrap()
        .push(("approve".into(), headers, json!(request)));
    let receipt = {
        let mut receipts = fake.receipts.lock().unwrap();
        receipts
            .entry(request.operation_id)
            .or_insert_with(|| OwnerDeviceApprovalReceipt {
                request: request.clone(),
                request_sha256: company_catalog::digest(&request).unwrap(),
                human_principal_id: Uuid::new_v4(),
                committed_at: Utc::now(),
            })
            .clone()
    };
    match fake.mode.load(Ordering::SeqCst) {
        1 => {
            fake.mode.store(0, Ordering::SeqCst);
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        3 => {
            sqlx::query("UPDATE auth_principals SET disabled_at=clock_timestamp() WHERE id=$1")
                .bind(fake.human)
                .execute(&fake.pool)
                .await
                .unwrap();
        }
        4 => {
            sqlx::query("UPDATE onboarding_device_requests SET status='denied',decided_at=clock_timestamp() WHERE request_id=$1 AND status='pending'").bind(request.device_request_id).execute(&fake.pool).await.unwrap();
        }
        5 => {
            sqlx::query("UPDATE auth_web_sessions SET revoked_at=clock_timestamp() WHERE id=$1")
                .bind(fake.session)
                .execute(&fake.pool)
                .await
                .unwrap();
        }
        6 => {
            sqlx::query("UPDATE onboarding_device_requests SET expires_at=clock_timestamp()-interval '1 second' WHERE request_id=$1").bind(request.device_request_id).execute(&fake.pool).await.unwrap();
        }
        7 => {
            let mut wrong = receipt;
            wrong.request.subject = "another-human".into();
            wrong.request_sha256 = company_catalog::digest(&wrong.request).unwrap();
            return Ok(Json(wrong));
        }
        _ => {}
    }
    Ok(Json(receipt))
}
async fn fake_subscription(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(request): Json<OwnerDeviceSubscriptionRequest>,
) -> Result<Json<DeviceProjectSubscription>, StatusCode> {
    fake.seen
        .lock()
        .unwrap()
        .push(("subscription".into(), headers, json!(request)));
    match fake.mode.load(Ordering::SeqCst) {
        10 => return Err(StatusCode::FORBIDDEN),
        13 => return Err(StatusCode::CONFLICT),
        15 => return Err(StatusCode::SERVICE_UNAVAILABLE),
        16 => return Err(StatusCode::UNAUTHORIZED),
        17 => return Err(StatusCode::NOT_FOUND),
        18 => return Err(StatusCode::GONE),
        _ => {}
    }
    let receipt = fake
        .receipts
        .lock()
        .unwrap()
        .get(&request.approval_id)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    let r = receipt.request;
    let mut bootstrap = DeviceProjectSubscription {
        schema_version: 1,
        device: r.device_id,
        scope: ProjectScope {
            company: r.company_id,
            area: r.area.as_str().into(),
            owner: InstanceId::new(r.owner_instance_id).unwrap(),
            project: r.project_id,
            space: fake.space,
        },
        catalog_revision: r.manifest_revision as u64,
        authority: fake.authority,
        receipt_keys: fake.keys,
    };
    match fake.mode.load(Ordering::SeqCst) {
        2 => bootstrap.device = Uuid::new_v4(),
        8 => bootstrap.catalog_revision += 1,
        9 => bootstrap.receipt_keys.clear(),
        14 => {
            bootstrap.receipt_keys = GatewaySecurity::initial(&[63; 32], SpacePolicy::default())
                .unwrap()
                .trust()
                .keys()
                .to_vec()
        }
        _ => {}
    }
    Ok(Json(bootstrap))
}
pub(crate) struct Owner {
    fake: Fake,
    config: codifix_config::CompanyOwnerConfig,
    secret: String,
    env: String,
    server: tokio::task::JoinHandle<()>,
}
impl Owner {
    async fn new(f: &Fixture) -> Self {
        Self::new_with_destinations(f, Vec::new()).await
    }
    pub(crate) async fn new_with_destinations(
        f: &Fixture,
        destinations: Vec<OnboardingDestination>,
    ) -> Self {
        let row=sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1").bind(f.project).fetch_one(&f.db.pool).await.unwrap();
        let mut manifest = crate::auth::onboarding::project_row(&row).unwrap();
        manifest.project_id = Uuid::new_v4();
        manifest.entry.area = OnboardingArea::Homelab;
        manifest.entry.owner_instance_id = Uuid::new_v4();
        manifest.entry.destinations = destinations;
        let security = GatewaySecurity::initial(&[61; 32], SpacePolicy::default()).unwrap();
        let fake = Fake {
            manifest: manifest.clone(),
            mode: Arc::new(AtomicU8::new(0)),
            seen: Arc::default(),
            receipts: Arc::default(),
            keys: security.trust().keys().to_vec(),
            pool: f.db.pool.clone(),
            human: f.human.id,
            session: f.human.session_id.unwrap(),
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
            authority: InstanceId::new(Uuid::new_v4()).unwrap(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = Router::new()
            .route(
                "/v1/onboarding/company-delivery/catalog",
                post(fake_catalog),
            )
            .route(
                "/v1/onboarding/company-delivery/approve",
                post(fake_approve),
            )
            .route(
                "/v1/onboarding/company-delivery/subscription",
                post(fake_subscription),
            )
            .with_state(fake.clone());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let env = format!("MEMORY_ENROLLMENT_SYNTHETIC_{}", Uuid::new_v4().simple());
        let secret = format!("synthetic-delivery-{}", Uuid::new_v4());
        unsafe { std::env::set_var(&env, &secret) };
        let config = codifix_config::CompanyOwnerConfig {
            owner_instance_id: manifest.entry.owner_instance_id,
            area: manifest.entry.area,
            base_url: format!("http://{addr}"),
            token_env: "DO_NOT_FORWARD_MANAGEMENT".into(),
            issuer: f.state.config.auth.oidc.issuer_url.clone(),
            delegation_policy_revision: 1,
            delivery_token_env: Some(env.clone()),
            delivery_policy_revision: Some(1),
        };
        Self {
            fake,
            config,
            secret,
            env,
            server,
        }
    }
    pub(crate) fn mode(&self, value: u8) {
        self.fake.mode.store(value, Ordering::SeqCst);
    }
    pub(crate) async fn close(self) {
        self.server.abort();
        let _ = self.server.await;
        unsafe { std::env::remove_var(&self.env) };
    }
}
pub(crate) fn configure(f: &mut Fixture, owner: &Owner) {
    f.state.config.auth.company_onboarding = Some(codifix_config::CompanyOnboardingConfig {
        company_id: Uuid::new_v4(),
        owners: vec![owner.config.clone()],
    });
}
pub(crate) async fn start(f: &Fixture) -> (OnboardingDeviceStartResponse, String, String) {
    static PEER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let peer = PEER.fetch_add(1, Ordering::Relaxed);
    let credential = format!(
        "device-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let poll = format!(
        "poll-{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let Json(response) = device::start(
        State(f.state.clone()),
        ConnectInfo(std::net::SocketAddr::from((
            [127, 18, (peer >> 8) as u8, peer as u8],
            12345,
        ))),
        Json(OnboardingDeviceStartRequest {
            request_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            name: "Synthetic company laptop".into(),
            platform: "macos".into(),
            credential_sha256: hex(&crate::auth::hash_secret(&credential)),
            poll_sha256: hex(&crate::auth::hash_secret(&poll)),
        }),
    )
    .await
    .unwrap();
    assert!(
        response
            .verification_uri_complete
            .contains("company_device=1")
    );
    (response, credential, poll)
}
pub(crate) async fn request(
    f: &Fixture,
    start: &OnboardingDeviceStartResponse,
) -> CompanyDeviceApproveRequest {
    let Json(review) = review(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Query(device::ReviewQuery {
            code: start.user_code.clone(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(review.request_id, start.request_id);
    assert_eq!(review.projects.len(), 1);
    let item = &review.projects[0];
    let m = &item.project.manifest;
    CompanyDeviceApproveRequest {
        user_code: start.user_code.clone(),
        confirmed: true,
        projects: vec![CompanyDeviceSelection {
            area: m.entry.area,
            owner_instance_id: m.entry.owner_instance_id,
            project_id: m.project_id,
            manifest_revision: m.revision,
            manifest_sha256: item.manifest_sha256.clone(),
            offline_confirmed: true,
            contribution_confirmed: false,
        }],
    }
}
pub(crate) async fn call(
    f: &Fixture,
    r: &CompanyDeviceApproveRequest,
) -> Result<OnboardingDevicePollResponse, ApiError> {
    approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(r.clone()),
    )
    .await
    .map(|r| r.0)
}
async fn actor(f: &Fixture, credential: &str) -> AuthenticatedPrincipal {
    crate::auth::resolve_principal(
        &f.state,
        crate::auth::Credential::ApiToken(credential.into()),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn remote_only_activation_has_zero_generic_authority_exact_scope_and_no_fallback() {
    let Some(mut f) = fixture().await else { return };
    let owner = Owner::new(&f).await;
    configure(&mut f, &owner);
    f.state.config.auth.oidc.issuer_url.push('/');
    let (start, credential, _) = start(&f).await;
    let request = request(&f, &start).await;
    let result = call(&f, &request).await.unwrap();
    assert_eq!(result.status, "approved");
    assert_eq!(result.projects.as_ref().unwrap()[0].role, AuthRole::Reader);
    assert_eq!(json!(result), json!(call(&f, &request).await.unwrap()));
    let actor = actor(&f, &credential).await;
    assert!(actor.global.is_empty());
    assert!(actor.project_roles.is_empty());
    for permission in [
        Permission::MemoryRead,
        Permission::AuthManage,
        Permission::SystemAdmin,
        Permission::ScopedProjectDelivery,
        Permission::BundleExport,
    ] {
        assert!(!actor.permissions_for_project("local").contains(permission));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::routes::build_http_app(f.state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    for path in [
        "/v1/auth/tokens",
        "/v1/auth/memberships",
        "/v1/admin/onboarding/users",
    ] {
        let response = client
            .get(format!("{origin}{path}"))
            .bearer_auth(&credential)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
    server.abort();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth_project_memberships WHERE principal_id=$1"
        )
        .bind(actor.id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap(),
        0
    );
    let selection = &request.projects[0];
    let mut tx = f.db.pool.begin().await.unwrap();
    let binding = company_binding(
        &f.state,
        &mut tx,
        &actor,
        selection.area.as_str(),
        selection.owner_instance_id,
        selection.project_id,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(binding.device_id, result.device_id.unwrap());
    tx.commit().await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        company_binding(
            &f.state,
            &mut tx,
            &actor,
            "workshop",
            f.state.config.area.as_ref().unwrap().instance_id,
            f.project
        )
        .await
        .is_err()
    );
    drop(tx);
    let Json(catalog) = device::catalog(State(f.state.clone()), Extension(actor.clone()))
        .await
        .unwrap();
    assert_eq!(catalog["projects"].as_array().unwrap().len(), 1);
    assert!(
        catalog["projects"][0]
            .get("curated_contributions_allowed")
            .is_none()
    );
    assert_eq!(
        catalog["projects"][0]["manifest"]["project_id"],
        json!(selection.project_id)
    );
    owner.fake.mode.store(10, Ordering::SeqCst);
    assert!(
        device::catalog(State(f.state.clone()), Extension(actor.clone()))
            .await
            .is_err()
    );
    owner.fake.mode.store(0, Ordering::SeqCst);
    for (_, headers, body) in owner.fake.seen.lock().unwrap().iter() {
        assert_eq!(
            headers.get("authorization").unwrap().to_str().unwrap(),
            format!("Bearer {}", owner.secret)
        );
        assert!(headers.get("cookie").is_none());
        assert!(!body.to_string().contains(&credential));
        assert!(!body.to_string().contains("DO_NOT_FORWARD_MANAGEMENT"));
    }
    let mut altered = request.clone();
    altered.projects[0].offline_confirmed = false;
    assert!(call(&f, &altered).await.is_err());
    let Json(_) = device::logout(State(f.state.clone()), Extension(actor.clone()))
        .await
        .unwrap();
    assert!(call(&f, &request).await.is_err());
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        company_binding(
            &f.state,
            &mut tx,
            &actor,
            selection.area.as_str(),
            selection.owner_instance_id,
            selection.project_id
        )
        .await
        .is_err()
    );
    drop(tx);
    owner.close().await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn device_catalog_reports_explicit_contribution_consent_without_writer_role() {
    let Some(mut f) = fixture().await else { return };
    let owner = Owner::new(&f).await;
    configure(&mut f, &owner);
    let (started, credential, _) = start(&f).await;
    let mut request = request(&f, &started).await;
    request.projects[0].contribution_confirmed = true;
    call(&f, &request).await.unwrap();
    let machine = actor(&f, &credential).await;
    let Json(catalog) = device::catalog(State(f.state.clone()), Extension(machine.clone()))
        .await
        .unwrap();
    assert_eq!(catalog["projects"][0]["allowed_roles"], json!(["reader"]));
    assert_eq!(
        catalog["projects"][0]["curated_contributions_allowed"],
        true
    );
    let mut legacy = catalog["projects"][0].clone();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("curated_contributions_allowed");
    assert!(
        !serde_json::from_value::<OnboardingDeviceReviewProject>(legacy)
            .unwrap()
            .curated_contributions_allowed
    );
    assert!(machine.global.is_empty());
    assert!(machine.project_roles.is_empty());
    owner.fake.mode.store(10, Ordering::SeqCst);
    assert!(
        device::catalog(State(f.state.clone()), Extension(machine))
            .await
            .is_err()
    );
    owner.close().await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn lost_owner_reply_resumes_exact_durable_operation_and_local_path_cannot_claim_intent() {
    let Some(mut f) = fixture().await else { return };
    let owner = Owner::new(&f).await;
    configure(&mut f, &owner);
    let (start, _, _) = start(&f).await;
    let request = request(&f, &start).await;
    owner.fake.mode.store(1, Ordering::SeqCst);
    let error = call(&f, &request).await.unwrap_err();
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(error.message.contains(&start.request_id.to_string()));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM onboarding_devices WHERE id IN(SELECT device_id FROM onboarding_device_requests WHERE request_id=$1)").bind(start.request_id).fetch_one(&f.db.pool).await.unwrap(),0);
    let row = sqlx::query(
        "SELECT state,receipt_json FROM onboarding_company_device_outcomes WHERE request_id=$1",
    )
    .bind(start.request_id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "uncertain");
    assert!(
        row.get::<Option<serde_json::Value>, _>("receipt_json")
            .is_none()
    );
    let local = device::approve(
        State(f.state.clone()),
        Extension(f.human.clone()),
        Json(OnboardingDeviceApproveRequest {
            user_code: start.user_code.clone(),
            confirmed: true,
            projects: vec![],
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(local.status, StatusCode::CONFLICT);
    let mut changed = request.clone();
    changed.projects[0].manifest_revision += 1;
    assert_eq!(
        call(&f, &changed).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    let result = call(&f, &request).await.unwrap();
    assert_eq!(result.status, "approved");
    {
        let requests = owner.fake.seen.lock().unwrap();
        let approvals = requests
            .iter()
            .filter(|(kind, _, _)| kind == "approve")
            .map(|(_, _, body)| body.clone())
            .collect::<Vec<_>>();
        assert_eq!(approvals.len(), 2);
        assert_eq!(approvals[0], approvals[1]);
    }
    owner.close().await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn malformed_receipt_or_bootstrap_never_activates_and_valid_resume_keeps_first_receipt() {
    let Some(mut f) = fixture().await else { return };
    let owner = Owner::new(&f).await;
    configure(&mut f, &owner);
    for mode in [2, 7, 8, 9] {
        let (start, _, _) = start(&f).await;
        let request = request(&f, &start).await;
        owner.fake.mode.store(mode, Ordering::SeqCst);
        assert_eq!(
            call(&f, &request).await.unwrap_err().status,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM onboarding_device_requests WHERE request_id=$1"
            )
            .bind(start.request_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
            "pending"
        );
        owner.fake.mode.store(0, Ordering::SeqCst);
        assert_eq!(call(&f, &request).await.unwrap().status, "approved");
    }
    owner.close().await;
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn human_session_cancel_and_challenge_expiry_during_unlocked_http_fence_activation() {
    for mode in [3, 4, 5, 6] {
        let Some(mut f) = fixture().await else { return };
        let owner = Owner::new(&f).await;
        configure(&mut f, &owner);
        let (start, _, _) = start(&f).await;
        let request = request(&f, &start).await;
        owner.fake.mode.store(mode, Ordering::SeqCst);
        assert!(call(&f, &request).await.is_err());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM onboarding_devices WHERE id IN(SELECT device_id FROM onboarding_device_requests WHERE request_id=$1)").bind(start.request_id).fetch_one(&f.db.pool).await.unwrap(),0);
        owner.close().await;
        f.db.cleanup().await.unwrap();
    }
}
