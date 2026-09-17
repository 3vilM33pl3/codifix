// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_supervisor::{
    Area, DEFAULT_MODEL, StableId, UsdMicros,
    action::{ActionKey, ActionKind, ActionState, KnownOutcome, StopReason},
    budget::ProviderRequest,
    proposal::{Proposal, ProposalKind},
};
use mem_supervisor_store::*;
use mem_test_support::IsolatedDatabase;
use std::time::Duration;
use uuid::Uuid;

const PRIVATE_TEXT: &str = "Synthetic private proposal sentinel: never operational metadata";
fn stable(id: Uuid) -> StableId {
    StableId::new(id.to_string()).expect("UUID")
}
fn proposal() -> Proposal {
    Proposal {
        summary: PRIVATE_TEXT.into(),
        proposal: ProposalKind::AskHuman,
        evidence_indices: vec![1, 0],
    }
}
struct Fixture {
    store: SupervisorStore,
    revision: Uuid,
    project: Uuid,
    claim: LeaseClaim,
    attempt: Uuid,
}
async fn fixture(db: &IsolatedDatabase, kind: ActionKind, ttl: u32, settled: bool) -> Fixture {
    let store = SupervisorStore::new(db.pool.clone());
    let revision = Uuid::new_v4();
    let project = Uuid::new_v4();
    let action = Uuid::new_v4();
    let attempt = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'synthetic proposal project','/synthetic')")
        .bind(project).bind(project.to_string()).execute(&db.pool).await.expect("project");
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("install");
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enable");
    store
        .submit(&ActionSubmission {
            id: action,
            key: ActionKey {
                area: Area::Workshop,
                source_event: stable(Uuid::new_v4()),
                kind,
                policy_revision: stable(revision),
            },
            source_digest: [1; 32],
        })
        .await
        .expect("submit");
    for transition in [
        ReadyTransition::Propose,
        ReadyTransition::Authorize,
        ReadyTransition::Queue,
    ] {
        store
            .advance(Area::Workshop, action, transition, [2; 32])
            .await
            .expect("advance");
    }
    let claim = store
        .claim(Area::Workshop, action, Uuid::new_v4(), ttl)
        .await
        .expect("claim");
    let request = ProviderRequest {
        id: stable(attempt),
        action_id: stable(action),
        policy_revision: stable(revision),
        model: StableId::new(DEFAULT_MODEL).expect("model"),
        worst_case_cost: UsdMicros(1000),
    };
    store
        .reserve(&claim, &request, [3; 32])
        .await
        .expect("reserve");
    store.begin(&claim, attempt).await.expect("begin");
    if settled {
        store
            .settle(attempt, UsdMicros(50), [4; 32])
            .await
            .expect("settle");
    }
    Fixture {
        store,
        revision,
        project,
        claim,
        attempt,
    }
}
async fn absent(f: &Fixture, db: &IsolatedDatabase) {
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM supervisor_proposals WHERE action_id=$1),(SELECT count(*) FROM supervisor_action_results WHERE action_id=$1)")
        .bind(f.claim.action_id()).fetch_one(&db.pool).await.expect("atomic absence");
    assert_eq!(counts, (0, 0));
    assert_eq!(
        f.store
            .action(Area::Workshop, f.claim.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Running
    );
}

#[tokio::test]
async fn private_proposal_survives_restart_is_exactly_scoped_and_retry_is_read_only_after_revocation()
 {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::StaleKnowledgeReview, 120, true).await;
    let first = f
        .store
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .expect("publish");
    assert_eq!(first.proposal.summary, PRIVATE_TEXT);
    assert_eq!(first.request_digest, [3; 32]);
    assert_eq!(
        f.store
            .action(Area::Workshop, f.claim.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Verified
    );
    let restarted = SupervisorStore::new(db.pool.clone());
    let read = restarted
        .private_proposal(Area::Workshop, f.project, f.claim.action_id())
        .await
        .expect("restart read");
    assert_eq!(read.proposal_digest, first.proposal_digest);
    assert_eq!(read.proposal.proposal, ProposalKind::AskHuman);
    assert_eq!(read.proposal.evidence_indices, vec![1, 0]);
    for (area, project, action) in [
        (Area::Trust, f.project, f.claim.action_id()),
        (Area::Workshop, Uuid::new_v4(), f.claim.action_id()),
        (Area::Workshop, f.project, Uuid::new_v4()),
    ] {
        assert!(matches!(
            restarted.private_proposal(area, project, action).await,
            Err(StoreError::NotFound)
        ));
    }
    let metadata = serde_json::to_string(&(
        restarted
            .action(Area::Workshop, f.claim.action_id())
            .await
            .expect("action"),
        restarted.attempt(f.attempt).await.expect("attempt"),
        restarted
            .journal(Area::Workshop, 0, 100)
            .await
            .expect("journal"),
    ))
    .expect("metadata");
    assert!(!metadata.contains(PRIVATE_TEXT));
    restarted
        .revoke_policy(Area::Workshop, f.revision)
        .await
        .expect("revoke");
    let count = restarted
        .journal(Area::Workshop, 0, 100)
        .await
        .expect("journal")
        .len();
    let retry = restarted
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .expect("exact retry");
    assert_eq!(retry.proposal_digest, first.proposal_digest);
    assert_eq!(retry.created_at, first.created_at);
    assert!(
        restarted
            .policy(Area::Workshop)
            .await
            .expect("policy")
            .revoked
    );
    assert_eq!(
        restarted
            .journal(Area::Workshop, 0, 100)
            .await
            .expect("journal")
            .len(),
        count
    );
    // Retention does not change project deletion behavior or confer read access.
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(f.project)
        .execute(&db.pool)
        .await
        .expect("delete synthetic project");
    assert!(
        restarted
            .private_proposal(Area::Workshop, f.project, f.claim.action_id())
            .await
            .is_ok()
    );
    assert!(
        restarted
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await
            .is_ok()
    );
    assert!(
        sqlx::query("UPDATE supervisor_proposals SET body=body")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM supervisor_proposals")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn conflicting_private_retries_cannot_relabel_project_body_or_ordered_evidence() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::ProposeCorrection, 120, true).await;
    f.store
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .expect("publish");
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                Uuid::new_v4(),
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Conflict)
    ));
    let mut changed = proposal();
    changed.summary = "Changed synthetic private body".into();
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &changed,
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[6; 32], [5; 32]]
            )
            .await,
        Err(StoreError::Conflict)
    ));
    let mut changed = proposal();
    changed.proposal = ProposalKind::ProposeCorrection;
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &changed,
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Conflict)
    ));
    let mut changed = proposal();
    changed.evidence_indices = vec![0, 1];
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &changed,
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Conflict)
    ));
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn new_private_write_requires_live_policy_settled_attempt_current_project_and_allowed_action()
{
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    for kind in [ActionKind::HealthCheck, ActionKind::InstallMaintenance] {
        let f = fixture(&db, kind, 120, true).await;
        assert!(matches!(
            f.store
                .publish_proposal(
                    &f.claim,
                    &f.attempt,
                    f.project,
                    &proposal(),
                    &[[5; 32], [6; 32]]
                )
                .await,
            Err(StoreError::InvalidProposal)
        ));
        absent(&f, &db).await;
    }
    let f = fixture(&db, ActionKind::AskQuestion, 120, false).await;
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Unsettled)
    ));
    f.store
        .settle(f.attempt, UsdMicros(50), [4; 32])
        .await
        .expect("settle");
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                Uuid::new_v4(),
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::NotFound)
    ));
    f.store
        .revoke_policy(Area::Workshop, f.revision)
        .await
        .expect("revoke");
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Policy(_))
    ));
    absent(&f, &db).await;
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn exact_attempt_and_fence_are_required_and_lease_expiry_rolls_publication_back() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let old = fixture(&db, ActionKind::ReviewRelease, 120, true).await;
    let newer = fixture(&db, ActionKind::ReviewRelease, 120, true).await;
    assert!(matches!(
        newer
            .store
            .publish_proposal(
                &newer.claim,
                &old.attempt,
                newer.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Fenced)
    ));
    let renewed = newer.store.renew(&newer.claim, 180).await.expect("renew");
    assert!(matches!(
        newer
            .store
            .publish_proposal(
                &renewed,
                &newer.attempt,
                newer.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Fenced)
    ));
    absent(&newer, &db).await;
    let f = fixture(&db, ActionKind::ReviewRelease, 1, true).await;
    let mut barrier = db.pool.begin().await.expect("barrier");
    sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area='workshop' FOR UPDATE")
        .fetch_one(&mut *barrier)
        .await
        .expect("barrier lock");
    let store = f.store.clone();
    let claim = f.claim.clone();
    let attempt = f.attempt;
    let project = f.project;
    let write = tokio::spawn(async move {
        store
            .publish_proposal(&claim, &attempt, project, &proposal(), &[[5; 32], [6; 32]])
            .await
    });
    tokio::time::sleep(Duration::from_millis(1100)).await;
    barrier.commit().await.expect("release");
    assert!(matches!(
        write.await.expect("join"),
        Err(StoreError::Fenced)
    ));
    absent(&f, &db).await;
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn private_body_and_verified_outcome_roll_back_together_on_journal_failure() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::StaleKnowledgeReview, 120, true).await;
    sqlx::raw_sql("CREATE FUNCTION synthetic_reject_proposal_journal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event='verified' THEN RAISE EXCEPTION 'synthetic journal failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER synthetic_reject_proposal_journal BEFORE INSERT ON supervisor_journal FOR EACH ROW EXECUTE FUNCTION synthetic_reject_proposal_journal();").execute(&db.pool).await.expect("failure injection");
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::PrivateStorage)
    ));
    absent(&f, &db).await;
    sqlx::query("DROP TRIGGER synthetic_reject_proposal_journal ON supervisor_journal")
        .execute(&db.pool)
        .await
        .expect("remove injection");
    f.store
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .expect("retry commits exactly once");
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn private_errors_discard_database_detail_and_corruption_is_not_returned() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::StaleKnowledgeReview, 120, true).await;
    sqlx::raw_sql("CREATE FUNCTION synthetic_private_error() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic private failure: %',NEW.body->>'summary'; END $$; CREATE TRIGGER synthetic_private_error BEFORE INSERT ON supervisor_proposals FOR EACH ROW EXECUTE FUNCTION synthetic_private_error();").execute(&db.pool).await.expect("private error injection");
    let error = f
        .store
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .err()
        .expect("static error");
    assert!(matches!(error, StoreError::PrivateStorage));
    assert!(!format!("{error:?} {error}").contains(PRIVATE_TEXT));
    assert!(std::error::Error::source(&error).is_none());
    absent(&f, &db).await;
    sqlx::query("DROP TRIGGER synthetic_private_error ON supervisor_proposals")
        .execute(&db.pool)
        .await
        .expect("remove injection");
    f.store
        .publish_proposal(
            &f.claim,
            &f.attempt,
            f.project,
            &proposal(),
            &[[5; 32], [6; 32]],
        )
        .await
        .expect("publish");
    // Only this disposable fixture bypasses the immutable guard to model damage.
    sqlx::query("ALTER TABLE supervisor_proposals DISABLE TRIGGER supervisor_proposal_immutable")
        .execute(&db.pool)
        .await
        .expect("inject corruption");
    sqlx::query("UPDATE supervisor_proposals SET body=jsonb_set(body,'{summary}',to_jsonb('tampered synthetic private text'::text)) WHERE action_id=$1").bind(f.claim.action_id()).execute(&db.pool).await.expect("tamper body");
    assert!(matches!(
        f.store
            .private_proposal(Area::Workshop, f.project, f.claim.action_id())
            .await,
        Err(StoreError::Corrupt)
    ));
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &proposal(),
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::Corrupt)
    ));
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn proposal_bounds_and_reference_validation_apply_before_private_storage() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::StaleKnowledgeReview, 120, true).await;
    let mut invalid = proposal();
    invalid.summary = "x".repeat(4097);
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &invalid,
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::InvalidProposal)
    ));
    let mut invalid = proposal();
    invalid.evidence_indices = vec![0, 0];
    assert!(matches!(
        f.store
            .publish_proposal(
                &f.claim,
                &f.attempt,
                f.project,
                &invalid,
                &[[5; 32], [6; 32]]
            )
            .await,
        Err(StoreError::InvalidProposal)
    ));
    assert!(matches!(
        f.store
            .publish_proposal(&f.claim, &f.attempt, f.project, &proposal(), &[[5; 32]])
            .await,
        Err(StoreError::InvalidProposal)
    ));
    assert!(matches!(
        f.store
            .publish_proposal(&f.claim, &f.attempt, f.project, &proposal(), &[[5; 32]; 9])
            .await,
        Err(StoreError::InvalidProposal)
    ));
    absent(&f, &db).await;
    let empty = Proposal {
        summary: "No actionable proposal".into(),
        proposal: ProposalKind::None,
        evidence_indices: vec![],
    };
    f.store
        .publish_proposal(&f.claim, &f.attempt, f.project, &empty, &[])
        .await
        .expect("explicit no-evidence suggestion");
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn generic_verified_completion_and_reconciliation_cannot_bypass_private_persistence() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let f = fixture(&db, ActionKind::StaleKnowledgeReview, 120, true).await;
    assert!(matches!(
        f.store
            .publish(&f.claim, KnownOutcome::Verified, [9; 32])
            .await,
        Err(StoreError::UnpersistedProposal)
    ));
    absent(&f, &db).await;
    f.store
        .stop(
            Area::Workshop,
            f.claim.action_id(),
            StopReason::Cancelled,
            [8; 32],
        )
        .await
        .expect("stop");
    assert!(matches!(
        f.store
            .reconcile(
                Area::Workshop,
                f.claim.action_id(),
                KnownOutcome::Verified,
                [9; 32]
            )
            .await,
        Err(StoreError::UnpersistedProposal)
    ));
    assert_eq!(
        f.store
            .action(Area::Workshop, f.claim.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Uncertain
    );
    f.store
        .reconcile(
            Area::Workshop,
            f.claim.action_id(),
            KnownOutcome::Failed,
            [9; 32],
        )
        .await
        .expect("failure may reconcile without model body");
    assert_eq!(
        f.store
            .action(Area::Workshop, f.claim.action_id())
            .await
            .expect("action")
            .state,
        ActionState::Failed
    );
    db.cleanup().await.expect("cleanup");
}
