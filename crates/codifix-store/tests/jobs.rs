// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use codifix_record::{JobFailureCode, JobKind, JobStatus};
use codifix_store::jobs::*;
use uuid::Uuid;

async fn setup(pool: &sqlx::PgPool) -> NewJob {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'job test','/disposable')",
    )
    .bind(id)
    .bind(id.to_string())
    .execute(pool)
    .await
    .unwrap();
    NewJob {
        id: Uuid::new_v4(),
        project_id: id,
        operation_id: Uuid::new_v4(),
        kind: JobKind::Embedding,
        work_key: "first".into(),
        actor: JobActor::Principal(Uuid::new_v4()),
        input: serde_json::json!({"version":1,"memory_ids":[]}),
        dependencies: vec![],
    }
}

async fn put(pool: &sqlx::PgPool, job: &NewJob) {
    let mut tx = pool.begin().await.unwrap();
    enqueue(&mut tx, job).await.unwrap();
    tx.commit().await.unwrap();
}

async fn expire(pool: &sqlx::PgPool, lease: &JobLease) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE background_project_slots SET lease_until=clock_timestamp()-interval '1 second' WHERE project_id=$1")
        .bind(lease.job.project_id).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(lease.job.id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn cleanup(pool: &sqlx::PgPool, project: Uuid) {
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn operator_repair_is_fresh_atomic_and_idempotent_without_changing_memory() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = &database.pool;
    let seed = setup(pool).await;
    let version = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,false,'pre-cutover fixture','fixture','implementation','project',3,0.9,now(),now(),to_tsvector('english','fixture'))")
        .bind(version).bind(seed.project_id).execute(pool).await.unwrap();
    let operation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("CREATE TEMP TABLE project_timeline_events (LIKE public.project_timeline_events INCLUDING DEFAULTS, CHECK(FALSE)) ON COMMIT DROP").execute(&mut *tx).await.unwrap();
    assert!(
        codifix_store::post_curation::request_repair(
            &mut tx,
            seed.project_id,
            operation,
            principal,
            &[version]
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(
        list(pool, seed.project_id, None, None, 100)
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    for _ in 0..2 {
        let mut tx = pool.begin().await.unwrap();
        codifix_store::post_curation::request_repair(
            &mut tx,
            seed.project_id,
            operation,
            principal,
            &[version],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    assert_eq!(
        list(pool, seed.project_id, None, None, 100)
            .await
            .unwrap()
            .jobs
            .len(),
        4
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM project_timeline_events WHERE project_id=$1 AND source='operator-repair'").bind(seed.project_id).fetch_one(pool).await.unwrap(),1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM memory_entries WHERE project_id=$1")
            .bind(seed.project_id)
            .fetch_one(pool)
            .await
            .unwrap(),
        1
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(
        codifix_store::post_curation::request_repair(
            &mut tx,
            seed.project_id,
            operation,
            Uuid::new_v4(),
            &[version]
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn independent_claimers_share_the_global_project_limit() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = &database.pool;
    for _ in 0..3 {
        put(pool, &setup(pool).await).await;
    }
    let policy = JobPolicy::default();
    let (a, b, c) = tokio::join!(
        claim(pool, policy),
        claim(pool, policy),
        claim(pool, policy)
    );
    let leases: Vec<_> = [a.unwrap(), b.unwrap(), c.unwrap()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(leases.len(), 2);
    assert_ne!(leases[0].job.project_id, leases[1].job.project_id);
    finish_result(
        begin_result(pool, &leases[0]).await.unwrap(),
        Completion::Succeeded,
        policy,
    )
    .await
    .unwrap();
    let next = claim(pool, policy).await.unwrap().unwrap();
    assert_ne!(next.job.project_id, leases[0].job.project_id);
    assert_ne!(next.job.project_id, leases[1].job.project_id);
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn enqueue_is_atomic_immutable_and_idempotent() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = database.pool.clone();
    let mut job = setup(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    enqueue(&mut tx, &job).await.unwrap();
    tx.rollback().await.unwrap();
    assert!(
        list(&pool, job.project_id, None, None, 10)
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    put(&pool, &job).await;
    put(&pool, &job).await;
    assert_eq!(
        list(&pool, job.project_id, None, None, 10)
            .await
            .unwrap()
            .jobs
            .len(),
        1
    );
    let update = sqlx::query("UPDATE background_jobs SET input='{}' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await;
    assert!(update.is_err());
    job.input = serde_json::json!({"changed":true});
    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        enqueue(&mut tx, &job).await,
        Err(codifix_store::Error::Conflict)
    ));
    tx.rollback().await.unwrap();
    cleanup(&pool, job.project_id).await;
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn concurrent_claims_dependencies_and_restart_recovery_are_fenced() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = database.pool.clone();
    let mut job = setup(&pool).await;
    put(&pool, &job).await;
    let parent = job.id;
    job.id = Uuid::new_v4();
    job.work_key = "dependent".into();
    job.kind = JobKind::SemanticDedup;
    job.dependencies = vec![parent];
    put(&pool, &job).await;
    let policy = JobPolicy::default();
    let (first, second) = tokio::join!(claim(&pool, policy), claim(&pool, policy));
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(
        usize::from(first.is_some()) + usize::from(second.is_some()),
        1
    );
    let lease = first.or(second).unwrap();
    assert_eq!(lease.job.id, parent);
    expire(&pool, &lease).await;
    let recovered = claim(&pool, policy).await.unwrap().unwrap();
    assert_eq!(recovered.job.id, parent);
    assert_ne!(recovered.token, lease.token);
    assert!(matches!(
        begin_result(&pool, &lease).await,
        Err(codifix_store::Error::StaleLease)
    ));
    let mut result = begin_result(&pool, &recovered).await.unwrap();
    sqlx::query("UPDATE projects SET name='fenced result' WHERE id=$1")
        .bind(job.project_id)
        .execute(&mut **result.transaction())
        .await
        .unwrap();
    finish_result(result, Completion::Succeeded, policy)
        .await
        .unwrap();
    let history = detail(&pool, job.project_id, parent).await.unwrap();
    assert!(
        history
            .history
            .iter()
            .any(|a| a.uncertain && a.outcome.as_deref() == Some("lease_lost"))
    );
    let dependent = claim(&pool, policy).await.unwrap().unwrap();
    assert_eq!(dependent.job.id, job.id);
    finish_result(
        begin_result(&pool, &dependent).await.unwrap(),
        Completion::Succeeded,
        policy,
    )
    .await
    .unwrap();
    assert!(claim(&pool, policy).await.unwrap().is_none());
    cleanup(&pool, job.project_id).await;
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn bounded_retries_blocking_and_manual_retry_preserve_history() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = database.pool.clone();
    let job = setup(&pool).await;
    put(&pool, &job).await;
    let policy = JobPolicy {
        max_attempts: 2,
        ..Default::default()
    };
    let first = claim(&pool, policy).await.unwrap().unwrap();
    renew(&pool, &first, policy).await.unwrap();
    finish_result(
        begin_result(&pool, &first).await.unwrap(),
        Completion::Failure(JobFailureCode::Transient),
        policy,
    )
    .await
    .unwrap();
    assert_eq!(
        detail(&pool, job.project_id, job.id)
            .await
            .unwrap()
            .job
            .status,
        JobStatus::RetryWait
    );
    assert!(claim(&pool, policy).await.unwrap().is_none());
    sqlx::query("UPDATE background_jobs SET available_at=clock_timestamp() WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let second = claim(&pool, policy).await.unwrap().unwrap();
    finish_result(
        begin_result(&pool, &second).await.unwrap(),
        Completion::Failure(JobFailureCode::Transient),
        policy,
    )
    .await
    .unwrap();
    assert_eq!(
        detail(&pool, job.project_id, job.id)
            .await
            .unwrap()
            .job
            .status,
        JobStatus::Failed
    );
    let request = Uuid::new_v4();
    assert_eq!(
        retry(&pool, job.project_id, job.id, request, &job.actor)
            .await
            .unwrap()
            .generation,
        2
    );
    assert_eq!(
        retry(&pool, job.project_id, job.id, request, &job.actor)
            .await
            .unwrap()
            .generation,
        2
    );
    assert!(matches!(
        retry(
            &pool,
            job.project_id,
            job.id,
            request,
            &JobActor::System("wrong-actor".into())
        )
        .await,
        Err(codifix_store::Error::Conflict)
    ));
    let third = claim(&pool, policy).await.unwrap().unwrap();
    assert!(
        retry(&pool, job.project_id, job.id, Uuid::new_v4(), &job.actor)
            .await
            .is_err()
    );
    finish_result(
        begin_result(&pool, &third).await.unwrap(),
        Completion::Failure(JobFailureCode::Quota),
        policy,
    )
    .await
    .unwrap();
    let history = detail(&pool, job.project_id, job.id).await.unwrap();
    assert_eq!(history.job.status, JobStatus::Blocked);
    assert_eq!(history.history.len(), 3);
    assert!(detail(&pool, Uuid::new_v4(), job.id).await.is_err());
    assert!(
        list(&pool, Uuid::new_v4(), None, None, 10)
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    cleanup(&pool, job.project_id).await;
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn lease_expiring_during_result_rolls_back_every_output() {
    let Some(database) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = database.pool.clone();
    let job = setup(&pool).await;
    put(&pool, &job).await;
    let policy = JobPolicy::default();
    let lease = claim(&pool, policy).await.unwrap().unwrap();
    let mut result = begin_result(&pool, &lease).await.unwrap();
    sqlx::query("UPDATE projects SET name='must not survive' WHERE id=$1")
        .bind(job.project_id)
        .execute(&mut **result.transaction())
        .await
        .unwrap();
    // Deterministic clock-expiry simulation, without sleeping or paid providers.
    sqlx::query(
        "UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(job.id)
    .execute(&mut **result.transaction())
    .await
    .unwrap();
    assert!(matches!(
        finish_result(result, Completion::Succeeded, policy).await,
        Err(codifix_store::Error::StaleLease)
    ));
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id=$1")
        .bind(job.project_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "job test");
    expire(&pool, &lease).await;
    let recovered = claim(&pool, policy).await.unwrap().unwrap();
    finish_result(
        begin_result(&pool, &recovered).await.unwrap(),
        Completion::Skipped,
        policy,
    )
    .await
    .unwrap();
    cleanup(&pool, job.project_id).await;
    database.cleanup().await.unwrap();
}

#[test]
fn policy_has_bounded_defaults() {
    let policy = JobPolicy::default();
    assert!(policy.validate().is_ok());
    assert_eq!((policy.lease_seconds, policy.max_attempts), (120, 5));
    assert_eq!(
        (policy.delay(1), policy.delay(2), policy.delay(40)),
        (10, 20, 600)
    );
    assert!(
        JobPolicy {
            max_attempts: 0,
            ..policy
        }
        .validate()
        .is_err()
    );
}
