// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
//! Store isolation/fault tests use fake receipt signatures, not crypto evidence.
use codifix_federation::{expertise::*, publication::*, *};
use codifix_federation_store::*;
use uuid::Uuid;

struct FakeSigner;
impl ReceiptAuthority for FakeSigner {
    fn generation(&self) -> u32 {
        1
    }
    fn public_key(&self) -> String {
        "fixture-key-not-a-cryptographic-proof".into()
    }
    fn sign(&self, claims: ReceiptClaims) -> Result<AuditReceipt> {
        Ok(AuditReceipt {
            claims,
            signature: vec![0; 64],
        })
    }
}
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: GatewayStore,
    fence: WriterFence,
    space: SpaceId,
    authority: InstanceId,
    responder: InstanceId,
    requester: InstanceId,
    advertisement: CapabilityAdvertisement,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = codifix_test_support::IsolatedDatabase::new().await?;
        let store = GatewayStore::new(db.pool.clone());
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let authority = instance();
        let responder = instance();
        let requester = instance();
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
                    external_models: std::collections::BTreeSet::from(["fake_model".into()]),
                },
            )
            .await
            .unwrap();
        let advertisement = CapabilityAdvertisement {
            owner: responder,
            collection: RecordId::new(Uuid::new_v4()).unwrap(),
            revision: StateRevision::new(1).unwrap(),
            collection_digest: CanonicalJsonCodec::digest(&"private reviewed snapshot").unwrap(),
            name: "homelab".into(),
            description: "reviewed".into(),
            topics: vec![],
            modes: vec![AnswerMode::Deterministic, AnswerMode::Model],
            requesters: vec![requester],
            model_destinations: vec!["fake_model".into()],
            output_destinations: vec!["local_human".into()],
            active: true,
            limits: Default::default(),
        };
        let f = Self {
            db,
            store,
            fence,
            space,
            authority,
            responder,
            requester,
            advertisement,
        };
        for (member, capability) in [
            (responder, SpaceCapability::Publish),
            (responder, SpaceCapability::AnswerExpertise),
            (responder, SpaceCapability::Pull),
            (requester, SpaceCapability::Pull),
            (requester, SpaceCapability::AskExpertise),
        ] {
            f.publish(
                authority,
                PublicationRecord::Membership { member, capability },
            )
            .await;
        }
        f.publish(
            responder,
            PublicationRecord::Expertise(f.advertisement.clone()),
        )
        .await;
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
                authored_at: chrono::Utc::now(),
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
            panic!("publication claim")
        };
        self.store
            .acknowledge_with_receipt(&lease, serde_json::json!({"fake":true}), &FakeSigner)
            .await
            .unwrap();
    }
    fn question(&self) -> ExpertiseQuestion {
        let created_at = chrono::Utc::now();
        ExpertiseQuestion {
            request_id: Uuid::new_v4(),
            space: self.space,
            requester: self.requester,
            principal: Uuid::new_v4(),
            capability: self.advertisement.key(),
            revision: self.advertisement.revision,
            collection_digest: self.advertisement.collection_digest.clone(),
            question: "PRIVATE QUESTION NOT AN AUDIT EVENT".into(),
            mode: AnswerMode::Deterministic,
            model_destination: None,
            output_destination: "local_human".into(),
            created_at,
            expires_at: created_at + chrono::Duration::seconds(120),
        }
    }
    async fn claim(&self) -> QuestionLease {
        self.store
            .claim_question(self.fence, self.responder)
            .await
            .unwrap()
            .unwrap()
    }
    async fn expire_lease(&self) {
        sqlx::query("UPDATE federation_expertise_mailbox SET lease_until=clock_timestamp()-interval '1 second' WHERE space=$1 AND status='running'")
        .bind(self.space.as_uuid()).execute(&self.db.pool).await.unwrap();
    }
}
fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn refusal(q: &ExpertiseQuestion) -> ExpertiseAnswer {
    ExpertiseAnswer {
        request_digest: q.digest().unwrap(),
        status: ExpertiseStatus::Refused,
        text: "Insufficient reviewed evidence".into(),
        citations: vec![],
        insufficient_evidence: true,
        reason: None,
        untrusted: true,
    }
}

#[tokio::test]
async fn retained_delivery_is_single_use_bound_fenced_and_metadata_only() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    f.store
        .submit_question(f.fence, f.requester, &q)
        .await
        .unwrap();
    let execution = f.claim().await;
    let answer = refusal(&q);
    f.store
        .complete_question(f.fence, f.responder, &execution, &answer)
        .await
        .unwrap();
    assert!(
        f.store
            .submit_question(f.fence, f.requester, &q)
            .await
            .unwrap()
            .answer
            .is_none()
    );
    let nonce = Uuid::new_v4();
    assert!(
        f.store
            .request_answer_delivery(f.fence, f.requester, q.request_id, nonce)
            .await
            .unwrap()
    );
    assert!(
        f.store
            .request_answer_delivery(f.fence, f.requester, q.request_id, nonce)
            .await
            .is_err()
    );
    let lease = f
        .store
        .claim_answer_delivery(f.fence, f.responder)
        .await
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .claim_answer_delivery(f.fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    let permit = lease.authorization(true).unwrap();
    let mut forged = permit.clone();
    forged.request_id = Uuid::new_v4();
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.responder, &forged)
            .await
            .is_err()
    );
    forged = permit.clone();
    forged.answer_digest = CanonicalJsonCodec::digest(&"forged").unwrap();
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.responder, &forged)
            .await
            .is_err()
    );
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.requester, &permit)
            .await
            .is_err()
    );
    let released = f
        .store
        .finish_answer_delivery(f.fence, f.responder, &permit)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(released.answer, Some(answer));
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.responder, &permit)
            .await
            .is_err()
    );
    assert!(
        f.store
            .question_result(f.fence, f.requester, q.request_id)
            .await
            .unwrap()
            .answer
            .is_none()
    );
    let metadata: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(d) FROM federation_expertise_deliveries d WHERE nonce=$1",
    )
    .bind(nonce)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    let serialized = serde_json::to_string(&metadata).unwrap();
    assert!(!serialized.contains(&q.question));
    assert!(!serialized.contains("Insufficient reviewed evidence"));
    assert!(
        !serde_json::to_string(&permit)
            .unwrap()
            .contains(&q.question)
    );
    f.store
        .request_answer_delivery(f.fence, f.requester, q.request_id, Uuid::new_v4())
        .await
        .unwrap();
    let lease = f
        .store
        .claim_answer_delivery(f.fence, f.responder)
        .await
        .unwrap()
        .unwrap();
    let new_fence = f.store.rotate_writer(f.fence).await.unwrap();
    assert!(
        f.store
            .finish_answer_delivery(new_fence, f.responder, &lease.authorization(true).unwrap())
            .await
            .is_err()
    );
    assert!(
        f.store
            .claim_answer_delivery(new_fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn delivery_limits_expiry_policy_and_journal_failure_fail_closed() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    f.store
        .submit_question(f.fence, f.requester, &q)
        .await
        .unwrap();
    let lease = f.claim().await;
    f.store
        .complete_question(f.fence, f.responder, &lease, &refusal(&q))
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION fail_delivery_journal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='delivery_requested' THEN RAISE EXCEPTION 'fixture'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_delivery BEFORE INSERT ON federation_expertise_attempts FOR EACH ROW EXECUTE FUNCTION fail_delivery_journal();")
        .execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .request_answer_delivery(f.fence, f.requester, q.request_id, Uuid::new_v4())
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_deliveries")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_delivery ON federation_expertise_attempts")
        .execute(&f.db.pool)
        .await
        .unwrap();
    for _ in 0..32 {
        f.store
            .request_answer_delivery(f.fence, f.requester, q.request_id, Uuid::new_v4())
            .await
            .unwrap();
    }
    assert!(matches!(
        f.store
            .request_answer_delivery(f.fence, f.requester, q.request_id, Uuid::new_v4())
            .await,
        Err(StoreError::Limits)
    ));
    let lease = f
        .store
        .claim_answer_delivery(f.fence, f.responder)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE federation_expertise_deliveries SET expires_at=clock_timestamp()-interval '1 second'").execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.responder, &lease.authorization(true).unwrap())
            .await
            .is_err()
    );
    assert!(
        f.store
            .claim_answer_delivery(f.fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    f.store
        .request_answer_delivery(f.fence, f.requester, q.request_id, Uuid::new_v4())
        .await
        .unwrap();
    let lease = f
        .store
        .claim_answer_delivery(f.fence, f.responder)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE federation_security SET policy='{}'::jsonb")
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(
        f.store
            .finish_answer_delivery(f.fence, f.responder, &lease.authorization(true).unwrap())
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn exact_retries_scoped_reads_single_claim_and_idempotent_result() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    assert_eq!(
        f.store
            .submit_question(f.fence, f.requester, &q)
            .await
            .unwrap()
            .status,
        ExpertiseStatus::Queued
    );
    assert_eq!(
        f.store
            .submit_question(f.fence, f.requester, &q)
            .await
            .unwrap()
            .status,
        ExpertiseStatus::Queued
    );
    let mut changed = q.clone();
    changed.question = "changed".into();
    assert!(
        f.store
            .submit_question(f.fence, f.requester, &changed)
            .await
            .is_err()
    );
    assert!(
        f.store
            .question_result(f.fence, f.responder, q.request_id)
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        f.store.claim_question(f.fence, f.responder),
        f.store.claim_question(f.fence, f.responder)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_ne!(a.is_some(), b.is_some());
    let lease = a.or(b).unwrap();
    f.store
        .renew_question(f.fence, f.responder, &lease)
        .await
        .unwrap();
    let answer = refusal(&q);
    f.store
        .complete_question(f.fence, f.responder, &lease, &answer)
        .await
        .unwrap();
    f.store
        .complete_question(f.fence, f.responder, &lease, &answer)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .question_result(f.fence, f.requester, q.request_id)
            .await
            .unwrap()
            .answer,
        None // Only a fresh origin authorization may release retained bytes.
    );
    let rows: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM federation_expertise_attempts t")
            .fetch_all(&f.db.pool)
            .await
            .unwrap();
    assert!(!serde_json::to_string(&rows).unwrap().contains(&q.question));
    let published: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT delivery FROM federation_accepted")
            .fetch_all(&f.db.pool)
            .await
            .unwrap();
    assert!(
        !serde_json::to_string(&published)
            .unwrap()
            .contains(&q.question)
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn lost_worker_fenced_and_ambiguous_model_never_reexecuted() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let mut q = f.question();
    f.store
        .submit_question(f.fence, f.requester, &q)
        .await
        .unwrap();
    let old = f.claim().await;
    f.expire_lease().await;
    let recovered = f.claim().await;
    assert_ne!(old.lease_token, recovered.lease_token);
    assert!(matches!(
        f.store
            .complete_question(f.fence, f.responder, &old, &refusal(&q))
            .await,
        Err(StoreError::LostLease)
    ));
    f.store
        .complete_question(f.fence, f.responder, &recovered, &refusal(&q))
        .await
        .unwrap();
    q = f.question();
    q.mode = AnswerMode::Model;
    q.model_destination = Some("fake_model".into());
    f.store
        .submit_question(f.fence, f.requester, &q)
        .await
        .unwrap();
    let lease = f.claim().await;
    f.store
        .reserve_question_model(f.fence, f.responder, &lease, 20, 10000, 1000)
        .await
        .unwrap();
    assert!(
        f.store
            .reserve_question_model(f.fence, f.responder, &lease, 20, 10000, 1000)
            .await
            .is_err()
    );
    f.expire_lease().await;
    assert!(
        f.store
            .claim_question(f.fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.store
            .question_result(f.fence, f.requester, q.request_id)
            .await
            .unwrap()
            .status,
        ExpertiseStatus::Uncertain
    );
    assert!(
        f.store
            .complete_question(f.fence, f.responder, &lease, &refusal(&q))
            .await
            .is_err()
    );
    let budget: (i64, i64) =
        sqlx::query_as("SELECT calls,reserved_tokens FROM federation_expertise_daily_budget")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(budget, (1, 1000));
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn third_instance_needs_enrollment_and_collection_allowlist() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let game = instance();
    let mut q = f.question();
    q.requester = game;
    assert!(f.store.submit_question(f.fence, game, &q).await.is_err());
    for capability in [SpaceCapability::AskExpertise, SpaceCapability::Pull] {
        f.publish(
            f.authority,
            PublicationRecord::Membership {
                member: game,
                capability,
            },
        )
        .await;
    }
    assert!(f.store.submit_question(f.fence, game, &q).await.is_err());
    f.advertisement.requesters.push(game);
    f.advertisement.revision = StateRevision::new(2).unwrap();
    f.publish(
        f.responder,
        PublicationRecord::Expertise(f.advertisement.clone()),
    )
    .await;
    q.revision = f.advertisement.revision;
    f.store.submit_question(f.fence, game, &q).await.unwrap();
    f.publish(
        f.authority,
        PublicationRecord::Revocation {
            member: game,
            capability: Some(SpaceCapability::AskExpertise),
        },
    )
    .await;
    assert!(
        f.store
            .question_result(f.fence, game, q.request_id)
            .await
            .is_err()
    );
    assert!(
        f.store
            .claim_question(f.fence, f.responder)
            .await
            .unwrap()
            .is_none()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn ttl_holds_and_writer_recovery_fail_closed() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    f.store
        .submit_question(f.fence, f.requester, &q)
        .await
        .unwrap();
    let old = f.claim().await;
    f.fence = f.store.rotate_writer(f.fence).await.unwrap();
    assert!(
        f.store
            .renew_question(f.fence, f.responder, &old)
            .await
            .is_err()
    );
    let recovered = f.claim().await;
    f.store
        .complete_question(f.fence, f.responder, &recovered, &refusal(&q))
        .await
        .unwrap();
    sqlx::query("UPDATE federation_expertise_mailbox SET payload_until=clock_timestamp()-interval '1 second'")
        .execute(&f.db.pool).await.unwrap();
    let expired = f
        .store
        .question_result(f.fence, f.requester, q.request_id)
        .await
        .unwrap();
    assert_eq!(expired.status, ExpertiseStatus::Expired);
    assert!(expired.answer.is_none());
    f.store
        .retention_decision(
            f.fence,
            f.authority,
            &RetentionRequest {
                action: RetentionAction::Hold,
                reason: "synthetic hold".into(),
            },
        )
        .await
        .unwrap();
    f.store.cleanup_questions(f.fence).await.unwrap();
    let kept: bool =
        sqlx::query_scalar("SELECT question IS NOT NULL FROM federation_expertise_mailbox")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(kept);
    f.store
        .retention_decision(
            f.fence,
            f.authority,
            &RetentionRequest {
                action: RetentionAction::ReleaseHold,
                reason: "synthetic release".into(),
            },
        )
        .await
        .unwrap();
    f.store.cleanup_questions(f.fence).await.unwrap();
    let removed: bool = sqlx::query_scalar(
        "SELECT question IS NULL AND answer IS NULL FROM federation_expertise_mailbox",
    )
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert!(removed);
    // Purged bodies do not bypass current access checks on metadata.
    f.publish(
        f.authority,
        PublicationRecord::Revocation {
            member: f.requester,
            capability: None,
        },
    )
    .await;
    assert!(
        f.store
            .question_result(f.fence, f.requester, q.request_id)
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn enqueue_journal_failure_rolls_back_and_limits_are_atomic() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let q = f.question();
    sqlx::raw_sql("CREATE FUNCTION fail_f1_journal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture failure'; END $$; CREATE TRIGGER fail_f1_journal BEFORE INSERT ON federation_expertise_attempts FOR EACH ROW EXECUTE FUNCTION fail_f1_journal();")
        .execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .submit_question(f.fence, f.requester, &q)
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_mailbox")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::raw_sql("DROP TRIGGER fail_f1_journal ON federation_expertise_attempts; DROP FUNCTION fail_f1_journal();").execute(&f.db.pool).await.unwrap();
    for _ in 0..32 {
        f.store
            .submit_question(f.fence, f.requester, &f.question())
            .await
            .unwrap();
    }
    assert!(matches!(
        f.store
            .submit_question(f.fence, f.requester, &f.question())
            .await,
        Err(StoreError::Limits)
    ));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn model_budget_exhaustion_and_changed_policy_do_not_grant_another_call() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    for index in 0..2 {
        let mut q = f.question();
        q.mode = AnswerMode::Model;
        q.model_destination = Some("fake_model".into());
        f.store
            .submit_question(f.fence, f.requester, &q)
            .await
            .unwrap();
        let lease = f.claim().await;
        let reserved = f
            .store
            .reserve_question_model(f.fence, f.responder, &lease, 1, 1000, 1000)
            .await;
        if index == 0 {
            reserved.unwrap();
            f.store
                .complete_question(f.fence, f.responder, &lease, &refusal(&q))
                .await
                .unwrap();
        } else {
            assert!(matches!(reserved, Err(StoreError::Limits)));
            // A worker cannot replace its policy pin with a new policy digest.
            let changed = SpacePolicy {
                external_models: std::collections::BTreeSet::from([
                    "fake_model".into(),
                    "another_model".into(),
                ]),
            };
            sqlx::query("UPDATE federation_security SET policy=$2 WHERE space=$1")
                .bind(f.space.as_uuid())
                .bind(sqlx::types::Json(&changed))
                .execute(&f.db.pool)
                .await
                .unwrap();
            assert!(
                f.store
                    .renew_question(f.fence, f.responder, &lease)
                    .await
                    .is_err()
            );
            let mut forged = lease;
            forged.policy_digest = CanonicalJsonCodec::digest(&changed).unwrap();
            assert!(
                f.store
                    .renew_question(f.fence, f.responder, &forged)
                    .await
                    .is_err()
            );
        }
    }
    f.db.cleanup().await.unwrap();
}
