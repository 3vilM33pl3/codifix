// SPDX-License-Identifier: AGPL-3.0-or-later
use mem_supervisor::{
    Area, DEFAULT_MODEL, StableId, UsdMicros,
    action::{ActionKey, ActionKind, ActionState, KnownOutcome, StopReason},
    budget::{AttemptState, BudgetError, ProviderRequest, Settlement},
    policy::PolicyError,
};
use mem_supervisor_store::*;
use mem_test_support::IsolatedDatabase;
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

fn stable(id: Uuid) -> StableId {
    StableId::new(id.to_string()).expect("UUID")
}
fn submission(area: Area, revision: Uuid) -> ActionSubmission {
    ActionSubmission {
        id: Uuid::new_v4(),
        key: ActionKey {
            area,
            source_event: stable(Uuid::new_v4()),
            kind: ActionKind::StaleKnowledgeReview,
            policy_revision: stable(revision),
        },
        source_digest: [7; 32],
    }
}
fn request(claim: &LeaseClaim, revision: Uuid, cost: u64) -> ProviderRequest {
    ProviderRequest {
        id: stable(Uuid::new_v4()),
        action_id: stable(claim.action_id()),
        policy_revision: stable(revision),
        model: StableId::new(DEFAULT_MODEL).expect("model"),
        worst_case_cost: UsdMicros(cost),
    }
}
fn attempt_id(request: &ProviderRequest) -> Uuid {
    Uuid::parse_str(request.id.as_str()).expect("UUID")
}
async fn queue(store: &SupervisorStore, action: &ActionSubmission) {
    store.submit(action).await.expect("submit");
    for transition in [
        ReadyTransition::Propose,
        ReadyTransition::Authorize,
        ReadyTransition::Queue,
    ] {
        store
            .advance(action.key.area, action.id, transition, [8; 32])
            .await
            .expect("transition");
    }
}
async fn setup(store: &SupervisorStore, area: Area, ttl: u32) -> (Uuid, LeaseClaim) {
    let revision = Uuid::new_v4();
    let policy = store.install_policy(area, revision).await.expect("policy");
    assert!(!policy.enabled);
    store
        .set_enabled(area, revision, true)
        .await
        .expect("enable");
    let action = submission(area, revision);
    queue(store, &action).await;
    (
        revision,
        store
            .claim(area, action.id, Uuid::new_v4(), ttl)
            .await
            .expect("claim"),
    )
}

async fn bind_work(
    db: &IsolatedDatabase,
    store: &SupervisorStore,
    claim: &LeaseClaim,
    digest: [u8; 32],
) {
    let instance = match store.instance_binding().await.expect("binding") {
        Some(binding) => binding.instance_id,
        None => {
            store
                .bind_instance(Uuid::new_v4(), claim.area())
                .await
                .expect("bind instance")
                .instance_id
        }
    };
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'synthetic budget fixture','/synthetic')")
        .bind(project).bind(project.to_string()).execute(&db.pool).await.expect("project");
    store
        .bind_model_work(claim, instance, project, digest, &[])
        .await
        .expect("bind model work");
}

#[tokio::test]
async fn defaults_idempotency_exclusive_claim_fences_and_immutable_results() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    assert!(matches!(
        store.policy(Area::Trust).await,
        Err(StoreError::NotFound)
    ));
    let revision = Uuid::new_v4();
    store
        .install_policy(Area::Trust, revision)
        .await
        .expect("install");
    let action = submission(Area::Trust, revision);
    store.submit(&action).await.expect("submit");
    store.submit(&action).await.expect("duplicate");
    let mut conflict = action.clone();
    conflict.source_digest = [1; 32];
    assert!(matches!(
        store.submit(&conflict).await,
        Err(StoreError::Conflict)
    ));
    store
        .advance(Area::Trust, action.id, ReadyTransition::Propose, [8; 32])
        .await
        .expect("propose");
    assert!(matches!(
        store
            .advance(Area::Trust, action.id, ReadyTransition::Authorize, [8; 32])
            .await,
        Err(StoreError::Policy(PolicyError::Disabled))
    ));
    store
        .set_enabled(Area::Trust, revision, true)
        .await
        .expect("enable");
    store
        .advance(Area::Trust, action.id, ReadyTransition::Authorize, [8; 32])
        .await
        .expect("authorize");
    store
        .advance(Area::Trust, action.id, ReadyTransition::Queue, [8; 32])
        .await
        .expect("queue");
    let (a, b) = tokio::join!(
        store.claim(Area::Trust, action.id, Uuid::new_v4(), 10),
        store.claim(Area::Trust, action.id, Uuid::new_v4(), 10)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let first = a.or(b).expect("one claimant");
    let renewed = store.renew(&first, 20).await.expect("renew");
    assert!(renewed.fence() > first.fence());
    assert!(matches!(
        store.publish(&first, KnownOutcome::Verified, [9; 32]).await,
        Err(StoreError::Fenced)
    ));
    store
        .publish(&renewed, KnownOutcome::Verified, [9; 32])
        .await
        .expect("publish");
    assert_eq!(
        store
            .action(Area::Trust, action.id)
            .await
            .expect("read")
            .state,
        ActionState::Verified
    );
    assert!(
        sqlx::query("UPDATE supervisor_action_results SET outcome='failed' WHERE action_id=$1")
            .bind(action.id)
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM supervisor_journal")
            .execute(&db.pool)
            .await
            .is_err()
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn concurrent_admission_caps_are_area_local_and_duplicate_begin_cannot_resend() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Homelab, 120).await;
    let mut jobs = Vec::new();
    // Each action independently locks; shared day admission must prevent races.
    for _ in 0..25 {
        let action = submission(Area::Homelab, revision);
        queue(&store, &action).await;
        let lease = store
            .claim(Area::Homelab, action.id, Uuid::new_v4(), 120)
            .await
            .expect("claim");
        let request = request(&lease, revision, 100_000);
        bind_work(&db, &store, &lease, [2; 32]).await;
        let cloned = store.clone();
        jobs.push(tokio::spawn(async move {
            cloned.reserve(&lease, &request, [2; 32]).await
        }));
    }
    let mut accepted = 0;
    for job in jobs {
        match job.await.expect("join") {
            Ok(Reservation::Reserved(_)) => accepted += 1,
            Err(StoreError::Budget(BudgetError::CallLimit)) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(accepted, 20);
    let (_, totals) = store.current_budget(Area::Homelab).await.expect("budget");
    assert_eq!(totals.calls, 20);
    assert_eq!(totals.reserved_usd_micros, 2_000_000);
    // Each model-dispatch database has exactly one local instance/area identity.
    let other_db = IsolatedDatabase::new().await.expect("second area database");
    let other_store = SupervisorStore::new(other_db.pool.clone());
    let (other_revision, other) = setup(&other_store, Area::General, 120).await;
    bind_work(&other_db, &other_store, &other, [3; 32]).await;
    let request = request(&other, other_revision, 2_500_000);
    assert!(matches!(
        other_store
            .reserve(&other, &request, [3; 32])
            .await
            .expect("reserve"),
        Reservation::Reserved(_)
    ));
    assert!(matches!(
        other_store
            .reserve(&other, &request, [3; 32])
            .await
            .expect("duplicate"),
        Reservation::Existing(_)
    ));
    assert!(matches!(
        other_store.reserve(&other, &request, [4; 32]).await,
        Err(StoreError::Conflict)
    ));
    let permit = other_store
        .begin(&other, attempt_id(&request))
        .await
        .expect("begin");
    assert_eq!(permit.request_digest(), [3; 32]);
    assert_eq!(permit.area(), Area::General);
    assert_eq!(permit.expires_at(), other.expires_at());
    assert!(permit.admitted_at() < permit.expires_at());
    assert!(matches!(
        other_store.begin(&other, attempt_id(&request)).await,
        Err(StoreError::Transition)
    ));
    let next = request_with_new_id(request.clone());
    assert!(matches!(
        other_store.reserve(&other, &next, [3; 32]).await,
        Err(StoreError::Budget(BudgetError::CostLimit))
    ));
    assert!(store.check_current(&claim).await.is_ok());
    other_db.cleanup().await.expect("cleanup other area");
    db.cleanup().await.expect("cleanup");
}
fn request_with_new_id(mut request: ProviderRequest) -> ProviderRequest {
    request.id = stable(Uuid::new_v4());
    request
}

#[tokio::test]
async fn reduced_limits_release_rotation_and_revocation_do_not_reset_accounting() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Workshop, 120).await;
    bind_work(&db, &store, &claim, [3; 32]).await;
    let first = request(&claim, revision, 500_000);
    store
        .reserve(&claim, &first, [3; 32])
        .await
        .expect("reserve");
    store
        .reduce_limits(Area::Workshop, revision, UsdMicros(499_999), 20)
        .await
        .expect("reduce");
    assert!(matches!(
        store.begin(&claim, attempt_id(&first)).await,
        Err(StoreError::Budget(BudgetError::ReducedLimit))
    ));
    assert!(
        store
            .reduce_limits(Area::Workshop, revision, UsdMicros(500_000), 20)
            .await
            .is_err()
    );
    store
        .release_before_send(&claim, attempt_id(&first), [0; 32])
        .await
        .expect("release");
    assert_eq!(
        store
            .current_budget(Area::Workshop)
            .await
            .expect("budget")
            .1
            .calls,
        0
    );
    assert!(matches!(
        store
            .reserve(&claim, &first, [3; 32])
            .await
            .expect("duplicate released"),
        Reservation::Existing(_)
    ));
    assert!(matches!(
        store.begin(&claim, attempt_id(&first)).await,
        Err(StoreError::Transition)
    ));
    let second = request(&claim, revision, 400_000);
    store
        .reserve(&claim, &second, [3; 32])
        .await
        .expect("reserve");
    store
        .begin(&claim, attempt_id(&second))
        .await
        .expect("begin");
    let new_revision = Uuid::new_v4();
    assert!(
        !store
            .install_policy(Area::Workshop, new_revision)
            .await
            .expect("rotate")
            .enabled
    );
    assert!(matches!(
        store.install_policy(Area::Workshop, revision).await,
        Err(StoreError::Conflict)
    ));
    assert!(
        store
            .publish(&claim, KnownOutcome::Verified, [4; 32])
            .await
            .is_err()
    );
    assert_eq!(
        store
            .settle(attempt_id(&second), UsdMicros(250_000), [5; 32])
            .await
            .expect("late settlement"),
        Settlement::Settled
    );
    assert_eq!(
        store
            .current_budget(Area::Workshop)
            .await
            .expect("budget")
            .1
            .charged_usd_micros,
        250_000
    );
    store
        .set_enabled(Area::Workshop, new_revision, true)
        .await
        .expect("enable new");
    store
        .revoke_policy(Area::Workshop, new_revision)
        .await
        .expect("revoke");
    assert!(matches!(
        store.set_enabled(Area::Workshop, new_revision, true).await,
        Err(StoreError::Policy(PolicyError::Revoked))
    ));
    assert!(
        store
            .install_policy(Area::Workshop, new_revision)
            .await
            .expect("idempotent install")
            .revoked
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn expiry_retains_all_reservations_and_recovery_never_requeues_unknown_effects() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::General, 1).await;
    bind_work(&db, &store, &claim, [2; 32]).await;
    let reserved = request(&claim, revision, 100_000);
    let begun = request(&claim, revision, 100_000);
    store
        .reserve(&claim, &reserved, [2; 32])
        .await
        .expect("reserve");
    store
        .reserve(&claim, &begun, [2; 32])
        .await
        .expect("reserve");
    store
        .begin(&claim, attempt_id(&begun))
        .await
        .expect("begin");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(
        store.publish(&claim, KnownOutcome::Verified, [2; 32]).await,
        Err(StoreError::Fenced)
    ));
    assert_eq!(
        store
            .reap_expired(Area::General, 100)
            .await
            .expect("recover"),
        1
    );
    assert_eq!(
        store
            .reap_expired(Area::General, 100)
            .await
            .expect("idempotent recover"),
        0
    );
    assert_eq!(
        store
            .attempt(attempt_id(&reserved))
            .await
            .expect("attempt")
            .state,
        AttemptState::Uncertain
    );
    assert_eq!(
        store
            .attempt(attempt_id(&begun))
            .await
            .expect("attempt")
            .state,
        AttemptState::Uncertain
    );
    assert_eq!(
        store
            .current_budget(Area::General)
            .await
            .expect("budget")
            .1
            .reserved_usd_micros,
        200_000
    );
    assert!(
        store
            .claim(Area::General, claim.action_id(), Uuid::new_v4(), 30)
            .await
            .is_err()
    );
    assert!(
        store
            .reconcile(
                Area::General,
                claim.action_id(),
                KnownOutcome::Failed,
                [2; 32]
            )
            .await
            .is_err()
    );
    store
        .settle(attempt_id(&reserved), UsdMicros(0), [3; 32])
        .await
        .expect("known unsent");
    store
        .settle(attempt_id(&begun), UsdMicros(42), [4; 32])
        .await
        .expect("usage proof");
    let restarted = SupervisorStore::new(db.pool.clone());
    restarted
        .reconcile(
            Area::General,
            claim.action_id(),
            KnownOutcome::Failed,
            [5; 32],
        )
        .await
        .expect("reconcile");
    assert_eq!(
        restarted
            .current_budget(Area::General)
            .await
            .expect("budget")
            .1
            .charged_usd_micros,
        42
    );
    assert!(restarted.begin(&claim, attempt_id(&begun)).await.is_err());
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn over_bound_settlement_records_full_u64_charge_and_revokes_current_policy_atomically() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Homelab, 120).await;
    bind_work(&db, &store, &claim, [2; 32]).await;
    let request = request(&claim, revision, 1);
    store
        .reserve(&claim, &request, [2; 32])
        .await
        .expect("reserve");
    store
        .begin(&claim, attempt_id(&request))
        .await
        .expect("begin");
    let current = Uuid::new_v4();
    store
        .install_policy(Area::Homelab, current)
        .await
        .expect("rotate");
    store
        .set_enabled(Area::Homelab, current, true)
        .await
        .expect("enable");
    let charge = UsdMicros(u64::MAX);
    assert_eq!(
        store
            .settle(attempt_id(&request), charge, [3; 32])
            .await
            .expect("breach"),
        Settlement::ExceededReservation
    );
    assert!(store.policy(Area::Homelab).await.expect("policy").revoked);
    assert_eq!(
        store
            .current_budget(Area::Homelab)
            .await
            .expect("budget")
            .1
            .charged_usd_micros,
        u128::from(u64::MAX)
    );
    assert_eq!(
        store
            .settle(attempt_id(&request), charge, [3; 32])
            .await
            .expect("duplicate"),
        Settlement::AlreadySettled
    );
    assert!(matches!(
        store
            .settle(attempt_id(&request), UsdMicros(0), [3; 32])
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store.settle(attempt_id(&request), charge, [4; 32]).await,
        Err(StoreError::Conflict)
    ));
    assert!(
        sqlx::query("UPDATE supervisor_settlements SET actual_usd_micros=0")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM supervisor_attempts")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE supervisor_attempt_states SET state='reserved'")
            .execute(&db.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE supervisor_policies SET revoked=false WHERE revoked")
            .execute(&db.pool)
            .await
            .is_err()
    );
    let events = store.journal(Area::Homelab, 0, 100).await.expect("journal");
    assert!(
        events
            .iter()
            .any(|entry| entry.event == "settled_over_bound")
    );
    assert_eq!(events.last().expect("event").event, "policy_revoked");
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn journal_cursor_cannot_pass_uncommitted_same_area_entry_and_other_area_progresses() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Trust, 120).await;
    let (_, other) = setup(&store, Area::General, 120).await;
    let before = store
        .journal(Area::Trust, 0, 100)
        .await
        .expect("journal")
        .last()
        .expect("entry")
        .sequence;
    let mut barrier = db.pool.begin().await.expect("barrier");
    sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area='trust' FOR UPDATE")
        .fetch_one(&mut *barrier)
        .await
        .expect("lock");
    let sequence:i64=sqlx::query_scalar("INSERT INTO supervisor_journal(area,policy_revision,event) VALUES('trust',$1,'policy_paused') RETURNING sequence").bind(revision).fetch_one(&mut *barrier).await.expect("uncommitted event");
    let cloned = store.clone();
    let action_id = claim.action_id();
    let mut writer = tokio::spawn(async move {
        cloned
            .stop(Area::Trust, action_id, StopReason::Cancelled, [4; 32])
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut writer)
            .await
            .is_err()
    );
    assert!(
        store
            .journal(Area::Trust, before, 100)
            .await
            .expect("stable cursor")
            .is_empty()
    );
    tokio::time::timeout(
        Duration::from_secs(2),
        store.publish(&other, KnownOutcome::Verified, [4; 32]),
    )
    .await
    .expect("independent area")
    .expect("publish");
    barrier.commit().await.expect("commit first");
    writer.await.expect("writer").expect("stop");
    let entries = store
        .journal(Area::Trust, before, 100)
        .await
        .expect("read committed suffix");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].sequence, sequence);
    assert!(entries[1].sequence > entries[0].sequence);
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn begin_waiting_on_journal_rechecks_expiry_and_cannot_issue_late_permit() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Trust, 1).await;
    bind_work(&db, &store, &claim, [2; 32]).await;
    let request = request(&claim, revision, 50);
    store
        .reserve(&claim, &request, [2; 32])
        .await
        .expect("reserve");
    let mut barrier = db.pool.begin().await.expect("barrier");
    sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area='trust' FOR UPDATE")
        .fetch_one(&mut *barrier)
        .await
        .expect("lock");
    let cloned = store.clone();
    let id = attempt_id(&request);
    let begin = tokio::spawn(async move { cloned.begin(&claim, id).await });
    tokio::time::sleep(Duration::from_millis(1100)).await;
    barrier.commit().await.expect("release");
    assert!(matches!(
        begin.await.expect("join"),
        Err(StoreError::Fenced)
    ));
    assert_eq!(
        store.attempt(id).await.expect("rolled back").state,
        AttemptState::Reserved
    );
    assert_eq!(
        store
            .reap_expired(Area::Trust, 100)
            .await
            .expect("recovery"),
        1
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn accounting_day_is_db_utc_and_prior_day_attempt_cannot_begin() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    let (revision, claim) = setup(&store, Area::Workshop, 120).await;
    bind_work(&db, &store, &claim, [1; 32]).await;
    let row = sqlx::query("SELECT (clock_timestamp() AT TIME ZONE 'UTC')::date AS day")
        .fetch_one(&db.pool)
        .await
        .expect("clock");
    let day: chrono::NaiveDate = row.get("day");
    assert_eq!(
        store
            .current_budget(Area::Workshop)
            .await
            .expect("budget")
            .0,
        day
    );
    let previous = day.pred_opt().expect("prior day");
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO supervisor_budget_days(area,day) VALUES('workshop',$1)")
        .bind(previous)
        .execute(&db.pool)
        .await
        .expect("prior fixture");
    sqlx::query("INSERT INTO supervisor_attempts(id,area,action_id,policy_revision,fence,day,model,request_digest,worst_usd_micros) VALUES($1,'workshop',$2,$3,$4,$5,'gpt-5.6-sol',$6,100)")
        .bind(id).bind(claim.action_id()).bind(revision).bind(claim.fence()).bind(previous).bind(vec![1_u8;32]).execute(&db.pool).await.expect("prior fixture");
    sqlx::query("INSERT INTO supervisor_attempt_states(attempt_id) VALUES($1)")
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("state");
    assert!(matches!(
        store.begin(&claim, id).await,
        Err(StoreError::Budget(BudgetError::WrongDay))
    ));
    assert_eq!(
        store
            .budget(Area::Workshop, previous)
            .await
            .expect("prior budget")
            .calls,
        1
    );
    assert_eq!(
        store
            .current_budget(Area::Workshop)
            .await
            .expect("current budget")
            .1
            .calls,
        0
    );
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn delayed_renew_publication_and_reservation_roll_back_when_lease_expires() {
    let Some(db) = IsolatedDatabase::new().await else {
        return;
    };
    let store = SupervisorStore::new(db.pool.clone());
    for (mode, area, name) in [
        (0, Area::Trust, "trust"),
        (1, Area::General, "general"),
        (2, Area::Workshop, "workshop"),
    ] {
        let (revision, claim) = setup(&store, area, 1).await;
        if area == Area::Workshop {
            bind_work(&db, &store, &claim, [2; 32]).await;
        }
        let id = claim.action_id();
        let original_fence = claim.fence();
        let request = request(&claim, revision, 50);
        let mut barrier = db.pool.begin().await.expect("barrier");
        sqlx::query("SELECT area FROM supervisor_journal_heads WHERE area=$1 FOR UPDATE")
            .bind(name)
            .fetch_one(&mut *barrier)
            .await
            .expect("lock");
        let cloned = store.clone();
        let operation = tokio::spawn(async move {
            match mode {
                0 => cloned.renew(&claim, 60).await.map(|_| ()),
                1 => {
                    cloned
                        .publish(&claim, KnownOutcome::Verified, [1; 32])
                        .await
                }
                _ => cloned.reserve(&claim, &request, [2; 32]).await.map(|_| ()),
            }
        });
        tokio::time::sleep(Duration::from_millis(1100)).await;
        barrier.commit().await.expect("release");
        assert!(matches!(
            operation.await.expect("join"),
            Err(StoreError::Fenced)
        ));
        let action = store.action(area, id).await.expect("action");
        assert_eq!(action.state, ActionState::Running);
        assert_eq!(action.fence, original_fence);
        assert_eq!(store.current_budget(area).await.expect("budget").1.calls, 0);
        assert_eq!(store.reap_expired(area, 100).await.expect("recovery"), 1);
    }
    db.cleanup().await.expect("cleanup");
}
