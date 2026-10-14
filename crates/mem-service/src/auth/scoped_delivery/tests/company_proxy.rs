// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two independent databases and the real signed gateway path. No provider,
//! real IdP, whole-space export or external service is involved.
use super::*;
use axum::{extract::ConnectInfo, middleware::Next};
use mem_record::{CompanyDeviceApproveRequest, CompanyDeviceSelection};
use std::net::{Ipv4Addr, SocketAddr};

#[derive(Clone)]
struct ObservedOwner {
    expected: String,
    seen: Arc<Mutex<Vec<String>>>,
    revoke: Arc<Mutex<Option<(sqlx::PgPool, Uuid)>>>,
    corrupt: Arc<Mutex<bool>>,
}
async fn observe(
    State(observed): State<ObservedOwner>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    assert_eq!(
        request.headers().get(header::AUTHORIZATION).unwrap(),
        &format!("Bearer {}", observed.expected)
    );
    assert!(!request.headers().contains_key(header::COOKIE));
    observed.seen.lock().unwrap().push(path.clone());
    let mut response = next.run(request).await;
    if path.ends_with("/snapshot") || path.ends_with("/replica") {
        let hook = observed.revoke.lock().unwrap().take();
        if let Some((pool, human)) = hook {
            sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
                .bind(human)
                .execute(&pool)
                .await
                .unwrap();
        }
        let corrupt = std::mem::take(&mut *observed.corrupt.lock().unwrap());
        if corrupt && response.status().is_success() {
            let (parts, body) = response.into_parts();
            let mut signed: serde_json::Value =
                serde_json::from_slice(&axum::body::to_bytes(body, 4 * 1024 * 1024).await.unwrap())
                    .unwrap();
            signed["signature"][0] = json!(signed["signature"][0].as_u64().unwrap() ^ 1);
            response = Response::from_parts(
                parts,
                axum::body::Body::from(serde_json::to_vec(&signed).unwrap()),
            );
        }
    }
    response
}

struct CompanyFixture {
    owner: DeliveryFixture,
    company: super::super::super::device::tests::Fixture,
    observed: ObservedOwner,
    actor: AuthenticatedPrincipal,
    credential: String,
    approval: CompanyDeviceApproveRequest,
    owner_server: tokio::task::JoinHandle<()>,
    company_server: tokio::task::JoinHandle<()>,
    company_origin: String,
    credential_env: String,
}
impl CompanyFixture {
    async fn new(n: u8) -> Option<Self> {
        Self::with_contributions(n, false).await
    }
    async fn with_contributions(n: u8, contribution_confirmed: bool) -> Option<Self> {
        let mut owner = DeliveryFixture::new(n).await?;
        let (delegate, example) =
            super::super::super::company_device_delivery::tests::setup(&mut owner.base).await;
        owner
            .base
            .state
            .config
            .auth
            .company_onboarding
            .as_mut()
            .unwrap()
            .company_id = example.company_id;
        owner.share().await;
        let secret = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        sqlx::query("UPDATE auth_service_tokens SET token_hash=$2 WHERE id=$1")
            .bind(delegate.token_id)
            .bind(crate::auth::hash_secret(&secret))
            .execute(&owner.base.db.pool)
            .await
            .unwrap();
        let observed = ObservedOwner {
            expected: secret.clone(),
            seen: Default::default(),
            revoke: Default::default(),
            corrupt: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let router = crate::routes::build_http_app(owner.base.state.clone()).layer(
            axum::middleware::from_fn_with_state(observed.clone(), observe),
        );
        let owner_server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut company = super::super::super::device::tests::fixture().await?;
        company.state.config.area.as_mut().unwrap().name = "homelab".into();
        let credential_env = format!("MEMORY_TEST_COMPANY_PROXY_{}", Uuid::new_v4().simple());
        // Synthetic, unique test environment entry; never a host/server credential.
        unsafe {
            std::env::set_var(&credential_env, &secret);
        }
        company.state.config.auth.company_onboarding = Some(serde_json::from_value(json!({
            "company_id":example.company_id,
            "owners":[{
                "owner_instance_id":example.owner_instance_id,"area":"workshop",
                "issuer":example.issuer,"base_url":origin,"token_env":"unused_management_secret",
                "delegation_policy_revision":1,
                "delivery_token_env":credential_env,"delivery_policy_revision":1
            }]
        })).unwrap());
        let credential = format!("mlt_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let poll_secret = format!(
            "poll_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let hash = |s: &str| {
            crate::auth::hash_secret(s)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let Json(started) = super::super::super::device::start(
            State(company.state.clone()),
            ConnectInfo(SocketAddr::from((Ipv4Addr::new(192, 0, 2, n), 4242))),
            Json(OnboardingDeviceStartRequest {
                request_id: Uuid::new_v4(),
                device_id: Uuid::new_v4(),
                name: "Synthetic company Mac".into(),
                platform: "macos".into(),
                credential_sha256: hash(&credential),
                poll_sha256: hash(&poll_secret),
            }),
        )
        .await
        .unwrap();
        assert!(
            started
                .verification_uri_complete
                .ends_with("&company_device=1")
        );
        let Json(review) = super::super::super::company_devices::review(
            State(company.state.clone()),
            Extension(company.human.clone()),
            axum::extract::Query(super::super::super::device::ReviewQuery {
                code: started.user_code.clone(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(review.projects.len(), 1);
        let selected = &review.projects[0];
        let approval = CompanyDeviceApproveRequest {
            user_code: started.user_code,
            confirmed: true,
            projects: vec![CompanyDeviceSelection {
                area: selected.project.manifest.entry.area,
                owner_instance_id: selected.project.manifest.entry.owner_instance_id,
                project_id: selected.project.manifest.project_id,
                manifest_revision: selected.project.manifest.revision,
                manifest_sha256: selected.manifest_sha256.clone(),
                offline_confirmed: true,
                contribution_confirmed,
            }],
        };
        let Json(result) = super::super::super::company_devices::approve(
            State(company.state.clone()),
            Extension(company.human.clone()),
            Json(approval.clone()),
        )
        .await
        .unwrap();
        assert_eq!(serde_json::to_value(result).unwrap()["status"], "approved");
        let actor = crate::auth::resolve_principal(
            &company.state,
            crate::auth::Credential::ApiToken(credential.clone()),
        )
        .await
        .unwrap()
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let company_origin = format!("http://{}", listener.local_addr().unwrap());
        let router = crate::routes::build_http_app(company.state.clone());
        let company_server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Some(Self {
            owner,
            company,
            observed,
            actor,
            credential,
            approval,
            owner_server,
            company_server,
            company_origin,
            credential_env,
        })
    }
    fn path(&self, suffix: &str) -> String {
        format!(
            "/v1/onboarding/areas/workshop/owners/{}/projects/{}/{suffix}",
            self.owner.fake.owner.as_uuid(),
            self.owner.base.project
        )
    }
    async fn http(&self, method: &str, path: &str, body: Option<serde_json::Value>) -> Response {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut request = client
            .request(
                method.parse().unwrap(),
                format!("{}{path}", self.company_origin),
            )
            .bearer_auth(&self.credential);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.unwrap();
        assert!(bytes.len() <= 4 * 1024 * 1024);
        let mut response = Response::builder()
            .status(status)
            .body(axum::body::Body::from(bytes))
            .unwrap();
        *response.headers_mut() = headers;
        response
    }
    async fn close(self) {
        self.owner_server.abort();
        self.company_server.abort();
        unsafe {
            std::env::remove_var(self.credential_env);
        }
        self.company.db.cleanup().await.unwrap();
        self.owner.close().await;
    }
}

#[tokio::test]
async fn company_private_replica_delivers_unpublished_curated_rows_and_rechecks_access() {
    use mem_federation::device_replica::DeviceReplicaRequest;
    use mem_federation_runtime::SignedDeviceReplicaPage;
    let Some(f) = CompanyFixture::with_contributions(129, true).await else {
        return;
    };
    let response = f.http("GET", &f.path("subscription"), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bootstrap: mem_federation_runtime::DeviceProjectSubscription = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap(),
    )
    .unwrap();
    let memory = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,FALSE,'UNPUBLISHED_CURATED_SENTINEL','curated','project','project',3,0.8,now(),now(),to_tsvector('simple','curated'))")
        .bind(memory).bind(f.owner.base.project).execute(&f.owner.base.db.pool).await.unwrap();
    sqlx::query("INSERT INTO memory_sources(id,memory_entry_id,file_path,source_kind,excerpt,created_at) VALUES($1,$2,'/PRIVATE_PATH','file','RAW_SESSION_SENTINEL',now())").bind(Uuid::new_v4()).bind(memory).execute(&f.owner.base.db.pool).await.unwrap();
    let request = DeviceReplicaRequest {
        schema_version: 1,
        nonce: Uuid::new_v4(),
        device: bootstrap.device,
        scope: bootstrap.scope.clone(),
        catalog_revision: bootstrap.catalog_revision,
        after: 0,
        through: None,
        limit: 32,
    };
    let response = f
        .http("POST", &f.path("replica"), Some(json!(request)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("UNPUBLISHED_CURATED_SENTINEL"));
    assert!(!String::from_utf8_lossy(&bytes).contains("RAW_SESSION_SENTINEL"));
    assert!(!String::from_utf8_lossy(&bytes).contains("/PRIVATE_PATH"));
    let signed: SignedDeviceReplicaPage = serde_json::from_slice(&bytes).unwrap();
    bootstrap
        .validate()
        .unwrap()
        .verify_device_replica_page(&signed, &request, bootstrap.authority)
        .unwrap();
    let contribution = mem_record::CuratedDeviceContribution {
        schema_version: 2,
        operation_id: Uuid::new_v4(),
        local_memory_id: Uuid::new_v4(),
        authored_at: chrono::Utc::now(),
        binding: mem_record::LaptopContributionBinding {
            scope: mem_record::LaptopContributionScope {
                company_id: bootstrap.scope.company,
                area: mem_record::OnboardingArea::Workshop,
                owner_instance_id: bootstrap.scope.owner.as_uuid(),
                project_id: bootstrap.scope.project,
                space_id: bootstrap.scope.space.as_uuid(),
            },
            device_id: bootstrap.device,
            manifest_revision: bootstrap.catalog_revision as i64,
            manifest_sha256: f.approval.projects[0].manifest_sha256.clone(),
        },
        change: mem_record::CuratedDeviceChange::Add {
            content: mem_record::CuratedDeviceContent {
                canonical_text: "LAPTOP_CURATED_ADDITION".into(),
                summary: "local addition".into(),
                memory_type: mem_record::MemoryType::Project,
                importance: 3,
                confidence: 0.8,
            },
        },
    };
    let mut saved = None;
    for _ in 0..2 {
        let response = f
            .http("POST", &f.path("contributions"), Some(json!(contribution)))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: mem_record::DeviceContributionReceipt = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            receipt.outcome,
            mem_record::DeviceContributionOutcome::Accepted
        );
        if let Some(previous) = &saved {
            assert_eq!(previous, &receipt);
        }
        saved = Some(receipt);
    }
    let mut next = request.clone();
    next.nonce = Uuid::new_v4();
    next.after = signed.claims.page.next;
    let response = f.http("POST", &f.path("replica"), Some(json!(next))).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("LAPTOP_CURATED_ADDITION"));
    let delta: SignedDeviceReplicaPage = serde_json::from_slice(&bytes).unwrap();
    bootstrap
        .validate()
        .unwrap()
        .verify_device_replica_page(&delta, &next, bootstrap.authority)
        .unwrap();
    let mut wrong = request.clone();
    wrong.scope.project = Uuid::new_v4();
    assert_eq!(
        f.http("POST", &f.path("replica"), Some(json!(wrong)))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    *f.observed.corrupt.lock().unwrap() = true;
    let response = f
        .http("POST", &f.path("replica"), Some(json!(request)))
        .await;
    assert!(!response.status().is_success());
    *f.observed.revoke.lock().unwrap() = Some((f.company.db.pool.clone(), f.company.human.id));
    let response = f
        .http("POST", &f.path("replica"), Some(json!(request)))
        .await;
    assert!(!response.status().is_success());
    // Restore only this synthetic company principal, then revoke at the owner
    // before the next transfer. Revocation cannot recall already transmitted bytes.
    sqlx::query("UPDATE auth_principals SET disabled_at=NULL WHERE id=$1")
        .bind(f.company.human.id)
        .execute(&f.company.db.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.owner.base.human.id)
        .execute(&f.owner.base.db.pool)
        .await
        .unwrap();
    assert!(
        !f.http("POST", &f.path("replica"), Some(json!(request)))
            .await
            .status()
            .is_success()
    );
    f.close().await;
}

#[tokio::test]
async fn company_device_end_to_end_signed_delivery_never_forwards_laptop_authority() {
    let Some(f) = CompanyFixture::new(120).await else {
        return;
    };
    let Json(replay) = super::super::super::company_devices::approve(
        State(f.company.state.clone()),
        Extension(f.company.human.clone()),
        Json(f.approval.clone()),
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(replay).unwrap()["status"], "approved");
    assert_eq!(
        0,
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth_project_memberships WHERE principal_id=$1"
        )
        .bind(f.actor.id)
        .fetch_one(&f.company.db.pool)
        .await
        .unwrap()
    );
    let response = f.http("GET", &f.path("subscription"), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bootstrap: mem_federation_runtime::DeviceProjectSubscription = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap(),
    )
    .unwrap();
    let request = OnboardingProjectSnapshotRequest {
        nonce: Uuid::new_v4(),
        after: 0,
        through: None,
        limit: 32,
    };
    let response = f
        .http("POST", &f.path("snapshot"), Some(json!(request)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let page: SignedProjectPage = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page.claims.page.request.device, bootstrap.device);
    assert_eq!(page.claims.page.request.scope, bootstrap.scope);
    bootstrap
        .validate()
        .unwrap()
        .verify_project_page(page.clone(), &page.claims.page.request, bootstrap.authority)
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("approved scoped knowledge"));
    assert!(!String::from_utf8_lossy(&bytes).contains("/synthetic"));
    let replay = f
        .http("POST", &f.path("snapshot"), Some(json!(request)))
        .await;
    assert_eq!(replay.status(), StatusCode::OK);
    let replay: SignedProjectPage = serde_json::from_slice(
        &axum::body::to_bytes(replay.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay.claims.page.entries, page.claims.page.entries);
    let mut altered = request;
    altered.limit = 1;
    assert_eq!(
        f.http("POST", &f.path("snapshot"), Some(json!(altered)))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let observed = f.observed.seen.lock().unwrap().len();
    let unselected = f.path("subscription").replace(
        &f.owner.base.project.to_string(),
        &Uuid::new_v4().to_string(),
    );
    assert_eq!(
        f.http("GET", &unselected, None).await.status(),
        StatusCode::FORBIDDEN
    );
    let wrong = f.path("subscription").replace("/workshop/", "/trust/");
    assert_eq!(
        f.http("GET", &wrong, None).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(observed, f.observed.seen.lock().unwrap().len());
    // It is a selective device, not a normal Memory/export/federation client.
    for path in [
        "/v1/projects/local/memories".to_string(),
        format!("/v1/onboarding/projects/{}/subscription", f.company.project),
    ] {
        assert_eq!(
            f.http("GET", &path, None).await.status(),
            StatusCode::FORBIDDEN
        );
    }
    f.close().await;
}

#[tokio::test]
async fn company_device_rechecks_human_after_owner_bytes_and_rejects_bad_signatures() {
    let Some(f) = CompanyFixture::new(121).await else {
        return;
    };
    let req = || OnboardingProjectSnapshotRequest {
        nonce: Uuid::new_v4(),
        after: 0,
        through: None,
        limit: 32,
    };
    // Owner verification succeeded; a forged response on the owner→company hop
    // must still be rejected against the immutable enrolled gateway trust.
    *f.observed.corrupt.lock().unwrap() = true;
    assert_eq!(
        f.http("POST", &f.path("snapshot"), Some(json!(req())))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    *f.owner.fake.hook.lock().unwrap() = Some(Hook::ExpirePage);
    assert_ne!(
        f.http("POST", &f.path("snapshot"), Some(json!(req())))
            .await
            .status(),
        StatusCode::OK
    );
    *f.observed.revoke.lock().unwrap() = Some((f.company.db.pool.clone(), f.company.human.id));
    let response = f
        .http("POST", &f.path("snapshot"), Some(json!(req())))
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = axum::body::to_bytes(response.into_body(), 8192)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("approved scoped knowledge"));
    let count = f.observed.seen.lock().unwrap().len();
    assert_ne!(
        f.http("GET", &f.path("subscription"), None).await.status(),
        StatusCode::OK
    );
    assert_eq!(count, f.observed.seen.lock().unwrap().len());
    f.close().await;
}
