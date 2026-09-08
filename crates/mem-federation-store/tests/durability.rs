// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, RecordLocator, SpaceCapability, publication::*,
};
use mem_federation_store::*;
use mem_record::{MemoryStatus, MemoryType, Permission, PermissionSet};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    gateway: GatewayStore,
    space: SpaceId,
    authority: InstanceId,
    member: InstanceId,
    fence: WriterFence,
}
fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn record_id() -> RecordId {
    RecordId::new(Uuid::new_v4()).unwrap()
}

impl Fixture {
    async fn new() -> Option<Self> {
        let pool = mem_test_support::migrated_pool().await?;
        let gateway = GatewayStore::new(pool.clone());
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = instance();
        let fence = gateway.initialize(space, authority).await.unwrap();
        Some(Self {
            pool,
            gateway,
            space,
            authority,
            member: instance(),
            fence,
        })
    }
    fn intent(&self, publisher: InstanceId, record: PublicationRecord) -> PublicationIntent {
        let id = record_id();
        PublicationIntent {
            locator: RecordLocator::new(format!("publication:{id:?}")).unwrap(),
            envelope: PublicationEnvelope {
                schema_version: PUBLICATION_SCHEMA_VERSION,
                space: self.space,
                publisher,
                record_id: id,
                author: AuthorIdentity::new("expert-not-publisher").unwrap(),
                authored_at: "2026-09-08T00:00:00Z".parse().unwrap(),
                causation: None,
                record,
            },
        }
    }
    fn membership(&self, capability: SpaceCapability) -> PublicationIntent {
        self.intent(
            self.authority,
            PublicationRecord::Membership {
                member: self.member,
                capability,
            },
        )
    }
    async fn accept(&self, intent: &PublicationIntent) -> PublicationDelivery {
        let result = self
            .gateway
            .accept(self.fence, intent.envelope.publisher, intent)
            .await
            .unwrap();
        assert_eq!(result.outcome, PublicationOutcome::Applied);
        result.delivery.unwrap()
    }
    async fn replica(&self) -> ReplicaStore {
        ReplicaStore::initialize(self.pool.clone(), self.space, self.member, self.authority)
            .await
            .unwrap()
    }
    async fn count(&self, table: &str) -> i64 {
        assert!(
            [
                "federation_accepted",
                "federation_publish_outbox",
                "federation_inbox",
                "federation_source_outbox"
            ]
            .contains(&table)
        );
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE space=$1"))
            .bind(self.space.as_uuid())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    async fn due(&self) {
        sqlx::query("UPDATE federation_publish_outbox SET available_at=clock_timestamp()-interval '1 second',lease_until=CASE WHEN lease_token IS NULL THEN NULL ELSE clock_timestamp()-interval '1 second' END WHERE space=$1 AND NOT published")
            .bind(self.space.as_uuid()).execute(&self.pool).await.unwrap();
    }
}

fn memory(stream: StreamId, source: RecordId, shared: u32) -> PublicationRecord {
    PublicationRecord::MemoryVersion {
        stream,
        publication: MemoryPublication {
            version: SharedVersion::new(shared).unwrap(),
            source: SourceVersionRef {
                record_id: source,
                version: SourceVersion::new(27).unwrap(),
            },
            content: MemoryContent {
                text: "approved knowledge".into(),
                summary: "approved knowledge".into(),
                memory_type: MemoryType::Decision,
                confidence: ConfidencePpm::from_confidence(0.8).unwrap(),
                importance: 3,
                tags: vec![],
            },
            evidence: vec![],
        },
    }
}

async fn insert_source(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    stream: StreamId,
    source: RecordId,
) {
    sqlx::query("INSERT INTO memory_entries (id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES ($1,$2,$3,27,FALSE,'approved knowledge','approved knowledge','decision','project',3,0.8,now(),now(),to_tsvector('approved knowledge'))")
        .bind(source.as_uuid()).bind(project).bind(stream.canonical.as_uuid()).execute(&mut **tx).await.unwrap();
}

#[tokio::test]
async fn source_mutation_and_intent_commit_or_rollback_together_offline() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES ($1,$2,'durability','/disposable')",
    )
    .bind(project)
    .bind(format!("federation-{project}"))
    .execute(&f.pool)
    .await
    .unwrap();
    let access = LocalProjectAccess {
        principal: Uuid::new_v4(),
        project,
        permissions: PermissionSet::from_permissions(&Permission::ALL),
        read_only: false,
    };
    let grant = LocalSpaceGrant {
        principal: access.principal,
        project,
        space: f.space,
        instance: f.authority,
        capabilities: vec![SpaceCapability::Publish],
    };
    let state = PublicationState::new(f.space, f.authority);
    let stream = StreamId {
        origin: f.authority,
        canonical: record_id(),
    };
    let source = record_id();
    let intent = f.intent(f.authority, memory(stream, source, 1));
    let mut tx = f.pool.begin().await.unwrap();
    insert_source(&mut tx, project, stream, source).await;
    queue_intent(&mut tx, &state, &access, Some(&grant), &intent)
        .await
        .unwrap();
    // A different connection cannot see the uncommitted queue or source.
    assert_eq!(f.count("federation_source_outbox").await, 0);
    tx.rollback().await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_entries WHERE id=$1)")
            .bind(source.as_uuid())
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(!exists);
    assert_eq!(f.count("federation_source_outbox").await, 0);

    let mut tx = f.pool.begin().await.unwrap();
    insert_source(&mut tx, project, stream, source).await;
    queue_intent(&mut tx, &state, &access, Some(&grant), &intent)
        .await
        .unwrap();
    queue_intent(&mut tx, &state, &access, Some(&grant), &intent)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        pending_intents(&f.pool, f.space, f.authority, 10)
            .await
            .unwrap(),
        vec![intent.clone()]
    );
    assert_eq!(
        intent_status(&f.pool, f.space, intent.envelope.key())
            .await
            .unwrap(),
        DeliveryStatus::Queued
    );
    // No gateway/PDS request occurred before the source commit.
    let delivery = f.accept(&intent).await;
    let mut tx = f.pool.begin().await.unwrap();
    record_acceptance(&mut tx, &delivery).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        intent_status(&f.pool, f.space, intent.envelope.key())
            .await
            .unwrap(),
        DeliveryStatus::Queued
    );
    let mut tx = f.pool.begin().await.unwrap();
    record_acceptance(&mut tx, &delivery).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        intent_status(&f.pool, f.space, intent.envelope.key())
            .await
            .unwrap(),
        DeliveryStatus::Accepted { sequence: 1 }
    );
    assert!(
        pending_intents(&f.pool, f.space, f.authority, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let mut changed = intent.clone();
    changed.envelope.author = AuthorIdentity::new("forged").unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    assert!(matches!(
        queue_intent(&mut tx, &state, &access, Some(&grant), &changed).await,
        Err(StoreError::Binding)
    ));
    tx.rollback().await.unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    assert!(matches!(
        queue_intent(&mut tx, &state, &access, None, &intent).await,
        Err(StoreError::LocalPolicy(_))
    ));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn concurrent_acceptance_and_lost_reply_retry_have_one_committed_order() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let intent = f.membership(SpaceCapability::Publish);
    let (a, b) = tokio::join!(
        f.gateway.accept(f.fence, f.authority, &intent),
        f.gateway.accept(f.fence, f.authority, &intent)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_ne!(a.outcome, b.outcome);
    assert!(matches!(
        a.outcome,
        PublicationOutcome::Applied | PublicationOutcome::Duplicate
    ));
    assert!(matches!(
        b.outcome,
        PublicationOutcome::Applied | PublicationOutcome::Duplicate
    ));
    assert_eq!(a.delivery, b.delivery);
    assert_eq!(f.count("federation_accepted").await, 1);
    assert_eq!(f.count("federation_publish_outbox").await, 1);
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let gateway = f.gateway.clone();
        let fence = f.fence;
        let authority = f.authority;
        let intent = f.intent(
            authority,
            PublicationRecord::Membership {
                member: instance(),
                capability: SpaceCapability::Pull,
            },
        );
        tasks.push(tokio::spawn(async move {
            gateway
                .accept(fence, authority, &intent)
                .await
                .unwrap()
                .delivery
                .unwrap()
                .sequence
                .get()
        }));
    }
    let mut order = vec![1];
    for task in tasks {
        order.push(task.await.unwrap());
    }
    order.sort();
    assert_eq!(order, (1..=13).collect::<Vec<_>>());
    // Original request retry after intervening commits uses original sequence.
    assert_eq!(
        f.gateway
            .accept(f.fence, f.authority, &intent)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Duplicate
    );
    let mut alias = intent.clone();
    alias.locator = RecordLocator::new("alternate-locator").unwrap();
    assert_eq!(
        f.gateway
            .accept(f.fence, f.authority, &alias)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Rejected(PublicationReject::Equivocation)
    );
    assert_eq!(
        f.gateway
            .accept(f.fence, f.member, &intent)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Rejected(PublicationReject::ForgedPublisher)
    );
    assert_eq!(f.count("federation_accepted").await, 13);
}

#[tokio::test]
async fn database_failure_rolls_back_acceptance_outbound_intent_and_order() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let constraint = format!("fail_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("ALTER TABLE federation_publish_outbox ADD CONSTRAINT {constraint} CHECK (space <> '{}') NOT VALID",f.space.as_uuid()))
        .execute(&f.pool).await.unwrap();
    let intent = f.membership(SpaceCapability::Publish);
    assert!(matches!(
        f.gateway.accept(f.fence, f.authority, &intent).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(f.count("federation_accepted").await, 0);
    assert_eq!(f.count("federation_publish_outbox").await, 0);
    sqlx::query(&format!(
        "ALTER TABLE federation_publish_outbox DROP CONSTRAINT {constraint}"
    ))
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(f.accept(&intent).await.sequence.get(), 1);
    // Retention cannot rewrite or delete accepted history.
    assert!(
        sqlx::query("DELETE FROM federation_accepted WHERE space=$1")
            .bind(f.space.as_uuid())
            .execute(&f.pool)
            .await
            .is_err()
    );
}

#[derive(Default)]
struct FakePds {
    records: Mutex<BTreeMap<String, PublicationDelivery>>,
    lose_reply: Mutex<bool>,
}
impl ImmutablePublisher for FakePds {
    fn publish<'a>(
        &'a self,
        delivery: &'a PublicationDelivery,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), PublishFailure>> + Send + 'a>> {
        Box::pin(async move {
            let mut records = self.records.lock().unwrap();
            if let Some(existing) = records.get(delivery.locator.as_str()) {
                if existing != delivery {
                    return Err(PublishFailure::Conflict);
                }
            } else {
                records.insert(delivery.locator.as_str().into(), delivery.clone());
            }
            let mut lose = self.lose_reply.lock().unwrap();
            if *lose {
                *lose = false;
                return Err(PublishFailure::Unavailable);
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn lost_pds_reply_retries_exact_record_and_only_publishes_contiguous_prefix() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accept(&f.membership(SpaceCapability::Pull)).await;
    let b = f.accept(&f.membership(SpaceCapability::Publish)).await;
    assert!(
        f.gateway
            .pull(f.space, f.member, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    let transport = FakePds::default();
    *transport.lose_reply.lock().unwrap() = true;
    let policy = RetryPolicy::default();
    assert_eq!(
        f.gateway
            .publish_one(f.fence, policy, &transport)
            .await
            .unwrap(),
        WorkerOutcome::Retrying
    );
    assert_eq!(transport.records.lock().unwrap().len(), 1);
    assert_eq!(
        f.gateway.status(f.space, a.envelope.key()).await.unwrap(),
        DeliveryStatus::Accepted { sequence: 1 }
    );
    assert_eq!(
        f.gateway
            .publish_one(f.fence, policy, &transport)
            .await
            .unwrap(),
        WorkerOutcome::Waiting
    );
    assert!(
        f.gateway
            .pull(f.space, f.member, 0, 100)
            .await
            .unwrap()
            .is_empty()
    );
    f.due().await;
    assert_eq!(
        f.gateway
            .publish_one(f.fence, policy, &transport)
            .await
            .unwrap(),
        WorkerOutcome::Published(1)
    );
    assert_eq!(
        f.gateway.pull(f.space, f.member, 0, 100).await.unwrap(),
        vec![a.clone()]
    );
    assert_eq!(
        f.gateway
            .publish_one(f.fence, policy, &transport)
            .await
            .unwrap(),
        WorkerOutcome::Published(2)
    );
    assert_eq!(transport.records.lock().unwrap().len(), 2);
    assert_eq!(
        f.gateway.status(f.space, b.envelope.key()).await.unwrap(),
        DeliveryStatus::Published { sequence: 2 }
    );
    assert_eq!(
        f.gateway
            .publish_one(f.fence, policy, &transport)
            .await
            .unwrap(),
        WorkerOutcome::Idle
    );
    assert!(matches!(
        f.gateway.pull(f.space, f.member, 3, 10).await,
        Err(StoreError::Resync)
    ));
    let replica = f.replica().await;
    let history = f.gateway.pull(f.space, f.member, 0, 100).await.unwrap();
    assert_eq!(replica.receive(f.authority, &history).await.unwrap(), 2);
    assert_eq!(
        replica.status(b.envelope.key()).await.unwrap(),
        Some(DeliveryStatus::Replicated {
            sequence: 2,
            receiver: f.member
        })
    );
}

#[tokio::test]
async fn lease_expiry_retry_limits_conflicts_and_recovery_fence_stale_workers() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.accept(&f.membership(SpaceCapability::Publish)).await;
    let policy = RetryPolicy {
        max_attempts: 2,
        ..Default::default()
    };
    let Claim::Ready(old) = f.gateway.claim_publication(f.fence, policy).await.unwrap() else {
        panic!("ready")
    };
    assert!(matches!(
        f.gateway.claim_publication(f.fence, policy).await.unwrap(),
        Claim::Waiting
    ));
    f.due().await;
    let Claim::Ready(next) = f.gateway.claim_publication(f.fence, policy).await.unwrap() else {
        panic!("retry")
    };
    assert_eq!(next.attempt(), 2);
    assert_eq!(old.delivery(), next.delivery());
    assert!(matches!(
        f.gateway.acknowledge_publication(&old).await,
        Err(StoreError::Fenced)
    ));
    f.due().await;
    assert!(matches!(
        f.gateway.claim_publication(f.fence, policy).await.unwrap(),
        Claim::Blocked
    ));
    f.gateway.retry_blocked(f.fence).await.unwrap();
    let Claim::Ready(lease) = f.gateway.claim_publication(f.fence, policy).await.unwrap() else {
        panic!("manual retry")
    };
    let fresh = f.gateway.rotate_writer(f.fence).await.unwrap();
    assert!(matches!(
        f.gateway.acknowledge_publication(&lease).await,
        Err(StoreError::Fenced)
    ));
    assert!(matches!(
        f.gateway
            .accept(f.fence, f.authority, &f.membership(SpaceCapability::Pull))
            .await,
        Err(StoreError::Fenced)
    ));
    let Claim::Ready(recovered) = f.gateway.claim_publication(fresh, policy).await.unwrap() else {
        panic!("recovered")
    };
    assert!(
        f.gateway
            .fail_publication(&recovered, policy, PublishFailure::Conflict)
            .await
            .unwrap()
    );
    assert!(matches!(
        f.gateway.claim_publication(fresh, policy).await.unwrap(),
        Claim::Blocked
    ));
    let pending = f.gateway.pending_publication(fresh).await.unwrap().unwrap();
    assert_eq!(pending.sequence, 1);
    assert!(pending.blocked);
    assert!(!pending.leased);
    assert_eq!(pending.last_error.as_deref(), Some("immutable_conflict"));
}

#[tokio::test]
async fn quarantined_or_cross_space_intents_never_consume_order_or_outbox_work() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let stream = StreamId {
        origin: f.authority,
        canonical: record_id(),
    };
    let missing_version = f.intent(f.authority, memory(stream, record_id(), 2));
    let result = f
        .gateway
        .accept(f.fence, f.authority, &missing_version)
        .await
        .unwrap();
    assert_eq!(
        result.outcome,
        PublicationOutcome::Quarantined(PublicationQuarantine::VersionGap)
    );
    assert!(result.delivery.is_none());
    let mut missing_evidence = f.intent(f.authority, memory(stream, record_id(), 1));
    let PublicationRecord::MemoryVersion { publication, .. } =
        &mut missing_evidence.envelope.record
    else {
        unreachable!()
    };
    publication.evidence.push(RecordKey {
        publisher: f.authority,
        id: record_id(),
    });
    assert_eq!(
        f.gateway
            .accept(f.fence, f.authority, &missing_evidence)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Quarantined(PublicationQuarantine::MissingDependency)
    );
    let mut wrong_space = f.membership(SpaceCapability::Pull);
    wrong_space.envelope.space = SpaceId::new(Uuid::new_v4()).unwrap();
    assert_eq!(
        f.gateway
            .accept(f.fence, f.authority, &wrong_space)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Rejected(PublicationReject::WrongSpace)
    );
    assert_eq!(f.count("federation_accepted").await, 0);
    assert_eq!(f.count("federation_publish_outbox").await, 0);
    assert_eq!(
        f.accept(&f.membership(SpaceCapability::Pull))
            .await
            .sequence
            .get(),
        1
    );
}

#[tokio::test]
async fn receiver_gap_invalid_batch_and_projection_failure_do_not_move_inbox_or_cursor() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let a = f.accept(&f.membership(SpaceCapability::Pull)).await;
    let b = f.accept(&f.membership(SpaceCapability::Publish)).await;
    let replica = f.replica().await;
    assert!(matches!(
        replica.receive(f.authority, std::slice::from_ref(&b)).await,
        Err(StoreError::Outcome(PublicationOutcome::Quarantined(
            PublicationQuarantine::OrderGap
        )))
    ));
    let mut invalid = b.clone();
    invalid.envelope.author = AuthorIdentity::new("tampered").unwrap();
    assert!(
        replica
            .receive(f.authority, &[a.clone(), invalid])
            .await
            .is_err()
    );
    assert_eq!(f.count("federation_inbox").await, 0);
    assert_eq!(replica.snapshot().await.unwrap().sequence(), 0);
    let constraint = format!("fail_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("ALTER TABLE federation_replicas ADD CONSTRAINT {constraint} CHECK (space <> '{}' OR cursor=0) NOT VALID",f.space.as_uuid()))
        .execute(&f.pool).await.unwrap();
    assert!(matches!(
        replica.receive(f.authority, &[a.clone(), b.clone()]).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(f.count("federation_inbox").await, 0);
    sqlx::query(&format!(
        "ALTER TABLE federation_replicas DROP CONSTRAINT {constraint}"
    ))
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        replica
            .receive(f.authority, &[a.clone(), b.clone()])
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        replica
            .receive(f.authority, &[b.clone(), a.clone()])
            .await
            .unwrap(),
        2
    );
    assert_eq!(f.count("federation_inbox").await, 2);
    assert!(matches!(
        replica.receive(instance(), &[]).await,
        Err(StoreError::Binding)
    ));
    assert!(matches!(
        replica
            .resynchronize(f.authority, std::slice::from_ref(&a))
            .await,
        Err(StoreError::Resync)
    ));
    let mut fork = a.clone();
    fork.locator = RecordLocator::new("different-prefix").unwrap();
    assert!(matches!(
        replica.resynchronize(f.authority, &[fork, b.clone()]).await,
        Err(StoreError::Resync)
    ));
    sqlx::query("UPDATE federation_replicas SET projection='{}' WHERE space=$1")
        .bind(f.space.as_uuid())
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(replica.snapshot().await, Err(StoreError::Corrupt)));
    assert_eq!(
        replica.resynchronize(f.authority, &[a, b]).await.unwrap(),
        2
    );
    assert_eq!(replica.snapshot().await.unwrap().sequence(), 2);
}

#[tokio::test]
async fn revocation_denies_live_pull_before_pds_publication_and_preserves_offline_history() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let grant = f.membership(SpaceCapability::Publish);
    f.accept(&f.membership(SpaceCapability::Pull)).await;
    f.accept(&grant).await;
    let stream = StreamId {
        origin: f.member,
        canonical: record_id(),
    };
    let version = f.intent(f.member, memory(stream, record_id(), 1));
    f.accept(&version).await;
    let pds = FakePds::default();
    for sequence in 1..=3 {
        assert_eq!(
            f.gateway
                .publish_one(f.fence, RetryPolicy::default(), &pds)
                .await
                .unwrap(),
            WorkerOutcome::Published(sequence)
        );
    }
    let replica = f.replica().await;
    replica
        .receive(
            f.authority,
            &f.gateway.pull(f.space, f.member, 0, 100).await.unwrap(),
        )
        .await
        .unwrap();
    f.accept(&f.intent(
        f.authority,
        PublicationRecord::Revocation {
            member: f.member,
            capability: None,
        },
    ))
    .await;
    assert!(matches!(
        f.gateway.pull(f.space, f.member, 0, 100).await,
        Err(StoreError::Outcome(PublicationOutcome::Rejected(
            PublicationReject::Unauthorized
        )))
    ));
    let next = f.intent(f.member, memory(stream, record_id(), 2));
    assert_eq!(
        f.gateway
            .accept(f.fence, f.member, &next)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Rejected(PublicationReject::Unauthorized)
    );
    assert_eq!(
        f.gateway
            .accept(f.fence, f.member, &version)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Duplicate
    );
    assert_eq!(replica.snapshot().await.unwrap().content_version(stream), 1);
    assert_eq!(f.count("federation_accepted").await, 4);
}

#[tokio::test]
async fn resync_replays_evidence_proposals_tombstones_relations_and_provenance() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let stream = StreamId {
        origin: f.authority,
        canonical: record_id(),
    };
    let target = StreamId {
        origin: f.authority,
        canonical: record_id(),
    };
    let mut history = vec![f.accept(&f.membership(SpaceCapability::Publish)).await];
    let evidence = EvidenceSnapshot {
        space: f.space,
        stream,
        public_source_label: "reviewed extract".into(),
        selected_text: "selected bytes only".into(),
        observed_at: "2026-09-08T00:00:00Z".parse().unwrap(),
    };
    let reviewed = ReviewedEvidence::new(
        evidence.clone(),
        ReviewReceipt {
            snapshot_digest: CanonicalJsonCodec::digest(&evidence).unwrap(),
            reviewer: AuthorIdentity::new("reviewer").unwrap(),
            reviewed_at: evidence.observed_at,
        },
    )
    .unwrap();
    let evidence_delivery = f
        .accept(&f.intent(f.authority, PublicationRecord::Evidence(reviewed.clone())))
        .await;
    history.push(evidence_delivery.clone());
    let mut record = memory(stream, record_id(), 1);
    let PublicationRecord::MemoryVersion { publication, .. } = &mut record else {
        unreachable!()
    };
    publication.evidence.push(evidence_delivery.envelope.key());
    let mut corrected = publication.content.clone();
    corrected.text = "approved correction".into();
    history.push(f.accept(&f.intent(f.authority, record)).await);
    history.push(
        f.accept(&f.intent(f.authority, memory(target, record_id(), 1)))
            .await,
    );
    let proposal = f
        .accept(&f.intent(
            f.member,
            PublicationRecord::Proposal(CorrectionProposal {
                target: stream,
                base_version: SharedVersion::new(1).unwrap(),
                content: corrected,
                evidence: vec![evidence_delivery.envelope.key()],
            }),
        ))
        .await;
    history.push(proposal.clone());
    let mut approval = f.intent(
        f.authority,
        PublicationRecord::ResolveProposal {
            proposal: proposal.envelope.key(),
            source: Some(SourceVersionRef {
                record_id: record_id(),
                version: SourceVersion::new(45).unwrap(),
            }),
        },
    );
    approval.envelope.causation = Some(proposal.envelope.key());
    history.push(f.accept(&approval).await);
    history.push(
        f.accept(&f.intent(
            f.authority,
            PublicationRecord::CanonicalState {
                stream,
                state: StatePublication {
                    revision: StateRevision::new(1).unwrap(),
                    content_version: SharedVersion::new(2).unwrap(),
                    status: MemoryStatus::Archived,
                    confidence: ConfidencePpm::from_confidence(0.8).unwrap(),
                    tombstone: true,
                },
            },
        ))
        .await,
    );
    let mut relation = RelationPublication {
        id: record_id(),
        source: stream,
        target,
        relation_type: mem_record::MemoryRelationType::RelatedTo,
        revision: StateRevision::new(1).unwrap(),
        active: true,
    };
    history.push(
        f.accept(&f.intent(f.authority, PublicationRecord::Relation(relation.clone())))
            .await,
    );
    relation.revision = StateRevision::new(2).unwrap();
    relation.active = false;
    history.push(
        f.accept(&f.intent(f.authority, PublicationRecord::Relation(relation)))
            .await,
    );
    let replica = f.replica().await;
    replica.receive(f.authority, &history[..3]).await.unwrap();
    assert_eq!(
        replica.resynchronize(f.authority, &history).await.unwrap(),
        history.len() as u32
    );
    let restarted = f.replica().await;
    let state = restarted.snapshot().await.unwrap();
    let mut reference = PublicationState::new(f.space, f.authority);
    for d in &history {
        assert_eq!(
            reference.apply(d.envelope.publisher, d.clone()),
            PublicationOutcome::Applied
        );
    }
    assert_eq!(state, reference);
    assert_eq!(state.history(stream)[1].author, proposal.envelope.author);
    assert_eq!(
        state
            .evidence(evidence_delivery.envelope.key())
            .unwrap()
            .published_snapshot_bytes()
            .unwrap(),
        reviewed.published_snapshot_bytes().unwrap()
    );
    assert!(state.current_state(stream).unwrap().tombstone);
    assert!(matches!(
        ReplicaStore::initialize(f.pool.clone(), f.space, f.member, instance()).await,
        Err(StoreError::Binding)
    ));
}
