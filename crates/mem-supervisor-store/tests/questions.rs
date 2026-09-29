// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_supervisor::{
    Area, StableId,
    action::{ActionKey, ActionKind},
};
use mem_supervisor_store::{
    ActionSubmission, ReadyTransition, StoreError, SupervisorStore, questions::*,
};
use mem_test_support::IsolatedDatabase;
use uuid::Uuid;

async fn fixture(db: &IsolatedDatabase) -> QuestionSubmission {
    let project = Uuid::new_v4();
    let actor = Uuid::new_v4();
    let token = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Synthetic','/never-disclose')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&db.pool)
    .await
    .expect("synthetic question test operation");
    sqlx::query(
        "INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'service_token','Synthetic')",
    )
    .bind(actor)
    .execute(&db.pool)
    .await
    .expect("synthetic question test operation");
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'operator')")
        .bind(Uuid::new_v4()).bind(actor).bind(project).execute(&db.pool).await.expect("synthetic question test operation");
    sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix) VALUES($1,$2,'Synthetic',$3,'fake')")
        .bind(token).bind(actor).bind(vec![1u8;32]).execute(&db.pool).await.expect("synthetic question test operation");
    QuestionSubmission {
        request_id: Uuid::new_v4(),
        project_id: project,
        instance_id: Uuid::new_v4(),
        profile_digest: [3; 32],
        request_digest: [4; 32],
        policy_revision: Uuid::new_v4(),
        actor: QuestionActor {
            principal_id: actor,
            token_id: Some(token),
            session_id: None,
        },
        question: "SYNTHETIC_PRIVATE_QUESTION".into(),
    }
}

#[tokio::test]
async fn immutable_queue_replay_capacity_expiry_and_rollback() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let input = fixture(&db).await;
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    let first = enqueue_tx(&mut tx, &input)
        .await
        .expect("synthetic question test operation");
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert!(
        read_tx(&mut tx, input.project_id, input.request_id)
            .await
            .expect("synthetic question test operation")
            .is_none()
    );
    let accepted = enqueue_tx(&mut tx, &input)
        .await
        .expect("synthetic question test operation");
    assert_ne!(first.attempt_id, accepted.attempt_id); // rolled-back IDs authorize nothing
    tx.commit()
        .await
        .expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    let retry = enqueue_tx(&mut tx, &input)
        .await
        .expect("synthetic question test operation");
    assert_eq!(retry.attempt_id, accepted.attempt_id);
    assert_eq!(
        terms_digest(&retry.submission).expect("synthetic question test operation"),
        terms_digest(&accepted.submission).expect("synthetic question test operation")
    );
    assert_eq!(
        status_tx(&mut tx, &retry)
            .await
            .expect("synthetic question test operation")
            .state,
        "queued"
    );
    assert!(
        read_tx(&mut tx, Uuid::new_v4(), input.request_id)
            .await
            .expect("synthetic question test operation")
            .is_none()
    );
    let mut changed = input.clone();
    changed.question.push('?');
    assert!(matches!(
        enqueue_tx(&mut tx, &changed).await,
        Err(StoreError::Conflict)
    ));
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    for _ in 1..8 {
        let mut value = input.clone();
        value.request_id = Uuid::new_v4();
        let mut tx = db
            .pool
            .begin()
            .await
            .expect("synthetic question test operation");
        enqueue_tx(&mut tx, &value)
            .await
            .expect("synthetic question test operation");
        tx.commit()
            .await
            .expect("synthetic question test operation");
    }
    let mut extra = input.clone();
    extra.request_id = Uuid::new_v4();
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert!(matches!(
        enqueue_tx(&mut tx, &extra).await,
        Err(StoreError::QueueFull)
    ));
    assert_eq!(
        list_tx(&mut tx, input.project_id, input.actor.principal_id, None)
            .await
            .expect("synthetic question test operation")
            .len(),
        8
    );
    assert!(
        list_tx(&mut tx, input.project_id, Uuid::new_v4(), None)
            .await
            .expect("synthetic question test operation")
            .is_empty()
    );
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    deny(&db.pool, input.request_id)
        .await
        .expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert_eq!(
        status_tx(&mut tx, &accepted)
            .await
            .expect("synthetic question test operation")
            .state,
        "denied"
    );
    assert!(authorize_question_tx(&mut tx, &accepted).await.is_err());
    enqueue_tx(&mut tx, &extra)
        .await
        .expect("synthetic question test operation");
    tx.commit()
        .await
        .expect("synthetic question test operation");
    assert_ne!(
        next(&db.pool, input.project_id, input.profile_digest)
            .await
            .expect("synthetic question test operation")
            .expect("synthetic question test operation")
            .submission
            .request_id,
        input.request_id
    );
    assert!(
        next(&db.pool, input.project_id, [8; 32])
            .await
            .expect("synthetic question test operation")
            .is_none()
    );
    // Explicit past timestamps only in this disposable fixture, not an update
    // bypass against the immutable consent table.
    let expired = Uuid::new_v4();
    sqlx::query("INSERT INTO supervisor_questions SELECT $1,project_id,instance_id,profile_digest,request_digest,policy_revision,principal_id,token_id,session_id,question,$2,clock_timestamp()-interval '20 minutes',clock_timestamp()-interval '10 minutes' FROM supervisor_questions WHERE request_id=$3")
        .bind(expired).bind(Uuid::new_v4()).bind(input.request_id).execute(&db.pool).await.expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    let old = read_tx(&mut tx, input.project_id, expired)
        .await
        .expect("synthetic question test operation")
        .expect("synthetic question test operation");
    assert_eq!(
        status_tx(&mut tx, &old)
            .await
            .expect("synthetic question test operation")
            .state,
        "expired"
    );
    assert!(authorize_question_tx(&mut tx, &old).await.is_err());
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    assert!(
        sqlx::query("UPDATE supervisor_questions SET question='changed' WHERE request_id=$1")
            .bind(input.request_id)
            .execute(&db.pool)
            .await
            .is_err()
    );
    let journal: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_journal")
        .fetch_one(&db.pool)
        .await
        .expect("synthetic question test operation");
    assert_eq!(journal, 0);
    db.cleanup()
        .await
        .expect("synthetic question test operation");
}

#[tokio::test]
async fn revoked_credentials_missing_direct_grants_and_unknown_actors_fail_closed() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let input = fixture(&db).await;
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    let question = enqueue_tx(&mut tx, &input)
        .await
        .expect("synthetic question test operation");
    tx.commit()
        .await
        .expect("synthetic question test operation");
    let mut actor = input.actor.clone();
    actor.token_id = None;
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert!(
        authorize_requester_tx(&mut tx, &actor, input.project_id)
            .await
            .is_err()
    );
    actor.token_id = Some(Uuid::new_v4());
    assert!(
        authorize_requester_tx(&mut tx, &actor, input.project_id)
            .await
            .is_err()
    );
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    sqlx::query("UPDATE auth_project_memberships SET permissions_json='[\"memory_read\"]' WHERE principal_id=$1")
        .bind(input.actor.principal_id).execute(&db.pool).await.expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert!(authorize_question_tx(&mut tx, &question).await.is_err());
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    sqlx::query("UPDATE auth_project_memberships SET permissions_json=NULL WHERE principal_id=$1")
        .bind(input.actor.principal_id)
        .execute(&db.pool)
        .await
        .expect("synthetic question test operation");
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=clock_timestamp() WHERE id=$1")
        .bind(input.actor.token_id)
        .execute(&db.pool)
        .await
        .expect("synthetic question test operation");
    let mut tx = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    assert!(authorize_question_tx(&mut tx, &question).await.is_err());
    tx.rollback()
        .await
        .expect("synthetic question test operation");
    db.cleanup()
        .await
        .expect("synthetic question test operation");
}

#[tokio::test]
async fn distinct_paid_actions_have_one_atomic_execution_slot() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let revision = Uuid::new_v4();
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("synthetic question test operation");
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("synthetic question test operation");
    let mut ids = Vec::new();
    for kind in [ActionKind::AskQuestion, ActionKind::ProposeCorrection] {
        let id = Uuid::new_v4();
        ids.push(id);
        store
            .submit(&ActionSubmission {
                id,
                key: ActionKey {
                    area: Area::Workshop,
                    source_event: StableId::new(id.to_string())
                        .expect("synthetic question test operation"),
                    kind,
                    policy_revision: StableId::new(revision.to_string())
                        .expect("synthetic question test operation"),
                },
                source_digest: [7; 32],
            })
            .await
            .expect("synthetic question test operation");
        for step in [
            ReadyTransition::Propose,
            ReadyTransition::Authorize,
            ReadyTransition::Queue,
        ] {
            store
                .advance(Area::Workshop, id, step, [7; 32])
                .await
                .expect("synthetic question test operation");
        }
    }
    let mut held = db
        .pool
        .begin()
        .await
        .expect("synthetic question test operation");
    sqlx::query("SELECT pg_advisory_xact_lock(1937076338,61)")
        .execute(&mut *held)
        .await
        .expect("synthetic question test operation");
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            store.claim(Area::Workshop, ids[0], Uuid::new_v4(), 30)
        )
        .await
        .is_err(),
        "claim must wait for the execution admission lock, not just its own action row"
    );
    held.rollback()
        .await
        .expect("synthetic question test operation");
    let (a, b) = tokio::join!(
        store.claim(Area::Workshop, ids[0], Uuid::new_v4(), 30),
        store.claim(Area::Workshop, ids[1], Uuid::new_v4(), 30)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let running: i64 =
        sqlx::query_scalar("SELECT count(*) FROM supervisor_actions WHERE state='running'")
            .fetch_one(&db.pool)
            .await
            .expect("synthetic question test operation");
    assert_eq!(running, 1);
    db.cleanup()
        .await
        .expect("synthetic question test operation");
}

#[test]
fn private_question_byte_and_control_bounds() {
    for text in ["", "  ", "hello\u{1b}[2J"] {
        assert!(validate_question(text).is_err());
    }
    assert!(validate_question(&"🦀".repeat(1025)).is_err());
    assert!(validate_question("A\nB\tC").is_ok());
}
