// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use codifix_store::{Error, executions::*};
use uuid::Uuid;

#[tokio::test]
async fn live_foreground_owner_is_not_recovered_and_lost_owner_cannot_commit() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'original','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&pool)
    .await
    .unwrap();
    let id = Uuid::new_v4();
    let owner = Execution::acquire(&pool, id).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    owner
        .register(&mut tx, Kind::Loop, Some(project))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        pending(&pool, Kind::Loop, 100)
            .await
            .unwrap()
            .iter()
            .all(|found| *found != id)
    );
    let mut tx = pool.begin().await.unwrap();
    owner
        .register(&mut tx, Kind::Loop, Some(project))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(recover(&pool, id).await.unwrap().is_none());
    assert!(matches!(
        Execution::acquire(&pool, id).await,
        Err(Error::Conflict)
    ));
    let mut phase = pool.begin().await.unwrap();
    fence(&mut phase, id).await.unwrap();
    sqlx::query("UPDATE projects SET name='must roll back' WHERE id=$1")
        .bind(project)
        .execute(&mut *phase)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT backend_pid FROM foreground_executions WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Only the session created above on the disposable database is terminated.
    // timeout=0 only acknowledges SIGTERM delivery; the backend may still own
    // its advisory lock when finish_tx runs. Wait for actual termination, not
    // an arbitrary sleep, before asserting that the owner has lost its fence.
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1, 5000)")
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(terminated, "disposable execution backend must terminate");
    drop(owner);
    assert!(matches!(
        finish_tx(&mut phase, id).await,
        Err(Error::StaleLease)
    ));
    phase.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT name FROM projects WHERE id=$1")
            .bind(project)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "original"
    );
    let mut repair = recover(&pool, id).await.unwrap().unwrap();
    assert_eq!(repair.project_id, Some(project));
    sqlx::query("UPDATE projects SET name='also rolled back' WHERE id=$1")
        .bind(project)
        .execute(&mut **repair.transaction())
        .await
        .unwrap();
    drop(repair);
    // Dropping recovery must not mark the domain operation repaired.
    let mut repair = recover(&pool, id).await.unwrap().unwrap();
    sqlx::query("UPDATE projects SET name='interrupted' WHERE id=$1")
        .bind(project)
        .execute(&mut **repair.transaction())
        .await
        .unwrap();
    repair.commit().await.unwrap();
    assert!(recover(&pool, id).await.unwrap().is_none());
    let mut stale = pool.begin().await.unwrap();
    assert!(matches!(
        fence(&mut stale, id).await,
        Err(Error::StaleLease)
    ));
    stale.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM foreground_executions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "interrupted"
    );
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn terminal_domain_write_and_execution_receipt_commit_together() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let id = Uuid::new_v4();
    let owner = Execution::acquire(&pool, id).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    owner
        .register(&mut tx, Kind::Validation, None)
        .await
        .unwrap();
    finish_tx(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
    drop(owner);
    assert!(recover(&pool, id).await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM foreground_executions WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "finished"
    );
    sqlx::query("DELETE FROM foreground_executions WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}
