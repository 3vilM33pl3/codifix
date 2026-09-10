// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use mem_federation::{expertise::*, publication::*, *};
use mem_federation_store::*;
use mem_record::{Permission, PermissionSet};
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    db: mem_test_support::IsolatedDatabase,
    access: LocalProjectAccess,
    state: PublicationState,
    owner: InstanceId,
    canonical: RecordId,
    collection: RecordId,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = mem_test_support::IsolatedDatabase::new().await?;
        let project = Uuid::new_v4();
        let principal = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'F1 fixture','/never-export-this-root')")
            .bind(project).bind(format!("f1-{project}")).execute(&db.pool).await.unwrap();
        sqlx::query(
            "INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','F1 fixture')",
        )
        .bind(principal)
        .execute(&db.pool)
        .await
        .unwrap();
        let owner = InstanceId::new(Uuid::new_v4()).unwrap();
        let mut state = PublicationState::new(SpaceId::new(Uuid::new_v4()).unwrap(), owner);
        let envelope = PublicationEnvelope {
            schema_version: 3,
            space: state.space(),
            publisher: owner,
            record_id: RecordId::new(Uuid::new_v4()).unwrap(),
            author: AuthorIdentity::new("authority").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record: PublicationRecord::Membership {
                member: owner,
                capability: SpaceCapability::AnswerExpertise,
            },
        };
        assert_eq!(
            state.apply(
                owner,
                PublicationDelivery::new(
                    AuthoritySequence::new(1).unwrap(),
                    RecordLocator::new("fixture:membership").unwrap(),
                    envelope
                )
                .unwrap()
            ),
            PublicationOutcome::Applied
        );
        let access = LocalProjectAccess {
            project,
            principal,
            read_only: false,
            permissions: PermissionSet::from_permissions(&[
                Permission::MemoryRead,
                Permission::MemoryCurate,
                Permission::BundleExport,
                Permission::AuthManage,
            ]),
        };
        let mut tx = db.pool.begin().await.unwrap();
        set_local_grant(
            &mut tx,
            &access,
            &LocalSpaceGrant {
                project,
                principal,
                space: state.space(),
                instance: owner,
                capabilities: vec![
                    SpaceCapability::Pull,
                    SpaceCapability::Publish,
                    SpaceCapability::AnswerExpertise,
                ],
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let canonical = RecordId::new(Uuid::new_v4()).unwrap();
        insert_version(&db.pool, project, canonical, 27, false).await;
        Some(Self {
            db,
            access,
            state,
            owner,
            canonical,
            collection: RecordId::new(Uuid::new_v4()).unwrap(),
        })
    }
    fn blueprint(&self) -> CollectionBlueprint {
        CollectionBlueprint {
            collection: self.collection,
            name: "Homelab".into(),
            description: "Reviewed knowledge only".into(),
            topics: vec!["recovery".into()],
            modes: vec![AnswerMode::Deterministic],
            requesters: vec![InstanceId::new(Uuid::new_v4()).unwrap()],
            model_destinations: vec![],
            output_destinations: vec!["local_human".into()],
            entries: vec![CollectionSelection {
                canonical: self.canonical,
                evidence: vec![],
            }],
        }
    }
    async fn draft(&self) -> CollectionDraft {
        preview_collection(
            &self.db.pool,
            &self.access,
            self.state.space(),
            self.owner,
            &self.blueprint(),
        )
        .await
        .unwrap()
    }
    async fn activate(&self, draft: &CollectionDraft) -> PublicationIntent {
        let mut tx = self.db.pool.begin().await.unwrap();
        let intent =
            activate_collection(&mut tx, &self.state, &self.access, self.owner, draft, true)
                .await
                .unwrap();
        tx.commit().await.unwrap();
        intent
    }
    async fn grant(&self, enabled: bool) {
        let mut tx = self.db.pool.begin().await.unwrap();
        set_collection_grant(
            &mut tx,
            &self.access,
            self.state.space(),
            self.owner,
            self.collection,
            self.access.principal,
            enabled,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    async fn eligible(&self) -> Result<CollectionSnapshot> {
        let mut tx = self.db.pool.begin().await.unwrap();
        let result = eligible_collection(
            &mut tx,
            &self.access,
            self.state.space(),
            self.owner,
            self.collection,
            StateRevision::new(1).unwrap(),
        )
        .await;
        tx.rollback().await.unwrap();
        result
    }
}
async fn insert_version(
    pool: &PgPool,
    project: Uuid,
    canonical: RecordId,
    version: i32,
    tombstone: bool,
) {
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,$4,$5,'Recovery requires a verified backup','Reviewed recovery','decision','project',3,0.8,now(),now(),to_tsvector('recovery'))")
        .bind(Uuid::new_v4()).bind(project).bind(canonical.as_uuid()).bind(version).bind(tombstone).execute(pool).await.unwrap();
}
#[tokio::test]
async fn activation_is_atomic_and_exactly_retryable_and_outbox_contains_metadata_only() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let draft = f.draft().await;
    let mut tx = f.db.pool.begin().await.unwrap();
    activate_collection(&mut tx, &f.state, &f.access, f.owner, &draft, true)
        .await
        .unwrap();
    // Inject a failure after all writes, before commit: collection and intent roll back together.
    assert!(sqlx::query("SELECT 1/0").execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_reviews")
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        pending_intents(&f.db.pool, f.state.space(), f.owner, 10)
            .await
            .unwrap()
            .is_empty()
    );
    let first = f.activate(&draft).await;
    assert_eq!(first, f.activate(&draft).await);
    let outbound = pending_intents(&f.db.pool, f.state.space(), f.owner, 10)
        .await
        .unwrap();
    assert_eq!(outbound, vec![first]);
    let wire = serde_json::to_string(&outbound).unwrap();
    assert!(!wire.contains("Recovery requires a verified backup"));
    assert!(!wire.contains("never-export-this-root"));
    assert!(f.eligible().await.is_err()); // no implicit grant to reviewer/worker
    f.grant(true).await;
    assert_eq!(f.eligible().await.unwrap().entries, draft.snapshot.entries);
    f.grant(false).await;
    assert!(f.eligible().await.is_err());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn changed_versions_tombstones_and_withdrawal_never_substitute_new_content() {
    for tombstone in [false, true] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        let draft = f.draft().await;
        f.activate(&draft).await;
        f.grant(true).await;
        insert_version(&f.db.pool, f.access.project, f.canonical, 28, tombstone).await;
        assert!(f.eligible().await.unwrap().entries.is_empty());
        let mut altered = draft.clone();
        altered.request_id = Uuid::new_v4();
        altered.review_digest = altered.digest().unwrap();
        let mut tx = f.db.pool.begin().await.unwrap();
        assert!(
            activate_collection(&mut tx, &f.state, &f.access, f.owner, &altered, true)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let request = CollectionWithdrawal {
            request_id: Uuid::new_v4(),
            collection: f.collection,
            expected_revision: StateRevision::new(1).unwrap(),
            confirmed: true,
        };
        let mut tx = f.db.pool.begin().await.unwrap();
        let intent = withdraw_collection(&mut tx, &f.state, &f.access, f.owner, &request)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut tx = f.db.pool.begin().await.unwrap();
        assert_eq!(
            withdraw_collection(&mut tx, &f.state, &f.access, f.owner, &request)
                .await
                .unwrap(),
            intent
        );
        tx.commit().await.unwrap();
        assert!(f.eligible().await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_reviews")
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        f.db.cleanup().await.unwrap();
    }
}
#[tokio::test]
async fn review_rejects_digest_tampering_cross_project_and_unconfirmed_actions() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let mut draft = f.draft().await;
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        activate_collection(&mut tx, &f.state, &f.access, f.owner, &draft, false)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    draft.snapshot.entries[0].content.text = "silently broadened".into();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        activate_collection(&mut tx, &f.state, &f.access, f.owner, &draft, true)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    // Even recalculating every digest cannot forge source content or authorship.
    draft.advertisement.collection_digest = draft.snapshot.digest().unwrap();
    draft.review_digest = draft.digest().unwrap();
    let mut tx = f.db.pool.begin().await.unwrap();
    assert!(
        activate_collection(&mut tx, &f.state, &f.access, f.owner, &draft, true)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut other = f.access.clone();
    other.project = Uuid::new_v4();
    assert!(
        preview_collection(&f.db.pool, &other, f.state.space(), f.owner, &f.blueprint())
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn result_transaction_fences_concurrent_new_versions_and_revoked_service_identity() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let draft = f.draft().await;
    f.activate(&draft).await;
    f.grant(true).await;
    let mut tx = f.db.pool.begin().await.unwrap();
    assert_eq!(
        eligible_collection(
            &mut tx,
            &f.access,
            f.state.space(),
            f.owner,
            f.collection,
            draft.snapshot.revision
        )
        .await
        .unwrap()
        .entries
        .len(),
        1
    );
    let pool = f.db.pool.clone();
    let project = f.access.project;
    let canonical = f.canonical;
    let mut writer = tokio::spawn(async move {
        insert_version(&pool, project, canonical, 28, false).await;
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut writer)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    writer.await.unwrap();
    assert!(f.eligible().await.unwrap().entries.is_empty());
    sqlx::query("UPDATE auth_principals SET disabled_at=now() WHERE id=$1")
        .bind(f.access.principal)
        .execute(&f.db.pool)
        .await
        .unwrap();
    assert!(f.eligible().await.is_err());
    f.db.cleanup().await.unwrap();
}
