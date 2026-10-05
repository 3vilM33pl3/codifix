// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use mem_federation::{
    AuthorIdentity, RecordLocator, SpaceCapability, publication::*, subscription::*,
};
use mem_federation_store::*;
use mem_record::{MemoryStatus, MemoryType};
use sqlx::PgPool;
use uuid::Uuid;

struct Signer;
impl ReceiptAuthority for Signer {
    fn generation(&self) -> u32 {
        1
    }
    fn public_key(&self) -> String {
        "synthetic-store-verifier".into()
    }
    fn sign(&self, claims: ReceiptClaims) -> Result<AuditReceipt> {
        Ok(AuditReceipt {
            claims,
            signature: vec![42; 64],
        })
    }
}
struct Fixture {
    db: mem_test_support::IsolatedDatabase,
    pool: PgPool,
    store: GatewayStore,
    fence: WriterFence,
    owner: InstanceId,
    authority: InstanceId,
    space: SpaceId,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = mem_test_support::IsolatedDatabase::new().await?;
        let pool = db.pool.clone();
        let store = GatewayStore::new(pool.clone());
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = instance();
        let owner = instance();
        let old = store.initialize(space, authority).await.unwrap();
        let fence = store
            .enroll_security(
                old,
                &[ReceiptKey {
                    generation: 1,
                    first_sequence: 1,
                    public_key: Signer.public_key(),
                }],
                &SpacePolicy::default(),
            )
            .await
            .unwrap();
        let f = Self {
            db,
            pool,
            store,
            fence,
            owner,
            authority,
            space,
        };
        f.enroll(owner).await;
        Some(f)
    }
    async fn enroll(&self, member: InstanceId) {
        for capability in [SpaceCapability::Publish, SpaceCapability::Pull] {
            self.publish(
                self.authority,
                PublicationRecord::Membership { member, capability },
            )
            .await;
        }
    }
    async fn publish(
        &self,
        publisher: InstanceId,
        record: PublicationRecord,
    ) -> PublicationDelivery {
        let intent = PublicationIntent {
            locator: RecordLocator::new(format!("record:{}", Uuid::new_v4())).unwrap(),
            envelope: PublicationEnvelope {
                schema_version: 2,
                space: self.space,
                publisher,
                record_id: id(),
                author: AuthorIdentity::new("original-expert").unwrap(),
                authored_at: chrono::Utc::now(),
                causation: None,
                record,
            },
        };
        let a = self
            .store
            .accept(self.fence, publisher, &intent)
            .await
            .unwrap();
        assert_eq!(a.outcome, PublicationOutcome::Applied);
        let Claim::Ready(lease) = self
            .store
            .claim_publication(self.fence, RetryPolicy::default())
            .await
            .unwrap()
        else {
            panic!("claim");
        };
        self.store
            .acknowledge_with_receipt(
                &lease,
                serde_json::json!({"synthetic_verified_transport":true}),
                &Signer,
            )
            .await
            .unwrap();
        a.delivery.unwrap()
    }
    async fn stream(
        &self,
        owner: InstanceId,
        canonical: RecordId,
        text: &str,
    ) -> (StreamId, Vec<RecordKey>) {
        let stream = StreamId {
            origin: owner,
            canonical,
        };
        let memory = self
            .publish(
                owner,
                PublicationRecord::MemoryVersion {
                    stream,
                    publication: MemoryPublication {
                        version: SharedVersion::new(1).unwrap(),
                        source: SourceVersionRef {
                            record_id: id(),
                            version: SourceVersion::new(3).unwrap(),
                        },
                        content: MemoryContent {
                            text: text.into(),
                            summary: text.into(),
                            memory_type: MemoryType::Decision,
                            confidence: ConfidencePpm::try_from(800_000).unwrap(),
                            importance: 3,
                            tags: vec!["approved".into()],
                        },
                        evidence: vec![],
                    },
                },
            )
            .await;
        let state = self
            .publish(
                owner,
                PublicationRecord::CanonicalState {
                    stream,
                    state: StatePublication {
                        revision: StateRevision::new(1).unwrap(),
                        content_version: SharedVersion::new(1).unwrap(),
                        status: MemoryStatus::Active,
                        confidence: ConfidencePpm::try_from(800_000).unwrap(),
                        tombstone: false,
                    },
                },
            )
            .await;
        (stream, vec![memory.envelope.key(), state.envelope.key()])
    }
    fn registration(
        &self,
        project: Uuid,
        stream: StreamId,
        records: Vec<RecordKey>,
    ) -> ProjectFeedRegistration {
        ProjectFeedRegistration {
            operation_id: Uuid::new_v4(),
            scope: ProjectScope {
                company: Uuid::new_v4(),
                area: "workshop".into(),
                owner: stream.origin,
                project,
                space: self.space,
            },
            expected_revision: 0,
            catalog_revision: 1,
            policy_revision: 1,
            reviews: vec![ProjectStreamReview {
                stream,
                review_id: Uuid::new_v4(),
                records,
                enabled: true,
            }],
        }
    }
    fn page(
        &self,
        r: &ProjectFeedRegistration,
        revision: u64,
        after: u64,
        limit: u32,
    ) -> ProjectPageRequest {
        ProjectPageRequest {
            nonce: Uuid::new_v4(),
            device: Uuid::new_v4(),
            scope: r.scope.clone(),
            catalog_revision: r.catalog_revision,
            policy_revision: r.policy_revision,
            binding_revision: revision,
            after,
            through: None,
            limit,
        }
    }
}
fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn id() -> RecordId {
    RecordId::new(Uuid::new_v4()).unwrap()
}

#[tokio::test]
async fn scoped_delivery_never_transmits_unselected_or_withdrawn_project_bytes() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (a, ak) = f.stream(f.owner, id(), "APPROVED_WORKSHOP_SENTINEL").await;
    let (b, bk) = f.stream(f.owner, id(), "DENIED_PROJECT_SENTINEL").await;
    let r = f.registration(Uuid::new_v4(), a, ak);
    let other = f.registration(Uuid::new_v4(), b, bk);
    let result = f
        .store
        .register_project(f.fence, f.owner, &r)
        .await
        .unwrap();
    f.store
        .register_project(f.fence, f.owner, &other)
        .await
        .unwrap();
    assert_eq!(
        result,
        f.store
            .register_project(f.fence, f.owner, &r)
            .await
            .unwrap()
    );
    let request = f.page(&r, result.revision, 0, 32);
    let page = f
        .store
        .project_page(f.fence, f.owner, &request)
        .await
        .unwrap();
    let wire = serde_json::to_string(&page).unwrap();
    assert!(wire.contains("APPROVED_WORKSHOP_SENTINEL"));
    assert!(!wire.contains("DENIED_PROJECT_SENTINEL"));
    let mut altered = r.clone();
    altered.reviews[0].review_id = Uuid::new_v4();
    assert!(matches!(
        f.store.register_project(f.fence, f.owner, &altered).await,
        Err(StoreError::Binding)
    ));
    let mut claimed = r.clone();
    claimed.operation_id = Uuid::new_v4();
    claimed.scope.project = Uuid::new_v4();
    assert!(matches!(
        f.store.register_project(f.fence, f.owner, &claimed).await,
        Err(StoreError::Binding)
    ));
    let mut withdrawn = r.clone();
    withdrawn.operation_id = Uuid::new_v4();
    withdrawn.expected_revision = 1;
    withdrawn.reviews[0].enabled = false;
    withdrawn.reviews[0].review_id = Uuid::new_v4();
    let next = f
        .store
        .register_project(f.fence, f.owner, &withdrawn)
        .await
        .unwrap();
    assert!(matches!(
        f.store.project_page(f.fence, f.owner, &request).await,
        Err(StoreError::Resync)
    ));
    let page = f
        .store
        .project_page(f.fence, f.owner, &f.page(&withdrawn, next.revision, 0, 32))
        .await
        .unwrap();
    let wire = serde_json::to_string(&page).unwrap();
    assert!(!wire.contains("APPROVED_WORKSHOP_SENTINEL"));
    assert!(!wire.contains("DENIED_PROJECT_SENTINEL"));
    assert!(
        page.entries
            .iter()
            .any(|e| matches!(&e.event,ProjectEvent::Review{review} if !review.enabled))
    );
    let mut trust = f.page(&r, 1, 0, 32);
    trust.scope.area = "trust".into();
    assert!(matches!(
        f.store.project_page(f.fence, f.owner, &trust).await,
        Err(StoreError::Binding)
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn read_only_projection_replay_cursor_and_failures_are_atomic() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (stream, keys) = f
        .stream(f.owner, id(), "offline searchable approved knowledge")
        .await;
    let r = f.registration(Uuid::new_v4(), stream, keys);
    let receipt = f
        .store
        .register_project(f.fence, f.owner, &r)
        .await
        .unwrap();
    let request = f.page(&r, receipt.revision, 0, 32);
    let projection = ProjectProjectionStore::initialize(
        f.pool.clone(),
        Uuid::new_v4(),
        request.device,
        r.scope.clone(),
    )
    .await
    .unwrap();
    let page = f
        .store
        .project_page(f.fence, f.owner, &request)
        .await
        .unwrap();
    assert_eq!(projection.apply_verified(&page).await.unwrap(), 3);
    assert_eq!(projection.apply_verified(&page).await.unwrap(), 3);
    let hits = projection.search("offline knowledge", 5).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].read_only);
    assert_eq!(hits[0].author.as_str(), "original-expert");
    let mut changed = page.clone();
    changed.entries[0].event = ProjectEvent::Withheld { stream };
    assert!(matches!(
        projection.apply_verified(&changed).await,
        Err(StoreError::Binding)
    ));
    assert_eq!(projection.cursor().await.unwrap(), 3);
    let mut withdraw = r.clone();
    withdraw.operation_id = Uuid::new_v4();
    withdraw.expected_revision = 1;
    withdraw.reviews[0].enabled = false;
    let current = f
        .store
        .register_project(f.fence, f.owner, &withdraw)
        .await
        .unwrap();
    let mut request = f.page(&withdraw, current.revision, 3, 32);
    request.device = page.request.device;
    let page = f
        .store
        .project_page(f.fence, f.owner, &request)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_project_cursor() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic cursor fault'; END$$; CREATE TRIGGER fail_project_cursor BEFORE UPDATE ON onboarding_project_projections FOR EACH ROW EXECUTE FUNCTION fail_project_cursor();")
        .execute(&f.pool).await.unwrap();
    assert!(projection.apply_verified(&page).await.is_err());
    assert_eq!(projection.cursor().await.unwrap(), 3);
    assert_eq!(projection.search("offline", 5).await.unwrap().len(), 1);
    sqlx::query("DROP TRIGGER fail_project_cursor ON onboarding_project_projections")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(projection.apply_verified(&page).await.unwrap(), 4);
    assert!(projection.search("offline", 5).await.unwrap().is_empty());
    let mut wrong = page.clone();
    wrong.request.device = Uuid::new_v4();
    wrong.request.nonce = Uuid::new_v4();
    assert!(matches!(
        projection.apply_verified(&wrong).await,
        Err(StoreError::Binding)
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn named_projects_and_canonical_ids_at_independent_origins_remain_distinct() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let second = instance();
    f.enroll(second).await;
    let canonical = id();
    let project = Uuid::new_v4();
    let (a, ak) = f.stream(f.owner, canonical, "first owner").await;
    let (b, bk) = f.stream(second, canonical, "second owner").await;
    let ra = f.registration(project, a, ak);
    let rb = f.registration(project, b, bk);
    let ca = f
        .store
        .register_project(f.fence, f.owner, &ra)
        .await
        .unwrap();
    let cb = f
        .store
        .register_project(f.fence, second, &rb)
        .await
        .unwrap();
    let pa = f
        .store
        .project_page(f.fence, f.owner, &f.page(&ra, ca.revision, 0, 32))
        .await
        .unwrap();
    let pb = f
        .store
        .project_page(f.fence, second, &f.page(&rb, cb.revision, 0, 32))
        .await
        .unwrap();
    assert!(!serde_json::to_string(&pa).unwrap().contains("second owner"));
    assert!(!serde_json::to_string(&pb).unwrap().contains("first owner"));
    assert!(matches!(
        f.store.project_page(f.fence, second, &pa.request).await,
        Err(StoreError::Binding)
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn registration_rollback_current_revocation_and_cursor_gaps_fail_closed() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (stream, keys) = f.stream(f.owner, id(), "approved").await;
    let r = f.registration(Uuid::new_v4(), stream, keys);
    sqlx::raw_sql("CREATE FUNCTION fail_project_event() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'synthetic event fault'; END$$; CREATE TRIGGER fail_project_event BEFORE INSERT ON federation_project_events FOR EACH ROW EXECUTE FUNCTION fail_project_event();")
        .execute(&f.pool).await.unwrap();
    assert!(
        f.store
            .register_project(f.fence, f.owner, &r)
            .await
            .is_err()
    );
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_project_feeds WHERE project=$1")
            .bind(r.scope.project)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(n, 0);
    sqlx::query("DROP TRIGGER fail_project_event ON federation_project_events")
        .execute(&f.pool)
        .await
        .unwrap();
    let receipt = f
        .store
        .register_project(f.fence, f.owner, &r)
        .await
        .unwrap();
    let mut request = f.page(&r, receipt.revision, 100, 32);
    assert!(matches!(
        f.store.project_page(f.fence, f.owner, &request).await,
        Err(StoreError::Resync)
    ));
    request.after = 0;
    f.publish(
        f.authority,
        PublicationRecord::Revocation {
            member: f.owner,
            capability: Some(SpaceCapability::Pull),
        },
    )
    .await;
    assert!(matches!(
        f.store.project_page(f.fence, f.owner, &request).await,
        Err(StoreError::Binding)
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn archived_state_and_tombstones_are_never_searchable_offline() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let (stream, mut keys) = f.stream(f.owner, id(), "active searchable diagnosis").await;
    let mut r = f.registration(Uuid::new_v4(), stream, keys.clone());
    let receipt = f
        .store
        .register_project(f.fence, f.owner, &r)
        .await
        .unwrap();
    let request = f.page(&r, receipt.revision, 0, 32);
    let projection = ProjectProjectionStore::initialize(
        f.pool.clone(),
        Uuid::new_v4(),
        request.device,
        r.scope.clone(),
    )
    .await
    .unwrap();
    let page = f
        .store
        .project_page(f.fence, f.owner, &request)
        .await
        .unwrap();
    projection.apply_verified(&page).await.unwrap();
    assert_eq!(projection.search("diagnosis", 5).await.unwrap().len(), 1);
    for (revision, status, tombstone) in [
        (2, MemoryStatus::Archived, false),
        (3, MemoryStatus::Active, true),
    ] {
        let state = f
            .publish(
                f.owner,
                PublicationRecord::CanonicalState {
                    stream,
                    state: StatePublication {
                        revision: StateRevision::new(revision).unwrap(),
                        content_version: SharedVersion::new(1).unwrap(),
                        status,
                        confidence: ConfidencePpm::try_from(800_000).unwrap(),
                        tombstone,
                    },
                },
            )
            .await;
        keys.push(state.envelope.key());
        r.operation_id = Uuid::new_v4();
        r.expected_revision = revision as u64 - 1;
        r.reviews[0].records = keys.clone();
        r.reviews[0].review_id = Uuid::new_v4();
        let receipt = f
            .store
            .register_project(f.fence, f.owner, &r)
            .await
            .unwrap();
        let mut request = f.page(&r, receipt.revision, projection.cursor().await.unwrap(), 32);
        request.device = page.request.device;
        let delta = f
            .store
            .project_page(f.fence, f.owner, &request)
            .await
            .unwrap();
        projection.apply_verified(&delta).await.unwrap();
        assert!(projection.search("diagnosis", 5).await.unwrap().is_empty());
    }
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn an_authorized_empty_project_initializes_without_invented_knowledge() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let r = ProjectFeedRegistration {
        operation_id: Uuid::new_v4(),
        scope: ProjectScope {
            company: Uuid::new_v4(),
            area: "general".into(),
            owner: f.owner,
            project: Uuid::new_v4(),
            space: f.space,
        },
        expected_revision: 0,
        catalog_revision: 1,
        policy_revision: 1,
        reviews: vec![],
    };
    let receipt = f
        .store
        .register_project(f.fence, f.owner, &r)
        .await
        .unwrap();
    assert_eq!(receipt.head, 0);
    let request = f.page(&r, receipt.revision, 0, 32);
    let page = f
        .store
        .project_page(f.fence, f.owner, &request)
        .await
        .unwrap();
    assert!(page.entries.is_empty());
    assert_eq!(page.through, 0);
    assert_eq!(page.next, 0);
    let projection = ProjectProjectionStore::initialize(
        f.pool.clone(),
        Uuid::new_v4(),
        request.device,
        r.scope.clone(),
    )
    .await
    .unwrap();
    assert_eq!(projection.apply_verified(&page).await.unwrap(), 0);
    assert_eq!(projection.apply_verified(&page).await.unwrap(), 0);
    assert!(projection.search("anything", 5).await.unwrap().is_empty());
    f.db.cleanup().await.unwrap();
}
