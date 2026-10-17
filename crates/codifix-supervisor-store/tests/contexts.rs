// SPDX-License-Identifier: AGPL-3.0-or-later
use codifix_supervisor::{
    Area, DEFAULT_MODEL, StableId, UsdMicros,
    action::{ActionKey, ActionKind},
    budget::{AttemptState, ProviderRequest},
    proposal::{Proposal, ProposalKind},
};
use codifix_supervisor_store::*;
use codifix_test_support::IsolatedDatabase;
use std::time::Duration;
use uuid::Uuid;

fn stable(id: Uuid) -> StableId {
    StableId::new(id.to_string()).expect("UUID")
}
async fn project(db: &IsolatedDatabase) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'synthetic context project','/synthetic')")
        .bind(id).bind(id.to_string()).execute(&db.pool).await.expect("project");
    id
}
async fn claim(store: &SupervisorStore, revision: Uuid, ttl: u32) -> LeaseClaim {
    let id = Uuid::new_v4();
    store
        .submit(&ActionSubmission {
            id,
            key: ActionKey {
                area: Area::Workshop,
                source_event: stable(Uuid::new_v4()),
                kind: ActionKind::StaleKnowledgeReview,
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
            .advance(Area::Workshop, id, transition, [2; 32])
            .await
            .expect("advance");
    }
    store
        .claim(Area::Workshop, id, Uuid::new_v4(), ttl)
        .await
        .expect("claim")
}
async fn fixture(
    db: &IsolatedDatabase,
    ttl: u32,
) -> (SupervisorStore, Uuid, Uuid, Uuid, LeaseClaim) {
    let store = SupervisorStore::new(db.pool.clone());
    let revision = Uuid::new_v4();
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("install");
    store
        .set_enabled(Area::Workshop, revision, true)
        .await
        .expect("enable");
    let instance = Uuid::new_v4();
    let project = project(db).await;
    let claim = claim(&store, revision, ttl).await;
    (store, instance, project, revision, claim)
}
fn request(claim: &LeaseClaim, revision: Uuid) -> (Uuid, ProviderRequest) {
    let id = Uuid::new_v4();
    (
        id,
        ProviderRequest {
            id: stable(id),
            action_id: stable(claim.action_id()),
            policy_revision: stable(revision),
            model: StableId::new(DEFAULT_MODEL).expect("model"),
            worst_case_cost: UsdMicros(100),
        },
    )
}
async fn count_contexts(db: &IsolatedDatabase) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM supervisor_model_contexts")
        .fetch_one(&db.pool)
        .await
        .expect("contexts")
}

#[tokio::test]
async fn local_binding_is_explicit_exact_immutable_and_does_not_enable_policy() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    assert!(store.instance_binding().await.expect("binding").is_none());
    let revision = Uuid::new_v4();
    store
        .install_policy(Area::Workshop, revision)
        .await
        .expect("install");
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let (a, b) = tokio::join!(
        store.bind_instance(first, Area::Workshop),
        store.bind_instance(second, Area::General)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let binding = a.or(b).expect("one winner");
    let again = store
        .bind_instance(binding.instance_id, binding.area)
        .await
        .expect("exact retry");
    assert_eq!(binding.created_at, again.created_at);
    assert!(!store.policy(Area::Workshop).await.expect("policy").enabled);
    assert!(matches!(
        store.bind_instance(Uuid::nil(), binding.area).await,
        Err(StoreError::InvalidInput)
    ));
    assert!(matches!(
        store.bind_instance(Uuid::new_v4(), binding.area).await,
        Err(StoreError::Conflict)
    ));
    let other = if binding.area == Area::Workshop {
        Area::Trust
    } else {
        Area::Workshop
    };
    assert!(matches!(
        store.bind_instance(binding.instance_id, other).await,
        Err(StoreError::Conflict)
    ));
    for sql in [
        "UPDATE supervisor_instance_binding SET instance_id=instance_id",
        "DELETE FROM supervisor_instance_binding",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn dispatch_defaults_deny_and_context_binds_exact_inputs_across_renewal() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, revision, first) = fixture(&db, 30).await;
    let (_, req) = request(&first, revision);
    assert!(matches!(
        store.reserve(&first, &req, [3; 32]).await,
        Err(StoreError::Binding)
    ));
    assert!(matches!(
        store
            .bind_model_work(&first, instance, project, [3; 32], &[[4; 32], [5; 32]])
            .await,
        Err(StoreError::Binding)
    ));
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("bind instance");
    assert!(matches!(
        store
            .bind_model_work(&first, Uuid::new_v4(), project, [3; 32], &[])
            .await,
        Err(StoreError::Binding)
    ));
    assert!(matches!(
        store
            .bind_model_work(&first, instance, Uuid::new_v4(), [3; 32], &[])
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .bind_model_work(&first, instance, project, [3; 32], &[[4; 32]; 9])
            .await,
        Err(StoreError::InvalidInput)
    ));
    assert_eq!(count_contexts(&db).await, 0);
    let context = store
        .bind_model_work(&first, instance, project, [3; 32], &[[4; 32], [5; 32]])
        .await
        .expect("bind context");
    assert_eq!(context.instance_id(), instance);
    assert_eq!(context.area(), Area::Workshop);
    assert_eq!(context.project_id(), project);
    assert_eq!(context.policy_revision(), revision);
    assert_eq!(context.request_digest(), [3; 32]);
    assert_eq!(context.evidence_digests(), &[[4; 32], [5; 32]]);
    let before = store
        .journal(Area::Workshop, 0, 100)
        .await
        .expect("journal")
        .len();
    assert_eq!(
        context,
        store
            .bind_model_work(&first, instance, project, [3; 32], &[[4; 32], [5; 32]])
            .await
            .expect("exact retry")
    );
    assert_eq!(
        before,
        store
            .journal(Area::Workshop, 0, 100)
            .await
            .expect("journal")
            .len()
    );
    let another_project = crate::project(&db).await;
    for (p, digests, request_hash) in [
        (another_project, vec![[4; 32], [5; 32]], [3; 32]),
        (project, vec![[5; 32], [4; 32]], [3; 32]),
        (project, vec![[4; 32], [5; 32]], [9; 32]),
    ] {
        assert!(matches!(
            store
                .bind_model_work(&first, instance, p, request_hash, &digests)
                .await,
            Err(StoreError::Conflict)
        ));
    }
    assert!(matches!(
        store.reserve(&first, &req, [9; 32]).await,
        Err(StoreError::Conflict)
    ));
    assert!(store.check_model_work(&first, &context).await.is_ok());
    let renewed = store.renew(&first, 120).await.expect("renew");
    assert!(matches!(
        store.check_model_work(&first, &context).await,
        Err(StoreError::Fenced)
    ));
    assert!(store.check_model_work(&renewed, &context).await.is_ok());
    assert_eq!(
        context,
        store
            .bind_model_work(&renewed, instance, project, [3; 32], &[[4; 32], [5; 32]])
            .await
            .expect("renewed context")
    );
    let other = claim(&store, revision, 120).await;
    assert!(matches!(
        store.check_model_work(&other, &context).await,
        Err(StoreError::Binding)
    ));
    for sql in [
        "UPDATE supervisor_model_contexts SET project_id=project_id",
        "DELETE FROM supervisor_model_contexts",
    ] {
        assert!(sqlx::query(sql).execute(&db.pool).await.is_err());
    }
    store
        .revoke_policy(Area::Workshop, revision)
        .await
        .expect("revoke");
    assert!(matches!(
        store.check_model_work(&renewed, &context).await,
        Err(StoreError::Policy(_))
    ));
    assert!(matches!(
        store
            .bind_model_work(&renewed, instance, project, [3; 32], &[[4; 32], [5; 32]])
            .await,
        Err(StoreError::Policy(_))
    ));
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn missing_current_project_blocks_dispatch_and_publication_but_not_usage_accounting() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, revision, claim) = fixture(&db, 120).await;
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("instance");
    let context = store
        .bind_model_work(&claim, instance, project, [3; 32], &[[4; 32]])
        .await
        .expect("context");
    let (reserved, req) = request(&claim, revision);
    store.reserve(&claim, &req, [3; 32]).await.expect("reserve");
    let (begun, req) = request(&claim, revision);
    store.reserve(&claim, &req, [3; 32]).await.expect("reserve");
    store.begin(&claim, begun).await.expect("begin");
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(&db.pool)
        .await
        .expect("delete synthetic project");
    assert!(matches!(
        store.check_model_work(&claim, &context).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.begin(&claim, reserved).await,
        Err(StoreError::NotFound)
    ));
    let (_, req) = request(&claim, revision);
    assert!(matches!(
        store.reserve(&claim, &req, [3; 32]).await,
        Err(StoreError::NotFound)
    ));
    store
        .settle(begun, UsdMicros(50), [7; 32])
        .await
        .expect("settlement retained");
    let proposal = Proposal {
        summary: "Synthetic suggestion".into(),
        proposal: ProposalKind::AskHuman,
        evidence_indices: vec![0],
    };
    assert!(matches!(
        store
            .publish_proposal(&claim, &begun, project, &proposal, &[[4; 32]])
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store.attempt(begun).await.expect("attempt").state,
        AttemptState::Settled { .. }
    ));
    assert_eq!(
        store
            .current_budget(Area::Workshop)
            .await
            .expect("budget")
            .1
            .calls,
        2
    );
    assert_eq!(count_contexts(&db).await, 1);
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn result_scope_cannot_be_relabelled_after_model_input_binding() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, revision, claim) = fixture(&db, 120).await;
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("instance");
    store
        .bind_model_work(&claim, instance, project, [3; 32], &[[4; 32], [5; 32]])
        .await
        .expect("context");
    let (id, req) = request(&claim, revision);
    store.reserve(&claim, &req, [3; 32]).await.expect("reserve");
    store.begin(&claim, id).await.expect("begin");
    store
        .settle(id, UsdMicros(50), [7; 32])
        .await
        .expect("settle");
    let proposal = Proposal {
        summary: "Synthetic suggestion".into(),
        proposal: ProposalKind::AskHuman,
        evidence_indices: vec![0],
    };
    let other_project = crate::project(&db).await;
    for (p, evidence) in [
        (other_project, vec![[4; 32], [5; 32]]),
        (project, vec![[5; 32], [4; 32]]),
        (project, vec![[4; 32]]),
    ] {
        assert!(matches!(
            store
                .publish_proposal(&claim, &id, p, &proposal, &evidence)
                .await,
            Err(StoreError::Conflict)
        ));
    }
    let persisted = store
        .publish_proposal(&claim, &id, project, &proposal, &[[4; 32], [5; 32]])
        .await
        .expect("publish exact");
    assert_eq!(persisted.request_digest, [3; 32]);
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn failed_journal_rolls_back_context_and_does_not_leak_diagnostics() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, _, claim) = fixture(&db, 120).await;
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("instance");
    sqlx::raw_sql("CREATE FUNCTION synthetic_context_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event='model_context_bound' THEN RAISE EXCEPTION 'private synthetic context sentinel'; END IF; RETURN NEW; END $$; CREATE TRIGGER synthetic_context_failure BEFORE INSERT ON supervisor_journal FOR EACH ROW EXECUTE FUNCTION synthetic_context_failure();").execute(&db.pool).await.expect("failure injection");
    let error = store
        .bind_model_work(&claim, instance, project, [3; 32], &[])
        .await
        .expect_err("failure");
    assert!(matches!(error, StoreError::PrivateStorage));
    assert!(!format!("{error} {error:?}").contains("sentinel"));
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(count_contexts(&db).await, 0);
    sqlx::query("DROP TRIGGER synthetic_context_failure ON supervisor_journal")
        .execute(&db.pool)
        .await
        .expect("remove injection");
    store
        .bind_model_work(&claim, instance, project, [3; 32], &[])
        .await
        .expect("retry");
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn context_waiting_on_journal_rechecks_lease_and_rolls_back() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, _, claim) = fixture(&db, 2).await;
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("instance");
    let mut barrier = db.pool.begin().await.expect("barrier");
    sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area='workshop' FOR UPDATE")
        .fetch_one(&mut *barrier)
        .await
        .expect("lock journal");
    let writer = store.clone();
    let operation = tokio::spawn(async move {
        writer
            .bind_model_work(&claim, instance, project, [3; 32], &[])
            .await
    });
    tokio::time::timeout(Duration::from_secs(1),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%supervisor_journal_heads%')").fetch_one(&db.pool).await.expect("waiter");
            if blocked {break}
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.expect("operation must reach journal lock");
    tokio::time::sleep(Duration::from_millis(2100)).await;
    barrier.commit().await.expect("release barrier");
    assert!(matches!(
        operation.await.expect("join"),
        Err(StoreError::Fenced)
    ));
    assert_eq!(count_contexts(&db).await, 0);
    assert!(
        !store
            .journal(Area::Workshop, 0, 100)
            .await
            .expect("journal")
            .iter()
            .any(|event| event.event == "model_context_bound")
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn legacy_attempts_cannot_gain_retroactive_context_and_corruption_blocks_dispatch() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let (store, instance, project, revision, legacy) = fixture(&db, 120).await;
    store
        .bind_instance(instance, Area::Workshop)
        .await
        .expect("instance");
    let (id, _) = request(&legacy, revision);
    // Synthetic legacy 0053 history: migration 0055 must not grant it new authority.
    sqlx::query("INSERT INTO supervisor_budget_days(area,day) VALUES('workshop',(clock_timestamp() AT TIME ZONE 'UTC')::date)").execute(&db.pool).await.expect("day");
    sqlx::query("INSERT INTO supervisor_attempts(id,area,action_id,policy_revision,fence,day,model,request_digest,worst_usd_micros) VALUES($1,'workshop',$2,$3,$4,(clock_timestamp() AT TIME ZONE 'UTC')::date,'gpt-5.6-sol',$5,100)")
        .bind(id).bind(legacy.action_id()).bind(revision).bind(legacy.fence()).bind(vec![3_u8;32]).execute(&db.pool).await.expect("legacy attempt");
    sqlx::query("INSERT INTO supervisor_attempt_states(attempt_id) VALUES($1)")
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("state");
    assert!(matches!(
        store
            .bind_model_work(&legacy, instance, project, [3; 32], &[])
            .await,
        Err(StoreError::Binding)
    ));
    assert!(matches!(
        store.begin(&legacy, id).await,
        Err(StoreError::Binding)
    ));
    store
        .release_before_send(&legacy, id, [8; 32])
        .await
        .expect("legacy safe release");
    let fresh = claim(&store, revision, 120).await;
    let context = store
        .bind_model_work(&fresh, instance, project, [3; 32], &[])
        .await
        .expect("fresh context");
    // Only the synthetic disposable owner bypasses immutable guards to test damage.
    sqlx::query(
        "ALTER TABLE supervisor_model_contexts DISABLE TRIGGER supervisor_model_context_immutable",
    )
    .execute(&db.pool)
    .await
    .expect("corruption injection");
    sqlx::query("UPDATE supervisor_model_contexts SET request_digest=$1 WHERE action_id=$2")
        .bind(vec![9_u8; 32])
        .bind(fresh.action_id())
        .execute(&db.pool)
        .await
        .expect("tamper");
    assert!(matches!(
        store.check_model_work(&fresh, &context).await,
        Err(StoreError::Corrupt)
    ));
    let (_, req) = request(&fresh, revision);
    assert!(matches!(
        store.reserve(&fresh, &req, [3; 32]).await,
        Err(StoreError::Corrupt)
    ));
    db.cleanup().await.expect("cleanup");
}
