// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use codifix_record::ActivityKind;
use codifix_store::events::{TimelineEvent, append, page};
use uuid::Uuid;

async fn project(pool: &sqlx::PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'event test','/disposable')",
    )
    .bind(id)
    .bind(id.to_string())
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn event_and_mutation_roll_back_together_and_subject_survives_retention() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let id = project(&pool).await;
    let mut event = TimelineEvent::new(id, ActivityKind::Curate, "synthetic mutation".into());
    event.memory_id = Some(Uuid::new_v4());
    event.canonical_id = Some(Uuid::new_v4());
    event.version_no = Some(2);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE projects SET name='rolled back' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    append(&mut tx, &event).await.unwrap();
    tx.rollback().await.unwrap();
    assert!(page(&pool, id, 0, 10).await.unwrap().events.is_empty());
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "event test");
    let mut tx = pool.begin().await.unwrap();
    let seq = append(&mut tx, &event).await.unwrap();
    tx.commit().await.unwrap();
    let first = page(&pool, id, 0, 10).await.unwrap();
    assert_eq!(
        first.events[0]["canonical_id"],
        event.canonical_id.unwrap().to_string()
    );
    assert!(!first.retention_gap);
    sqlx::query("DELETE FROM project_timeline_events WHERE id=$1")
        .bind(event.id)
        .execute(&pool)
        .await
        .unwrap();
    let after_delete = page(&pool, id, 0, 10).await.unwrap();
    assert!(after_delete.retention_gap);
    assert_eq!(after_delete.deleted_through, Some(seq));
    assert!(!page(&pool, id, seq, 10).await.unwrap().retention_gap);
}

#[tokio::test]
async fn later_insert_cannot_overtake_an_uncommitted_position() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let id = project(&pool).await;
    let mut first = pool.begin().await.unwrap();
    let first_seq = append(
        &mut first,
        &TimelineEvent::new(id, ActivityKind::Curate, "first".into()),
    )
    .await
    .unwrap();
    let second_pool = pool.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mut second = tokio::spawn(async move {
        let mut tx = second_pool.begin().await.unwrap();
        started_tx.send(()).unwrap();
        let seq = append(
            &mut tx,
            &TimelineEvent::new(id, ActivityKind::Curate, "second".into()),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        seq
    });
    started_rx.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
            .await
            .is_err()
    );
    assert!(page(&pool, id, 0, 10).await.unwrap().events.is_empty());
    first.commit().await.unwrap();
    let second_seq = second.await.unwrap();
    assert!(second_seq > first_seq);
    let first_page = page(&pool, id, 0, 1).await.unwrap();
    assert_eq!(first_page.next_after, first_seq);
    assert!(first_page.has_more);
    let second_page = page(&pool, id, first_page.next_after, 1).await.unwrap();
    assert_eq!(second_page.next_after, second_seq);
    assert!(!second_page.has_more);
}

#[tokio::test]
async fn append_failure_rolls_back_domain_write_and_rejects_overflow() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let id = project(&pool).await;
    let mut event = TimelineEvent::new(id, ActivityKind::Curate, "once".into());
    let mut tx = pool.begin().await.unwrap();
    append(&mut tx, &event).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE projects SET name='must roll back' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(append(&mut tx, &event).await.is_err());
    tx.rollback().await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "event test");
    event.duration_ms = Some(u64::MAX);
    let mut tx = pool.begin().await.unwrap();
    assert!(matches!(
        append(&mut tx, &event).await,
        Err(codifix_store::Error::Invalid(_))
    ));
    tx.rollback().await.unwrap();
}
