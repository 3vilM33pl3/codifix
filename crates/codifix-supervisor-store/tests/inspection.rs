// SPDX-License-Identifier: AGPL-3.0-or-later
use codifix_record::{SupervisorArea as Area, SupervisorJournalEvent};
use codifix_supervisor::{
    StableId,
    action::{ActionKey, ActionKind},
};
use codifix_supervisor_store::{ActionSubmission, StoreError, SupervisorStore};
use codifix_test_support::IsolatedDatabase;
use uuid::Uuid;

#[tokio::test]
async fn inspection_is_identity_bound_read_only_paginated_and_area_local() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let instance = Uuid::new_v4();
    let empty = store
        .inspect(instance, Area::Workshop, 0, 10)
        .await
        .expect("empty");
    assert!(empty.area.is_none() && empty.policy.is_none() && empty.budget.is_none());
    assert!(store.instance_binding().await.expect("binding").is_none());
    for (after, limit) in [(-1, 1), (0, 0), (0, 101)] {
        assert!(matches!(
            store.inspect(instance, Area::Workshop, after, limit).await,
            Err(StoreError::InvalidInput)
        ));
    }
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("bind");
    assert!(matches!(
        store.inspect(Uuid::new_v4(), Area::Workshop, 0, 1).await,
        Err(StoreError::Binding)
    ));
    assert!(matches!(
        store.inspect(instance, Area::Trust, 0, 1).await,
        Err(StoreError::Binding)
    ));
    let revision = Uuid::new_v4();
    let foreign_revision = Uuid::new_v4();
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("policy");
    store
        .install_policy(Area::Trust, foreign_revision)
        .await
        .expect("unrelated policy");
    let action = Uuid::new_v4();
    store
        .submit(&ActionSubmission {
            id: action,
            key: ActionKey {
                area: Area::Workshop,
                source_event: StableId::new(Uuid::new_v4().to_string()).expect("id"),
                kind: ActionKind::HealthCheck,
                policy_revision: StableId::new(revision.to_string()).expect("id"),
            },
            source_digest: [9; 32],
        })
        .await
        .expect("observe");
    let first = store
        .inspect(instance, Area::Workshop, 0, 1)
        .await
        .expect("first");
    assert!(first.has_more);
    assert_eq!(first.journal.len(), 1);
    assert_eq!(
        first.journal[0].event,
        SupervisorJournalEvent::PolicyInstalled
    );
    assert_eq!(first.policy.as_ref().expect("policy").model, "gpt-5.6-sol");
    assert!(!first.policy.as_ref().expect("policy").enabled);
    let second = store
        .inspect(
            instance,
            Area::Workshop,
            first.next_after.parse().expect("cursor"),
            1,
        )
        .await
        .expect("second");
    assert!(!second.has_more);
    assert_eq!(second.journal[0].event, SupervisorJournalEvent::Observed);
    assert_eq!(second.journal[0].action_id, Some(action));
    assert_eq!(second.action_counts[0].count, "1");
    assert_eq!(second.budget.as_ref().expect("budget").calls, 0);
    assert_eq!(
        second.budget.as_ref().expect("budget").reserved_usd_micros,
        "0"
    );
    let encoded = serde_json::to_string(&second).expect("metadata");
    assert!(!encoded.contains(&foreign_revision.to_string()));
    for excluded in [
        "source_digest",
        "source_event",
        "proposal",
        "request_digest",
        "worker",
        "evidence",
    ] {
        assert!(!encoded.contains(excluded), "unexpected field {excluded}");
    }
    let none = store
        .inspect(
            instance,
            Area::Workshop,
            second.next_after.parse().expect("cursor"),
            100,
        )
        .await
        .expect("caught up");
    assert!(none.journal.is_empty());
    assert_eq!(none.next_after, second.next_after);
    assert!(!none.has_more);
    let budget_days: i64 = sqlx::query_scalar("SELECT count(*) FROM supervisor_budget_days")
        .fetch_one(&db.pool)
        .await
        .expect("read-only budget");
    assert_eq!(budget_days, 0);
    assert_eq!(
        store
            .journal(Area::Workshop, 0, 500)
            .await
            .expect("journal")
            .len(),
        2
    );
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enable");
    let current = store
        .inspect(instance, Area::Workshop, 0, 100)
        .await
        .expect("updated");
    assert!(current.policy.as_ref().expect("policy").enabled);
    store
        .revoke_policy(Area::Workshop, revision)
        .await
        .expect("revoke");
    assert!(
        store
            .inspect(instance, Area::Workshop, 0, 100)
            .await
            .expect("revoked")
            .policy
            .expect("policy")
            .revoked
    );
    db.cleanup().await.expect("cleanup");
}
