// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
//! Disposable-Postgres gateway tests; proof bytes here are fake store fixtures.
use chrono::Utc;
use mem_federation::{expertise::*, publication::*, *};
use mem_federation_store::*;
use uuid::Uuid;

struct FakeSigner;
impl ReceiptAuthority for FakeSigner {
    fn generation(&self) -> u32 {
        1
    }
    fn public_key(&self) -> String {
        "fixture-key".into()
    }
    fn sign(&self, claims: ReceiptClaims) -> Result<AuditReceipt> {
        Ok(AuditReceipt {
            claims,
            signature: vec![0; 64],
        })
    }
}
struct Fixture {
    db: mem_test_support::IsolatedDatabase,
    store: GatewayStore,
    fence: WriterFence,
    space: SpaceId,
    requester: InstanceId,
    responder: InstanceId,
    ad: CapabilityAdvertisement,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = mem_test_support::IsolatedDatabase::new().await?;
        let store = GatewayStore::new(db.pool.clone());
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = InstanceId::new(Uuid::new_v4()).unwrap();
        let requester = InstanceId::new(Uuid::new_v4()).unwrap();
        let responder = InstanceId::new(Uuid::new_v4()).unwrap();
        let old = store.initialize(space, authority).await.unwrap();
        let fence = store
            .enroll_security(
                old,
                &[ReceiptKey {
                    generation: 1,
                    first_sequence: 1,
                    public_key: FakeSigner.public_key(),
                }],
                &SpacePolicy {
                    external_models: std::collections::BTreeSet::from(["codex_openai".into()]),
                },
            )
            .await
            .unwrap();
        let ad = CapabilityAdvertisement {
            owner: responder,
            collection: RecordId::new(Uuid::new_v4()).unwrap(),
            revision: StateRevision::new(1).unwrap(),
            collection_digest: CanonicalJsonCodec::digest(&"reviewed").unwrap(),
            name: "Workshop".into(),
            description: "reviewed releases".into(),
            topics: vec![],
            modes: vec![AnswerMode::Deterministic],
            requesters: vec![requester],
            model_destinations: vec![],
            output_destinations: vec!["codex_openai".into()],
            active: true,
            limits: Default::default(),
        };
        let f = Self {
            db,
            store,
            fence,
            space,
            requester,
            responder,
            ad,
        };
        for (publisher, record) in [
            (
                authority,
                PublicationRecord::Membership {
                    member: requester,
                    capability: SpaceCapability::Pull,
                },
            ),
            (
                authority,
                PublicationRecord::Membership {
                    member: requester,
                    capability: SpaceCapability::AskExpertise,
                },
            ),
            (
                authority,
                PublicationRecord::Membership {
                    member: responder,
                    capability: SpaceCapability::Pull,
                },
            ),
            (
                authority,
                PublicationRecord::Membership {
                    member: responder,
                    capability: SpaceCapability::Publish,
                },
            ),
            (
                authority,
                PublicationRecord::Membership {
                    member: responder,
                    capability: SpaceCapability::AnswerExpertise,
                },
            ),
            (responder, PublicationRecord::Expertise(f.ad.clone())),
        ] {
            f.publish(publisher, record).await;
        }
        Some(f)
    }
    async fn publish(&self, publisher: InstanceId, record: PublicationRecord) {
        let id = Uuid::new_v4();
        let intent = PublicationIntent {
            locator: RecordLocator::new(format!("fixture:{id}")).unwrap(),
            envelope: PublicationEnvelope {
                schema_version: 3,
                space: self.space,
                publisher,
                record_id: RecordId::new(id).unwrap(),
                author: AuthorIdentity::new("fixture").unwrap(),
                authored_at: Utc::now(),
                causation: None,
                record,
            },
        };
        assert_eq!(
            self.store
                .accept(self.fence, publisher, &intent)
                .await
                .unwrap()
                .outcome,
            PublicationOutcome::Applied
        );
        let Claim::Ready(lease) = self
            .store
            .claim_publication(self.fence, RetryPolicy::default())
            .await
            .unwrap()
        else {
            panic!("claim");
        };
        self.store
            .acknowledge_with_receipt(&lease, serde_json::json!({"fake":true}), &FakeSigner)
            .await
            .unwrap();
    }
    fn question(&self) -> ExpertiseQuestion {
        let now = Utc::now();
        ExpertiseQuestion {
            request_id: Uuid::new_v4(),
            space: self.space,
            requester: self.requester,
            principal: Uuid::new_v4(),
            capability: self.ad.key(),
            revision: self.ad.revision,
            collection_digest: self.ad.collection_digest.clone(),
            question: "What changed?".into(),
            mode: AnswerMode::Deterministic,
            model_destination: None,
            output_destination: "codex_openai".into(),
            created_at: now,
            expires_at: now + chrono::Duration::seconds(120),
        }
    }
}

#[tokio::test]
async fn addressed_exchange_is_fenced_private_and_outside_shared_pull() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    let policy = CanonicalJsonCodec::digest(&"standing-policy").unwrap();
    let record = AddressedExpertiseRecord::question(q.clone(), 1, policy.clone()).unwrap();
    let queued = f
        .store
        .enqueue_addressed(f.fence, f.requester, &record, None)
        .await
        .unwrap();
    assert_eq!(queued.status, AddressedStatus::Queued);
    assert!(
        f.store
            .enqueue_addressed(f.fence, f.requester, &record, None)
            .await
            .is_ok()
    );
    let mut changed = record.clone();
    if let AddressedExpertisePayload::Question(ref mut changed_q) = changed.payload {
        changed_q.question.push('!');
    }
    changed.payload_digest = CanonicalJsonCodec::digest(&changed.payload).unwrap();
    assert!(
        f.store
            .enqueue_addressed(f.fence, f.requester, &changed, None)
            .await
            .is_err()
    );
    assert!(
        f.store
            .read_addressed(f.fence, f.responder, q.request_id, "question")
            .await
            .unwrap()
            .unwrap()
            .record
            .is_none()
    );
    let lease = f.store.claim_addressed(f.fence).await.unwrap().unwrap();
    assert!(
        f.store
            .acknowledge_addressed(
                f.fence,
                &lease,
                "cid1",
                serde_json::json!({"record_cid":"wrong"})
            )
            .await
            .is_err()
    );
    f.store
        .acknowledge_addressed(
            f.fence,
            &lease,
            "cid1",
            serde_json::json!({"record_cid":"cid1"}),
        )
        .await
        .unwrap();
    let delivered = f
        .store
        .read_addressed(f.fence, f.responder, q.request_id, "question")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.record, Some(record.clone()));
    let outsider = InstanceId::new(Uuid::new_v4()).unwrap();
    assert!(
        f.store
            .read_addressed(f.fence, outsider, q.request_id, "question")
            .await
            .is_err()
    );
    assert!(
        f.store
            .addressed_inbox(f.fence, f.responder, 32)
            .await
            .unwrap()
            .len()
            == 1
    );
    let answer_lease = f
        .store
        .claim_addressed_question(f.fence, f.responder)
        .await
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .claim_addressed_question(f.fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    let answer = ExpertiseAnswer {
        request_digest: q.digest().unwrap(),
        status: ExpertiseStatus::Refused,
        text: "Insufficient reviewed evidence".into(),
        citations: vec![],
        insufficient_evidence: true,
        reason: None,
        untrusted: true,
    };
    let answer_record =
        AddressedExpertiseRecord::answer(&q, answer, 1, policy, Utc::now()).unwrap();
    assert!(
        f.store
            .enqueue_addressed(f.fence, f.responder, &answer_record, Some(Uuid::new_v4()))
            .await
            .is_err()
    );
    f.store
        .enqueue_addressed(
            f.fence,
            f.responder,
            &answer_record,
            Some(answer_lease.lease_token),
        )
        .await
        .unwrap();
    assert!(
        f.store
            .addressed_inbox(f.fence, f.responder, 32)
            .await
            .unwrap()
            .is_empty()
    );
    let accepted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_accepted WHERE space=$1")
            .bind(f.space.as_uuid())
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(accepted, 6);
}

#[tokio::test]
async fn expired_addressed_payload_waits_for_verified_pds_deletion_and_honors_hold() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    let record = AddressedExpertiseRecord::question(
        q.clone(),
        1,
        CanonicalJsonCodec::digest(&"policy").unwrap(),
    )
    .unwrap();
    f.store
        .enqueue_addressed(f.fence, f.requester, &record, None)
        .await
        .unwrap();
    let lease = f.store.claim_addressed(f.fence).await.unwrap().unwrap();
    f.store
        .acknowledge_addressed(
            f.fence,
            &lease,
            "cid1",
            serde_json::json!({"record_cid":"cid1"}),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE federation_expertise_addressed SET created_at=clock_timestamp()-interval '3 days',expires_at=clock_timestamp()-interval '2 days',payload_until=clock_timestamp()-interval '1 day' WHERE space=$1 AND request_id=$2")
        .bind(f.space.as_uuid()).bind(q.request_id).execute(&f.db.pool).await.unwrap();
    f.store.cleanup_addressed(f.fence).await.unwrap();
    let retained:bool=sqlx::query_scalar("SELECT record IS NOT NULL FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question'")
        .bind(f.space.as_uuid()).bind(q.request_id).fetch_one(&f.db.pool).await.unwrap();
    assert!(retained);
    let deletion = f
        .store
        .claim_addressed_delete(f.fence)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(deletion.record, record);
    assert!(
        f.store
            .acknowledge_addressed_delete(
                f.fence,
                &AddressedDeleteLease {
                    lease_token: Uuid::new_v4(),
                    ..deletion.clone()
                },
                serde_json::json!({"prior_record_cid":"cid1"})
            )
            .await
            .is_err()
    );
    f.store
        .acknowledge_addressed_delete(
            f.fence,
            &deletion,
            serde_json::json!({"prior_record_cid":"cid1"}),
        )
        .await
        .unwrap();
    let (retained,verified):(bool,String)=sqlx::query_as("SELECT record IS NOT NULL,delete_status FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question'")
        .bind(f.space.as_uuid()).bind(q.request_id).fetch_one(&f.db.pool).await.unwrap();
    assert!(!retained);
    assert_eq!(verified, "verified");

    let other = f.question();
    let record = AddressedExpertiseRecord::question(
        other.clone(),
        1,
        CanonicalJsonCodec::digest(&"policy").unwrap(),
    )
    .unwrap();
    f.store
        .enqueue_addressed(f.fence, f.requester, &record, None)
        .await
        .unwrap();
    sqlx::query("UPDATE federation_expertise_addressed SET created_at=clock_timestamp()-interval '3 days',expires_at=clock_timestamp()-interval '2 days',payload_until=clock_timestamp()-interval '1 day' WHERE space=$1 AND request_id=$2")
        .bind(f.space.as_uuid()).bind(other.request_id).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO federation_retention_decisions(space,decision,actor,action,reason) VALUES($1,1,$2,'hold','fixture')")
        .bind(f.space.as_uuid()).bind(f.responder.as_uuid()).execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .claim_addressed_delete(f.fence)
            .await
            .unwrap()
            .is_none()
    );
    f.store.cleanup_addressed(f.fence).await.unwrap();
    let retained:bool=sqlx::query_scalar("SELECT record IS NOT NULL FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question'")
        .bind(f.space.as_uuid()).bind(other.request_id).fetch_one(&f.db.pool).await.unwrap();
    assert!(retained);
}
