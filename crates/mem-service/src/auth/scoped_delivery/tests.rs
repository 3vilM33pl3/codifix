// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_federation::{AuthorIdentity, RecordLocator, SpaceCapability};
use mem_federation_runtime::{
    Endpoint, GatewayClient, GatewaySecurity, InstanceSync, SignedProjectPage,
};
use mem_federation_store::{
    Claim, GatewayStore, PublicationIntent, RetryPolicy, SpacePolicy, WriterFence,
};
use std::sync::{Arc, Mutex};

mod company_proxy;

#[tokio::test]
async fn company_device_uses_signed_scoped_feed_without_owner_device_or_bearer() {
    let Some(mut f) = DeliveryFixture::new(119).await else {
        return;
    };
    let (delegate, approval) =
        super::super::company_device_delivery::tests::setup(&mut f.base).await;
    // Existing per-project registration belongs to the approved company scope.
    f.base
        .state
        .config
        .auth
        .company_onboarding
        .as_mut()
        .unwrap()
        .company_id = approval.company_id;
    f.share().await;
    let Json(receipt) = super::super::company_device_delivery::approve(
        State(f.base.state.clone()),
        Extension(delegate.clone()),
        Json(approval.clone()),
    )
    .await
    .unwrap();
    let Json(subscription) = super::super::company_device_delivery::subscription(
        State(f.base.state.clone()),
        Extension(delegate.clone()),
        Json(OwnerDeviceSubscriptionRequest {
            approval_id: receipt.request.operation_id,
        }),
    )
    .await
    .unwrap();
    assert_eq!(subscription.device, approval.device_id);
    assert_eq!(subscription.scope.company, approval.company_id);
    let request = OwnerDeviceSnapshotRequest {
        approval_id: approval.operation_id,
        snapshot: OnboardingProjectSnapshotRequest {
            nonce: Uuid::new_v4(),
            after: 0,
            through: None,
            limit: 32,
        },
    };
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        super::super::company_device_delivery::snapshot(
            State(f.base.state.clone()),
            Extension(delegate.clone()),
            Json(request.clone()),
        ),
    )
    .await
    .expect("no SQL locks across gateway HTTP")
    .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let page: SignedProjectPage = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page.claims.page.request.device, approval.device_id);
    assert_eq!(page.claims.page.request.scope, subscription.scope);
    assert!(String::from_utf8_lossy(&bytes).contains("approved scoped knowledge"));
    assert_eq!(
        0,
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM onboarding_devices WHERE id=$1")
            .bind(approval.device_id)
            .fetch_one(&f.base.db.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        0,
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM onboarding_owner_snapshot_requests WHERE device_id=$1"
        )
        .bind(approval.device_id)
        .fetch_one(&f.base.db.pool)
        .await
        .unwrap()
    );
    let repeated = super::super::company_device_delivery::snapshot(
        State(f.base.state.clone()),
        Extension(delegate.clone()),
        Json(request.clone()),
    )
    .await
    .unwrap();
    let replay: SignedProjectPage = serde_json::from_slice(
        &axum::body::to_bytes(repeated.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(replay.claims.page.entries, page.claims.page.entries);
    let mut changed = request.clone();
    changed.snapshot.limit = 1;
    assert!(
        super::super::company_device_delivery::snapshot(
            State(f.base.state.clone()),
            Extension(delegate.clone()),
            Json(changed)
        )
        .await
        .is_err()
    );
    // A local device token is not company delivery authority.
    assert!(
        super::super::company_device_delivery::snapshot(
            State(f.base.state.clone()),
            Extension(f.device.clone()),
            Json(request.clone())
        )
        .await
        .is_err()
    );
    // Owner-side access removed while the signed gateway response is in flight
    // prevents those bytes being returned to the company.
    *f.fake.hook.lock().unwrap() = Some(Hook::Revoke(f.base.membership));
    let mut next = request.clone();
    next.snapshot.nonce = Uuid::new_v4();
    assert!(
        super::super::company_device_delivery::snapshot(
            State(f.base.state.clone()),
            Extension(delegate),
            Json(next)
        )
        .await
        .is_err()
    );
    f.close().await;
}

#[derive(Clone)]
struct FakeGateway {
    store: GatewayStore,
    pool: sqlx::PgPool,
    fence: WriterFence,
    owner: InstanceId,
    space: SpaceId,
    security: Arc<GatewaySecurity>,
    seen: Arc<Mutex<Vec<ProjectFeedRegistration>>>,
    hook: Arc<Mutex<Option<Hook>>>,
}
enum Hook {
    LoseRegistration,
    FailConfirmation,
    Revoke(Uuid),
    Withdraw(Uuid),
    ExpirePage,
}

#[tokio::test]
async fn composite_local_route_rejects_other_owner_area_without_fallback() {
    let Some(f) = DeliveryFixture::new(118).await else {
        return;
    };
    let local_owner = f.fake.owner.as_uuid();
    let response = scoped_subscription(
        State(f.base.state.clone()),
        Extension(f.device.clone()),
        Path(("workshop".into(), local_owner, f.base.project)),
    )
    .await
    .unwrap();
    let bootstrap: mem_federation_runtime::DeviceProjectSubscription = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bootstrap.scope.owner.as_uuid(), local_owner);
    for (area, owner) in [
        ("homelab", local_owner),
        ("trust", local_owner),
        ("workshop", Uuid::new_v4()),
    ] {
        assert!(
            scoped_subscription(
                State(f.base.state.clone()),
                Extension(f.device.clone()),
                Path((area.into(), owner, f.base.project))
            )
            .await
            .is_err()
        );
    }
    assert!(f.fake.seen.lock().unwrap().is_empty());
    f.close().await;
}
async fn register(
    State(fake): State<FakeGateway>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ProjectFeedRegistration>,
) -> Response {
    assert_eq!(
        headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer synthetic-owner-token-that-is-not-a-device-token"
    );
    fake.seen.lock().unwrap().push(request.clone());
    let receipt = fake
        .store
        .register_project(fake.fence, fake.owner, &request)
        .await
        .unwrap();
    let lose = matches!(
        fake.hook.lock().unwrap().as_ref(),
        Some(Hook::LoseRegistration)
    );
    if lose {
        fake.hook.lock().unwrap().take();
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let fail = matches!(
        fake.hook.lock().unwrap().as_ref(),
        Some(Hook::FailConfirmation)
    );
    if fail {
        fake.hook.lock().unwrap().take();
        sqlx::query("CREATE FUNCTION synthetic_owner_confirmation_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic confirmation unavailable'; END $$").execute(&fake.pool).await.unwrap();
        sqlx::query("CREATE TRIGGER synthetic_owner_confirmation_fail BEFORE UPDATE ON onboarding_owner_project_operations FOR EACH ROW EXECUTE FUNCTION synthetic_owner_confirmation_fail()").execute(&fake.pool).await.unwrap();
    }
    Json(
        fake.security
            .sign_owner_project_binding(request, receipt)
            .unwrap(),
    )
    .into_response()
}
async fn private_replica(
    State(fake): State<FakeGateway>,
    headers: axum::http::HeaderMap,
    Json(page): Json<mem_federation::device_replica::DeviceReplicaPage>,
) -> Response {
    assert_eq!(
        headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer synthetic-owner-token-that-is-not-a-device-token"
    );
    let authority = fake
        .store
        .authorize_device_replica_attestation(fake.fence, fake.owner, &page)
        .await
        .unwrap();
    Json(
        fake.security
            .sign_device_replica_page(authority, page)
            .unwrap(),
    )
    .into_response()
}
async fn page(
    State(fake): State<FakeGateway>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ProjectPageRequest>,
) -> Response {
    assert_eq!(
        headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer synthetic-owner-token-that-is-not-a-device-token"
    );
    let page = match fake
        .store
        .project_page(fake.fence, fake.owner, &request)
        .await
    {
        Ok(v) => v,
        Err(_) => return StatusCode::CONFLICT.into_response(),
    };
    let hook = fake.hook.lock().unwrap().take();
    match hook {
        Some(Hook::Revoke(id)) => {
            sqlx::query("DELETE FROM auth_project_memberships WHERE id=$1")
                .bind(id)
                .execute(&fake.pool)
                .await
                .unwrap();
        }
        Some(Hook::Withdraw(id)) => {
            sqlx::query(
                "UPDATE federation_source_subscriptions SET enabled=false WHERE canonical_id=$1",
            )
            .bind(id)
            .execute(&fake.pool)
            .await
            .unwrap();
        }
        _ => {}
    }
    let mut signed = fake.security.sign_project_page(page).unwrap();
    if matches!(hook, Some(Hook::ExpirePage)) {
        signed.claims.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
    }
    Json(signed).into_response()
}
struct DeliveryFixture {
    base: super::super::device::tests::Fixture,
    fake: FakeGateway,
    sync: InstanceSync,
    device: AuthenticatedPrincipal,
    server: tokio::task::JoinHandle<()>,
}
impl DeliveryFixture {
    async fn new(n: u8) -> Option<Self> {
        let mut base = super::super::device::tests::fixture().await?;
        let (_, token, _) = super::super::device::tests::enroll(&base, n, AuthRole::Writer).await;
        let device =
            super::super::resolve_principal(&base.state, super::super::Credential::ApiToken(token))
                .await
                .unwrap()
                .unwrap();
        base.state.config.auth.company_onboarding =
            Some(serde_json::from_value(json!({"company_id":Uuid::new_v4(),"owners":[]})).unwrap());
        let owner = InstanceId::new(base.state.config.area.as_ref().unwrap().instance_id).unwrap();
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = InstanceId::new(Uuid::new_v4()).unwrap();
        let security =
            Arc::new(GatewaySecurity::initial(&[23; 32], SpacePolicy::default()).unwrap());
        let store = GatewayStore::new(base.db.pool.clone());
        let old = store.initialize(space, authority).await.unwrap();
        let fence = store
            .enroll_security(old, security.trust().keys(), &SpacePolicy::default())
            .await
            .unwrap();
        let fake = FakeGateway {
            store,
            pool: base.db.pool.clone(),
            fence,
            owner,
            space,
            security,
            seen: Default::default(),
            hook: Default::default(),
        };
        for capability in [SpaceCapability::Publish, SpaceCapability::Pull] {
            publish(
                &fake,
                authority,
                PublicationRecord::Membership {
                    member: owner,
                    capability,
                },
            )
            .await;
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let router = axum::Router::new()
            .route(
                "/v1/projects/device-replica/attest",
                axum::routing::post(private_replica),
            )
            .route("/v1/projects/register", axum::routing::post(register))
            .route("/v1/projects/page", axum::routing::post(page))
            .with_state(fake.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = GatewayClient::new(
            Endpoint::new(&origin, true).unwrap(),
            "synthetic-owner-token-that-is-not-a-device-token".into(),
            space,
            authority,
            fake.security.trust(),
        )
        .unwrap();
        let sync = InstanceSync::new(base.db.pool.clone(), client, owner)
            .await
            .unwrap();
        let (records, receipts) = fake.store.audited_history(fence).await.unwrap();
        sync.replica()
            .receive_with_receipts(authority, &records, &receipts)
            .await
            .unwrap();
        *base.state.federation.write().unwrap() = Some(Arc::new(
            crate::federation::ConfiguredInstance::synthetic_project_delivery(
                sync.clone(),
                owner,
                space,
                authority,
                base.project,
            ),
        ));
        Some(Self {
            base,
            fake,
            sync,
            device,
            server,
        })
    }
    async fn request(
        &self,
        request: OnboardingProjectSnapshotRequest,
    ) -> Result<Response, ApiError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            snapshot(
                State(self.base.state.clone()),
                Extension(self.device.clone()),
                Path(self.base.project),
                Json(request),
            ),
        )
        .await
        .expect("SQL locks must not span gateway HTTP")
    }
    async fn share(&self) -> Uuid {
        let pool = &self.base.db.pool;
        sqlx::query("UPDATE auth_project_memberships SET role='operator' WHERE id=$1")
            .bind(self.base.membership)
            .execute(pool)
            .await
            .unwrap();
        let canonical = Uuid::new_v4();
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,1,false,'approved scoped knowledge','approved summary','decision','project',3,0.8,now(),now(),to_tsvector('approved'))")
            .bind(id).bind(self.base.project).bind(canonical).execute(pool).await.unwrap();
        let access = LocalProjectAccess {
            principal: self.base.human.id,
            project: self.base.project,
            permissions: AuthRole::Operator.permissions(),
            read_only: false,
        };
        let known = self.sync.verified_snapshot().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        mem_federation_store::set_local_grant(
            &mut tx,
            &LocalProjectAccess {
                permissions: access
                    .permissions
                    .union(PermissionSet::from_permissions(&[Permission::AuthManage])),
                ..access.clone()
            },
            &LocalSpaceGrant {
                principal: access.principal,
                project: access.project,
                space: self.fake.space,
                instance: self.fake.owner,
                capabilities: vec![SpaceCapability::Pull, SpaceCapability::Publish],
            },
            true,
        )
        .await
        .unwrap();
        let expected = mem_federation_store::preview_source(
            pool,
            self.base.project,
            RecordId::new(canonical).unwrap(),
        )
        .await
        .unwrap();
        mem_federation_store::review_and_share(
            &mut tx,
            &known,
            &access,
            self.fake.owner,
            RecordId::new(canonical).unwrap(),
            &expected,
            &[],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        mem_federation_store::prepare_source_changes(pool, &known, &access, self.fake.owner, 32)
            .await
            .unwrap();
        for intent in
            mem_federation_store::pending_intents(pool, self.fake.space, self.fake.owner, 32)
                .await
                .unwrap()
        {
            let delivery = publish_intent(&self.fake, self.fake.owner, &intent).await;
            let mut tx = pool.begin().await.unwrap();
            mem_federation_store::record_acceptance(&mut tx, &delivery)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let (records, receipts) = self
            .fake
            .store
            .audited_history(self.fake.fence)
            .await
            .unwrap();
        self.sync
            .replica()
            .receive_with_receipts(
                self.sync.verified_snapshot().await.unwrap().authority(),
                &records,
                &receipts,
            )
            .await
            .unwrap();
        canonical
    }
    async fn close(self) {
        self.server.abort();
        self.base.db.cleanup().await.unwrap();
    }
}
fn cursor() -> OnboardingProjectSnapshotRequest {
    OnboardingProjectSnapshotRequest {
        nonce: Uuid::new_v4(),
        after: 0,
        through: None,
        limit: 32,
    }
}
async fn publish(
    fake: &FakeGateway,
    publisher: InstanceId,
    record: PublicationRecord,
) -> PublicationDelivery {
    let intent = PublicationIntent {
        locator: RecordLocator::new(format!("synthetic:{}", Uuid::new_v4())).unwrap(),
        envelope: PublicationEnvelope {
            schema_version: 2,
            space: fake.space,
            publisher,
            record_id: RecordId::new(Uuid::new_v4()).unwrap(),
            author: AuthorIdentity::new("synthetic-author").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record,
        },
    };
    publish_intent(fake, publisher, &intent).await
}
async fn publish_intent(
    fake: &FakeGateway,
    publisher: InstanceId,
    intent: &PublicationIntent,
) -> PublicationDelivery {
    let acceptance = fake
        .store
        .accept(fake.fence, publisher, intent)
        .await
        .unwrap();
    assert_eq!(acceptance.outcome, PublicationOutcome::Applied);
    let Claim::Ready(lease) = fake
        .store
        .claim_publication(fake.fence, RetryPolicy::default())
        .await
        .unwrap()
    else {
        panic!("ready publication")
    };
    fake.store
        .acknowledge_with_receipt(
            &lease,
            json!({"synthetic_verified":true}),
            fake.security.as_ref(),
        )
        .await
        .unwrap();
    acceptance.delivery.unwrap()
}

#[tokio::test]
async fn empty_snapshot_exact_nonce_and_lost_registration_restart() {
    let Some(f) = DeliveryFixture::new(81).await else {
        return;
    };
    *f.fake.hook.lock().unwrap() = Some(Hook::LoseRegistration);
    let request = cursor();
    assert_eq!(
        f.request(request.clone()).await.unwrap_err().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let operation = f.fake.seen.lock().unwrap()[0].operation_id;
    let response = f.request(request.clone()).await.unwrap();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let page: SignedProjectPage = serde_json::from_slice(&bytes).unwrap();
    assert!(page.claims.page.entries.is_empty());
    let seen = f.fake.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|r| r.operation_id == operation));
    let mut altered = request;
    altered.limit = 1;
    assert_eq!(
        f.request(altered).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    f.close().await;
}

#[tokio::test]
async fn approved_source_cursor_and_withdrawal_disable_prior_stream() {
    let Some(f) = DeliveryFixture::new(82).await else {
        return;
    };
    let canonical = f.share().await;
    let denied_canonical = RecordId::new(Uuid::new_v4()).unwrap();
    let denied = publish(
        &f.fake,
        f.fake.owner,
        PublicationRecord::MemoryVersion {
            stream: StreamId {
                origin: f.fake.owner,
                canonical: denied_canonical,
            },
            publication: MemoryPublication {
                version: SharedVersion::new(1).unwrap(),
                source: SourceVersionRef {
                    record_id: RecordId::new(Uuid::new_v4()).unwrap(),
                    version: SourceVersion::new(1).unwrap(),
                },
                content: MemoryContent {
                    text: "DENIED_SENTINEL_OTHER_PUBLISHED_PROJECT".into(),
                    summary: "denied".into(),
                    memory_type: MemoryType::Decision,
                    confidence: ConfidencePpm::from_confidence(0.8).unwrap(),
                    importance: 3,
                    tags: vec![],
                },
                evidence: vec![],
            },
        },
    )
    .await;
    let (records, receipts) = f.fake.store.audited_history(f.fake.fence).await.unwrap();
    f.sync
        .replica()
        .receive_with_receipts(
            f.sync.verified_snapshot().await.unwrap().authority(),
            &records,
            &receipts,
        )
        .await
        .unwrap();
    let request = cursor();
    let response = f.request(request).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let page: SignedProjectPage = serde_json::from_slice(&bytes).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("approved scoped knowledge"));
    assert!(!String::from_utf8_lossy(&bytes).contains("DENIED_SENTINEL"));
    let transmitted = serde_json::to_string(&*f.fake.seen.lock().unwrap()).unwrap();
    assert!(!transmitted.contains(&denied.envelope.record_id.as_uuid().to_string()));
    assert!(!transmitted.contains(&denied_canonical.as_uuid().to_string()));
    assert!(page.claims.page.next > 0);
    let mut next = cursor();
    next.after = page.claims.page.next;
    next.through = Some(page.claims.page.through);
    let response = f.request(next).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let page: SignedProjectPage = serde_json::from_slice(&bytes).unwrap();
    assert!(page.claims.page.entries.is_empty());
    *f.fake.hook.lock().unwrap() = Some(Hook::Withdraw(canonical));
    assert_eq!(
        f.request(cursor()).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    let response = f.request(cursor()).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("approved scoped knowledge"));
    assert!(
        f.fake
            .seen
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .reviews
            .iter()
            .any(|r| r.stream.canonical.as_uuid() == canonical
                && !r.enabled
                && r.records.is_empty())
    );
    f.close().await;
}

#[tokio::test]
async fn authority_revoke_during_gateway_and_denied_sentinel_never_transmitted() {
    let Some(f) = DeliveryFixture::new(83).await else {
        return;
    };
    f.share().await;
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'DENIED_SENTINEL_RAW_CAPTURE','denied','decision','project',3,0.8,now(),now(),to_tsvector('denied'))")
        .bind(Uuid::new_v4()).bind(f.base.project).execute(&f.base.db.pool).await.unwrap();
    *f.fake.hook.lock().unwrap() = Some(Hook::Revoke(f.base.membership));
    assert_eq!(
        f.request(cursor()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    let seen = serde_json::to_string(&*f.fake.seen.lock().unwrap()).unwrap();
    assert!(!seen.contains("DENIED_SENTINEL"));
    assert!(!seen.contains("approved scoped knowledge"));
    let count = f.fake.seen.lock().unwrap().len();
    assert_eq!(
        f.request(cursor()).await.unwrap_err().status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.fake.seen.lock().unwrap().len(), count);
    f.close().await;
}

#[tokio::test]
async fn expired_gateway_page_and_saved_catalog_revision_fail_closed() {
    let Some(f) = DeliveryFixture::new(84).await else {
        return;
    };
    f.share().await;
    *f.fake.hook.lock().unwrap() = Some(Hook::ExpirePage);
    assert_eq!(
        f.request(cursor()).await.unwrap_err().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    sqlx::query("INSERT INTO onboarding_project_versions(project_id,revision,area,owner_instance_id,offline_allowed,entry_json,reviewed_by) SELECT project_id,2,area,owner_instance_id,false,jsonb_set(entry_json,'{offline_allowed}','false'),reviewed_by FROM onboarding_project_versions WHERE project_id=$1 AND revision=1")
        .bind(f.base.project).execute(&f.base.db.pool).await.unwrap();
    let count = f.fake.seen.lock().unwrap().len();
    assert_eq!(
        f.request(cursor()).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    assert_eq!(f.fake.seen.lock().unwrap().len(), count);
    f.close().await;
}

#[tokio::test]
async fn subscription_bootstrap_is_public_trust_but_exact_device_authority() {
    let Some(f) = DeliveryFixture::new(85).await else {
        return;
    };
    let Json(value) = subscription(
        State(f.base.state.clone()),
        Extension(f.device.clone()),
        Path(f.base.project),
    )
    .await
    .unwrap();
    value.validate().unwrap();
    assert_eq!(value.schema_version, 1);
    assert_eq!(value.scope.project, f.base.project);
    assert_eq!(value.scope.owner, f.fake.owner);
    assert_eq!(value.receipt_keys, f.fake.security.trust().keys());
    assert_eq!(value.catalog_revision, 1);
    let encoded = serde_json::to_string(&value).unwrap();
    assert!(!encoded.contains("synthetic-owner-token"));
    assert!(!encoded.contains("endpoint"));
    assert!(!encoded.contains("database"));
    assert!(f.fake.seen.lock().unwrap().is_empty());
    assert_eq!(
        subscription(
            State(f.base.state.clone()),
            Extension(f.base.human.clone()),
            Path(f.base.project)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    let mut wrong = f.device.clone();
    wrong.token_id = Some(Uuid::new_v4());
    assert_eq!(
        subscription(
            State(f.base.state.clone()),
            Extension(wrong),
            Path(f.base.project)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    let mut wrong = f.device.clone();
    wrong.id = Uuid::new_v4();
    assert_eq!(
        subscription(
            State(f.base.state.clone()),
            Extension(wrong),
            Path(f.base.project)
        )
        .await
        .unwrap_err()
        .status,
        StatusCode::FORBIDDEN
    );
    f.close().await;
}

#[tokio::test]
async fn post_gateway_confirmation_failure_retains_exact_durable_intent() {
    let Some(f) = DeliveryFixture::new(86).await else {
        return;
    };
    *f.fake.hook.lock().unwrap() = Some(Hook::FailConfirmation);
    let request = cursor();
    let error = f.request(request.clone()).await.unwrap_err();
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
    let first = f.fake.seen.lock().unwrap()[0].clone();
    assert!(error.message.contains(&first.operation_id.to_string()));
    let pending: Uuid = sqlx::query_scalar(
        "SELECT pending_operation FROM onboarding_owner_project_heads WHERE project_id=$1",
    )
    .bind(f.base.project)
    .fetch_one(&f.base.db.pool)
    .await
    .unwrap();
    assert_eq!(pending, first.operation_id);
    sqlx::query(
        "DROP TRIGGER synthetic_owner_confirmation_fail ON onboarding_owner_project_operations",
    )
    .execute(&f.base.db.pool)
    .await
    .unwrap();
    let response = f.request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        f.fake.seen.lock().unwrap().as_slice(),
        &[first.clone(), first]
    );
    f.close().await;
}

#[test]
fn removed_streams_are_explicitly_disabled_and_retained() {
    let owner = InstanceId::new(Uuid::new_v4()).unwrap();
    let a = ProjectStreamReview {
        stream: StreamId {
            origin: owner,
            canonical: RecordId::new(Uuid::new_v4()).unwrap(),
        },
        review_id: Uuid::new_v4(),
        records: vec![],
        enabled: true,
    };
    let mut b = a.clone();
    b.stream.canonical = RecordId::new(Uuid::new_v4()).unwrap();
    let complete = complete_reviews(std::slice::from_ref(&a), std::slice::from_ref(&b)).unwrap();
    assert_eq!(complete.len(), 2);
    assert!(
        complete
            .iter()
            .any(|r| r.stream == a.stream && !r.enabled && r.records.is_empty())
    );
    assert!(complete.contains(&b));
    let empty = complete_reviews(&complete, &[]).unwrap();
    assert!(empty.iter().all(|r| !r.enabled && r.records.is_empty()));
}

#[test]
fn page_reviews_and_withheld_events_require_exact_registered_bindings() {
    let owner = InstanceId::new(Uuid::new_v4()).unwrap();
    let review = ProjectStreamReview {
        stream: StreamId {
            origin: owner,
            canonical: RecordId::new(Uuid::new_v4()).unwrap(),
        },
        review_id: Uuid::new_v4(),
        records: vec![],
        enabled: false,
    };
    let sources = CurrentProjectSources {
        reviews: vec![],
        records: BTreeMap::new(),
        fingerprint: mem_federation::ContentDigest::sha256_bytes(b"empty"),
    };
    let entry = mem_federation_store::ProjectEntry {
        sequence: 1,
        event: ProjectEvent::Review {
            review: review.clone(),
        },
    };
    assert!(
        validate_entries(
            &sources,
            std::slice::from_ref(&review),
            std::slice::from_ref(&entry)
        )
        .is_ok()
    );
    let mut changed_review = review.clone();
    changed_review.review_id = Uuid::new_v4();
    assert!(validate_entries(&sources, &[changed_review], &[entry]).is_err());
    let unknown = ProjectEvent::Withheld {
        stream: StreamId {
            origin: owner,
            canonical: RecordId::new(Uuid::new_v4()).unwrap(),
        },
    };
    assert!(
        validate_entries(
            &sources,
            &[review],
            &[mem_federation_store::ProjectEntry {
                sequence: 1,
                event: unknown
            }]
        )
        .is_err()
    );
}
