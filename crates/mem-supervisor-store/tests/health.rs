// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_supervisor::Area;
use mem_supervisor_store::{StoreError, SupervisorStore};
use mem_test_support::IsolatedDatabase;
use uuid::Uuid;

async fn setup(db: &IsolatedDatabase) -> (SupervisorStore, Uuid, Uuid) {
    let store = SupervisorStore::new(db.pool.clone());
    let instance = Uuid::new_v4();
    let revision = Uuid::new_v4();
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("bind synthetic identity");
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("install paused policy");
    (store, instance, revision)
}

#[tokio::test]
async fn concurrent_health_checks_commit_one_metadata_observation_and_restart_keeps_schedule() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, revision) = setup(&db).await;
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'synthetic','SYNTHETIC_PRIVATE_NOT_IN_HEALTH','/synthetic/private')")
        .bind(Uuid::new_v4()).execute(&db.pool).await.expect("seed project");
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("explicit enable");
    let (a, b) = tokio::join!(
        store.observe_health(instance, Area::Workshop),
        store.observe_health(instance, Area::Workshop)
    );
    let a = a.expect("first checker");
    let b = b.expect("second checker");
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
    let observed = a.or(b).expect("one observation");
    assert_eq!(observed.project_count, "1");
    assert_eq!(observed.canonical_count, "0");
    assert!(observed.next_due_at - observed.observed_at >= chrono::Duration::minutes(5));
    let restarted = SupervisorStore::new(db.pool.clone());
    assert!(
        restarted
            .observe_health(instance, Area::Workshop)
            .await
            .expect("restart")
            .is_none()
    );
    let snapshot = restarted
        .inspect(instance, Area::Workshop, 0, 100)
        .await
        .expect("read view");
    assert_eq!(
        snapshot
            .last_health_observation
            .expect("last observation")
            .action_id,
        observed.action_id
    );
    assert_eq!(snapshot.action_counts[0].count, "1");
    assert!(
        snapshot
            .journal
            .iter()
            .any(|e| e.action_id == Some(observed.action_id))
    );
    let metadata = serde_json::to_string(&observed).expect("metadata");
    assert!(!metadata.contains("SYNTHETIC_PRIVATE") && !metadata.contains("/synthetic"));
    for table in [
        "supervisor_attempts",
        "supervisor_budget_days",
        "supervisor_proposals",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&db.pool)
                .await
                .expect("no model work"),
            0
        );
    }
    assert!(
        sqlx::query("DELETE FROM supervisor_health_observations")
            .execute(&db.pool)
            .await
            .is_err()
    );
    // Advance only the synthetic schedule, not a caller-controlled clock.
    sqlx::query(
        "UPDATE supervisor_health_schedule SET next_due_at=clock_timestamp()-interval '1 second'",
    )
    .execute(&db.pool)
    .await
    .expect("synthetic due time");
    assert!(
        restarted
            .observe_health(instance, Area::Workshop)
            .await
            .expect("next period")
            .is_some()
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn health_checks_do_not_bootstrap_identity_or_bypass_pause_revocation_or_area_binding() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, revision) = setup(&db).await;
    assert!(
        store
            .observe_health(instance, Area::Workshop)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_health_schedule")
            .fetch_one(&db.pool)
            .await
            .expect("no lazy schedule"),
        0
    );
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enable");
    assert!(matches!(
        store.observe_health(Uuid::new_v4(), Area::Workshop).await,
        Err(StoreError::Binding)
    ));
    assert!(store.observe_health(instance, Area::Trust).await.is_err());
    store
        .revoke_policy(Area::Workshop, revision)
        .await
        .expect("revoke");
    assert!(
        store
            .observe_health(instance, Area::Workshop)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM supervisor_health_observations")
            .fetch_one(&db.pool)
            .await
            .expect("no observation"),
        0
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn interrupted_health_transaction_rolls_back_schedule_action_result_and_journal() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, revision) = setup(&db).await;
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enable");
    let before = store
        .journal(Area::Workshop, 0, 100)
        .await
        .expect("before")
        .len();
    sqlx::raw_sql("CREATE FUNCTION synthetic_reject_health() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic failure'; END $$; CREATE CONSTRAINT TRIGGER synthetic_reject_health AFTER INSERT ON supervisor_health_observations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_reject_health();")
        .execute(&db.pool).await.expect("failure injection");
    assert!(
        store
            .observe_health(instance, Area::Workshop)
            .await
            .is_err()
    );
    for table in [
        "supervisor_health_schedule",
        "supervisor_health_observations",
        "supervisor_actions",
        "supervisor_action_results",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&db.pool)
                .await
                .expect("rollback"),
            0
        );
    }
    assert_eq!(
        store
            .journal(Area::Workshop, 0, 100)
            .await
            .expect("after")
            .len(),
        before
    );
    sqlx::query("DROP TRIGGER synthetic_reject_health ON supervisor_health_observations")
        .execute(&db.pool)
        .await
        .expect("end failure injection");
    assert!(
        store
            .observe_health(instance, Area::Workshop)
            .await
            .expect("recover")
            .is_some()
    );
    db.cleanup().await.expect("cleanup");
}
