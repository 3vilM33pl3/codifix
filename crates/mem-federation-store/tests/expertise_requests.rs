// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use mem_federation::{expertise::*, publication::*, *};
use mem_federation_store::*;
use mem_record::{Permission, PermissionSet};
use uuid::Uuid;

struct Fixture {
    db: mem_test_support::IsolatedDatabase,
    access: LocalProjectAccess,
    q: ExpertiseQuestion,
    ad: CapabilityAdvertisement,
    destinations: Vec<String>,
}

#[tokio::test]
async fn origin_results_are_immutable_scoped_recoverable_and_lease_bounded() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let mut lease = QuestionLease {
        question: f.q.clone(),
        lease_token: Uuid::new_v4(),
        lease_until: chrono::Utc::now() + chrono::Duration::seconds(30),
        policy_digest: CanonicalJsonCodec::digest(&"policy").unwrap(),
    };
    let answer = ExpertiseAnswer {
        request_digest: f.q.digest().unwrap(),
        status: ExpertiseStatus::Refused,
        text: "Insufficient reviewed evidence".into(),
        citations: vec![],
        insufficient_evidence: true,
        reason: None,
        untrusted: true,
    };
    let mut tx = f.db.pool.begin().await.unwrap();
    persist_expertise_answer(&mut tx, f.access.project, &lease, &answer)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_origin_results")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    for _ in 0..2 {
        let mut tx = f.db.pool.begin().await.unwrap();
        assert_eq!(
            persist_expertise_answer(&mut tx, f.access.project, &lease, &answer)
                .await
                .unwrap(),
            answer
        );
        tx.commit().await.unwrap();
        lease.lease_token = Uuid::new_v4(); // recovered, authenticated gateway claim
    }
    let mut changed = answer.clone();
    changed.text.push('!');
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        persist_expertise_answer(&mut tx, f.access.project, &lease, &changed)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        persist_expertise_answer(&mut tx, Uuid::new_v4(), &lease, &answer)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    lease.lease_until = chrono::Utc::now() - chrono::Duration::seconds(1);
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        persist_expertise_answer(&mut tx, f.access.project, &lease, &answer)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE federation_expertise_origin_results SET hold=TRUE,payload_until=clock_timestamp()-interval '1 second',metadata_until=clock_timestamp()-interval '1 second'").execute(&f.db.pool).await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    cleanup_expertise_results(&mut tx, f.access.project)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let kept: bool =
        sqlx::query_scalar("SELECT answer IS NOT NULL FROM federation_expertise_origin_results")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(kept);
    lease.lease_until = chrono::Utc::now() + chrono::Duration::seconds(30);
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        persist_expertise_answer(&mut tx, f.access.project, &lease, &answer)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE federation_expertise_origin_results SET hold=FALSE")
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    cleanup_expertise_results(&mut tx, f.access.project)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_origin_results")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    f.db.cleanup().await.unwrap();
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = mem_test_support::IsolatedDatabase::new().await?;
        let access = LocalProjectAccess {
            project: Uuid::new_v4(),
            principal: Uuid::new_v4(),
            read_only: false,
            permissions: PermissionSet::from_permissions(&[
                Permission::MemoryRead,
                Permission::BundleExport,
                Permission::AuthManage,
            ]),
        };
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/synthetic')",
        )
        .bind(access.project)
        .bind(access.project.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','fixture')",
        )
        .bind(access.principal)
        .execute(&db.pool)
        .await
        .unwrap();
        let space = SpaceId::new(Uuid::new_v4()).unwrap();
        let requester = InstanceId::new(Uuid::new_v4()).unwrap();
        let ad = CapabilityAdvertisement {
            owner: InstanceId::new(Uuid::new_v4()).unwrap(),
            collection: RecordId::new(Uuid::new_v4()).unwrap(),
            revision: StateRevision::new(1).unwrap(),
            collection_digest: CanonicalJsonCodec::digest(&"fixture").unwrap(),
            name: "Recovery".into(),
            description: "Reviewed recovery knowledge".into(),
            topics: vec!["recovery".into()],
            modes: vec![AnswerMode::Deterministic],
            requesters: vec![requester],
            model_destinations: vec![],
            output_destinations: vec!["local_human".into()],
            active: true,
            limits: Default::default(),
        };
        let created_at = chrono::Utc::now();
        let q = ExpertiseQuestion {
            request_id: Uuid::new_v4(),
            space,
            requester,
            principal: access.principal,
            capability: ad.key(),
            revision: ad.revision,
            collection_digest: ad.collection_digest.clone(),
            question: "PRIVATE_QUESTION How do I recover?".into(),
            mode: AnswerMode::Deterministic,
            model_destination: None,
            output_destination: "local_human".into(),
            created_at,
            expires_at: created_at + chrono::Duration::seconds(120),
        };
        let mut tx = db.pool.begin().await.unwrap();
        set_local_grant(
            &mut tx,
            &access,
            &LocalSpaceGrant {
                project: access.project,
                principal: access.principal,
                space,
                instance: requester,
                capabilities: vec![SpaceCapability::AskExpertise],
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        Some(Self {
            db,
            access,
            q,
            ad,
            destinations: vec!["local_human".into()],
        })
    }
    async fn review(&self) -> QuestionReview {
        let mut tx = self.db.pool.begin().await.unwrap();
        let review = prepare_expertise_question(
            &mut tx,
            &self.access,
            &self.q,
            &self.ad,
            &self.destinations,
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        review
    }
    async fn load(
        &self,
        access: &LocalProjectAccess,
        destinations: &[String],
    ) -> Result<ExpertiseQuestion> {
        let mut tx = self.db.pool.begin().await.unwrap();
        let result = consented_expertise_question(
            &mut tx,
            access,
            self.q.space,
            self.q.requester,
            self.q.request_id,
            &self.ad,
            destinations,
        )
        .await;
        tx.rollback().await.unwrap();
        result
    }
    async fn confirm(&self, review: &QuestionReview) -> Result<()> {
        let mut tx = self.db.pool.begin().await.unwrap();
        let result = confirm_expertise_question(
            &mut tx,
            &self.access,
            review,
            &self.ad,
            &self.destinations,
            &review.digest,
        )
        .await;
        if result.is_ok() {
            tx.commit().await.unwrap();
        } else {
            tx.rollback().await.unwrap();
        }
        result
    }
}

#[tokio::test]
async fn exact_consent_is_atomic_private_and_readable_in_fresh_transactions() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let review = f.review().await;
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    let mut tx = f.db.pool.begin().await.unwrap();
    confirm_expertise_question(
        &mut tx,
        &f.access,
        &review,
        &f.ad,
        &f.destinations,
        &review.digest,
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    let (a, b) = tokio::join!(f.confirm(&review), f.confirm(&review));
    a.unwrap();
    b.unwrap();
    assert_eq!(f.load(&f.access, &f.destinations).await.unwrap(), f.q);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_request_audit")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let journal: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM federation_expertise_request_audit a")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(!journal.contains("PRIVATE_QUESTION"));
    let mut altered = review.clone();
    altered.question.question.push('!');
    assert!(f.confirm(&altered).await.is_err());
    let mut tx = f.db.pool.begin().await.unwrap();
    let changed = prepare_expertise_question(
        &mut tx,
        &f.access,
        &altered.question,
        &f.ad,
        &f.destinations,
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    // Even genuine new approval cannot replace a previously used request ID.
    assert!(f.confirm(&changed).await.is_err());
    altered.digest = CanonicalJsonCodec::digest(&"invented approval").unwrap();
    assert!(f.confirm(&altered).await.is_err());
    assert_eq!(f.load(&f.access, &f.destinations).await.unwrap(), f.q);
    // A journal failure cannot leave consent committed without its receipt.
    sqlx::raw_sql("CREATE FUNCTION fail_consent_journal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic'; END $$; CREATE TRIGGER fail_consent BEFORE INSERT ON federation_expertise_request_audit FOR EACH ROW EXECUTE FUNCTION fail_consent_journal();")
        .execute(&f.db.pool).await.unwrap();
    let mut next = f.q.clone();
    next.request_id = Uuid::new_v4();
    let mut tx = f.db.pool.begin().await.unwrap();
    let review = prepare_expertise_question(&mut tx, &f.access, &next, &f.ad, &f.destinations)
        .await
        .unwrap();
    assert!(
        confirm_expertise_question(
            &mut tx,
            &f.access,
            &review,
            &f.ad,
            &f.destinations,
            &review.digest
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_requests")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn fresh_policy_identity_scope_and_expiry_are_required_even_with_hold() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    let review = f.review().await;
    f.confirm(&review).await.unwrap();
    assert!(f.load(&f.access, &[]).await.is_err());
    let mut access = f.access.clone();
    access.principal = Uuid::new_v4();
    assert!(f.load(&access, &f.destinations).await.is_err());
    access = f.access.clone();
    access.project = Uuid::new_v4();
    assert!(f.load(&access, &f.destinations).await.is_err());
    access = f.access.clone();
    access.read_only = true;
    assert!(f.load(&access, &f.destinations).await.is_err());
    access = f.access.clone();
    access.permissions = PermissionSet::EMPTY;
    assert!(f.load(&access, &f.destinations).await.is_err());
    f.ad.description.push_str(" changed policy review");
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    assert!(f.confirm(&review).await.is_err());
    f.ad.description = "Reviewed recovery knowledge".into();
    sqlx::query("UPDATE federation_local_grants SET enabled=FALSE")
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    sqlx::query("UPDATE federation_local_grants SET enabled=TRUE")
        .execute(&f.db.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE federation_expertise_requests SET expires_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&f.db.pool)
    .await
    .unwrap();
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    let mut tx = f.db.pool.begin().await.unwrap();
    assert_eq!(
        retained_expertise_consent(&mut tx, &f.access, f.q.space, f.q.requester, f.q.request_id)
            .await
            .unwrap()
            .question,
        f.q
    );
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE federation_expertise_requests SET hold=TRUE,payload_until=clock_timestamp()-interval '1 second',metadata_until=clock_timestamp()-interval '1 second'").execute(&f.db.pool).await.unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        retained_expertise_consent(&mut tx, &f.access, f.q.space, f.q.requester, f.q.request_id)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(f.load(&f.access, &f.destinations).await.is_err());
    let mut tx = f.db.pool.begin().await.unwrap();
    cleanup_expertise_requests(&mut tx, f.access.project)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let retained: bool =
        sqlx::query_scalar("SELECT question IS NOT NULL FROM federation_expertise_requests")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert!(retained);
    sqlx::query("UPDATE federation_expertise_requests SET hold=FALSE")
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    cleanup_expertise_requests(&mut tx, f.access.project)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_request_audit")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    f.db.cleanup().await.unwrap();
}
