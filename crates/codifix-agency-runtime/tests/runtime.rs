#![allow(clippy::unwrap_used)]
use chrono::{Duration, Utc};
use codifix_agency::*;
use codifix_agency_runtime::{
    config::{Config, Peer},
    store::{Store, Submit},
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    peer: Peer,
    grant: Mandate,
    project: Uuid,
}
async fn fixture() -> Option<Fixture> {
    let db = codifix_test_support::IsolatedDatabase::new().await?;
    let store = Store {
        pool: db.pool.clone(),
    };
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'agency fixture','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&db.pool)
    .await
    .unwrap();
    let grant = Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: instance,
        subject_instance_id: instance,
        specialist_id: "operations".into(),
        capabilities: BTreeSet::from(["inspect".into(), "model_draft".into()]),
        resources: BTreeSet::from([ResourceScope {
            kind: "service".into(),
            id: "fixture".into(),
        }]),
        destinations: BTreeSet::new(),
        expires_at: Utc::now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 1000,
            calls: 100,
            turns: 100,
            delegation_depth: 3,
        },
    };
    let peer = Peer {
        id: "operator".into(),
        instance_id: instance,
        token_env: "AGENCY_FIXTURE_TOKEN".into(),
        operator: true,
        mandates: BTreeSet::from([grant.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    let cfg = Config {
        instance_id: instance,
        listen: "127.0.0.1:0".parse().unwrap(),
        public_url: "https://agency.example.test".into(),
        database_env: "AGENCY_FIXTURE_DB".into(),
        specialists: vec![SpecialistManifest {
            schema_version: 1,
            instance_id: instance,
            specialist_id: "operations".into(),
            name: "Operations".into(),
            capabilities: grant.capabilities.clone(),
        }],
        peers: vec![peer.clone()],
        models: vec![],
        model_profiles: vec![],
        public_profile: None,
        private_repository_dids: vec![],
        linear: None,
        openbao_controllers: BTreeMap::new(),
    };
    store
        .install_mandate(project, &grant, &peer, instance)
        .await
        .unwrap();
    Some(Fixture {
        db,
        store,
        cfg,
        peer,
        grant,
        project,
    })
}
fn submit(f: &Fixture) -> Submit {
    Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: f.grant.id,
        specialist_id: "operations".into(),
        objective: "Observe service state".into(),
        acceptance_criteria: vec!["status_observed".into()],
        capability: "inspect".into(),
        resource: ResourceScope {
            kind: "service".into(),
            id: "fixture".into(),
        },
        deadline: Utc::now() + Duration::minutes(10),
        budget: Budget {
            micro_usd: 100,
            calls: 10,
            turns: 10,
            delegation_depth: 2,
        },
        payload: json!({"kind":"inspect"}),
        parent_id: None,
    }
}
async fn working(f: &Fixture) -> codifix_agency_runtime::store::StoredTask {
    let mut task = f.store.submit(&submit(f), &f.peer).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    f.store.get_internal(task.contract.id).await.unwrap()
}
#[tokio::test]
async fn standing_schedule_initiates_and_completes_verified_work() {
    let Some(f) = fixture().await else { return };
    let req = submit(&f);
    let schedule = Uuid::new_v4();
    sqlx::query("INSERT INTO agency_schedules(id,mandate_id,requester,interval_seconds,next_at,template,enabled) VALUES($1,$2,$3,60,clock_timestamp(),$4,true)").bind(schedule).bind(f.grant.id).bind(&f.peer.id).bind(serde_json::to_value(req).unwrap()).execute(&f.db.pool).await.unwrap();
    codifix_agency_runtime::worker::schedules(&f.store, &f.cfg)
        .await
        .unwrap();
    assert!(
        codifix_agency_runtime::worker::run_once(&f.store, &f.cfg)
            .await
            .unwrap()
    );
    let tasks = f.store.list(&f.peer).await.unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].contract.status, TaskStatus::Completed);
    codifix_agency_runtime::worker::schedules(&f.store, &f.cfg)
        .await
        .unwrap();
    assert_eq!(f.store.list(&f.peer).await.unwrap().len(), 1);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn task_replay_and_ownership_do_not_grant_extra_access() {
    let Some(f) = fixture().await else { return };
    let req = submit(&f);
    let first = f.store.submit(&req, &f.peer).await.unwrap();
    let second = f.store.submit(&req, &f.peer).await.unwrap();
    assert_eq!(first.contract.id, second.contract.id);
    let mut changed = req.clone();
    changed.objective = "different".into();
    assert!(f.store.submit(&changed, &f.peer).await.is_err());
    let mut intruder = f.peer.clone();
    intruder.id = "intruder".into();
    intruder.operator = false;
    intruder.mandates.clear();
    assert!(f.store.get(first.contract.id, &intruder).await.is_err());
    assert!(f.store.submit(&req, &intruder).await.is_err());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn atomic_effect_admission_elects_one_sender_and_debits_once() {
    let Some(f) = fixture().await else { return };
    let task = working(&f).await;
    let effect = codifix_store::effects::EffectIntent {
        id: Uuid::new_v4(),
        project_id: Some(f.project),
        kind: codifix_store::effects::EffectKind::ProviderRequest,
        host_id: "agency".into(),
        resource: "fixture".into(),
        input_fingerprint: "a".repeat(64),
        actor_id: "operations".into(),
        source: "agency-test".into(),
    };
    let usage = Usage {
        micro_usd: 40,
        calls: 1,
        turns: 1,
    };
    let (a, b) = tokio::join!(
        f.store
            .admit_effect(task.contract.id, task.revision, usage, &effect),
        f.store
            .admit_effect(task.contract.id, task.revision, usage, &effect)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let task = f.store.get_internal(task.contract.id).await.unwrap();
    assert_eq!(task.used_micro_usd, 40);
    assert_eq!(task.used_calls, 1);
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn concurrent_task_spending_cannot_exceed_shared_grant() {
    let Some(f) = fixture().await else { return };
    let a = working(&f).await;
    let b = working(&f).await;
    sqlx::query("UPDATE agency_mandates SET used_microusd=950 WHERE id=$1")
        .bind(f.grant.id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let usage = Usage {
        micro_usd: 40,
        ..Usage::default()
    };
    let (a, b) = tokio::join!(
        f.store.authorize(a.contract.id, a.revision, usage),
        f.store.authorize(b.contract.id, b.revision, usage)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn revocation_cancellation_and_waiting_states_prevent_future_effects() {
    let Some(f) = fixture().await else { return };
    let mut task = working(&f).await;
    task.contract
        .clarify("Need a decision".into(), Utc::now())
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    let task = f.store.get_internal(task.contract.id).await.unwrap();
    assert!(
        f.store
            .authorize(task.contract.id, task.revision, Usage::default())
            .await
            .is_err()
    );
    f.store.cancel(task.contract.id, &f.peer).await.unwrap();
    let task = f.store.get_internal(task.contract.id).await.unwrap();
    assert!(
        f.store
            .authorize(task.contract.id, task.revision, Usage::default())
            .await
            .is_err()
    );
    let other = working(&f).await;
    f.store
        .control_mandate(f.grant.id, "revoke", &f.peer)
        .await
        .unwrap();
    assert!(
        f.store
            .authorize(other.contract.id, other.revision, Usage::default())
            .await
            .is_err()
    );
    assert!(f.store.submit(&submit(&f), &f.peer).await.is_err());
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn child_work_obeys_both_parent_task_and_parent_mandate() {
    let Some(f) = fixture().await else { return };
    let parent = f.store.submit(&submit(&f), &f.peer).await.unwrap();
    let mut child = f.grant.clone();
    child.id = Uuid::new_v4();
    child.parent_id = Some(f.grant.id);
    child.budget = Budget {
        micro_usd: 500,
        calls: 20,
        turns: 20,
        delegation_depth: 1,
    };
    f.store
        .install_mandate(f.project, &child, &f.peer, f.cfg.instance_id)
        .await
        .unwrap();
    let mut req = submit(&f);
    req.parent_id = Some(parent.contract.id);
    req.mandate_id = child.id;
    req.budget.micro_usd = 500;
    req.budget.delegation_depth = 1;
    assert!(f.store.submit(&req, &f.peer).await.is_err());
    req.budget.micro_usd = 60;
    req.deadline = parent.request.deadline;
    let first = f.store.submit(&req, &f.peer).await.unwrap();
    req.request_id = Uuid::new_v4().to_string();
    assert!(f.store.submit(&req, &f.peer).await.is_err());
    f.store
        .control_mandate(f.grant.id, "pause", &f.peer)
        .await
        .unwrap();
    let mut first = first;
    first
        .contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&first).await.unwrap();
    let first = f.store.get_internal(first.contract.id).await.unwrap();
    assert!(
        f.store
            .authorize(first.contract.id, first.revision, Usage::default())
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn clarification_is_bounded_idempotent_and_preserves_initial_request_identity() {
    let Some(f) = fixture().await else { return };
    let req = submit(&f);
    let mut task = f.store.submit(&req, &f.peer).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    task.contract
        .clarify("Which plan revision?".into(), Utc::now())
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    let answered = f
        .store
        .respond(
            task.contract.id,
            "answer-1",
            "revision two",
            Some(json!({"kind":"inspect","revision":2})),
            &f.peer,
        )
        .await
        .unwrap();
    let replay = f
        .store
        .respond(
            task.contract.id,
            "answer-1",
            "revision two",
            Some(json!({"kind":"inspect","revision":2})),
            &f.peer,
        )
        .await
        .unwrap();
    assert_eq!(answered.revision, replay.revision);
    assert_eq!(answered.used_turns, 1);
    assert_eq!(
        f.store.submit(&req, &f.peer).await.unwrap().contract.id,
        task.contract.id
    );
    assert!(
        f.store
            .respond(task.contract.id, "answer-1", "different", None, &f.peer)
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn leases_expiry_and_parent_cancellation_stop_future_actions() {
    let Some(f) = fixture().await else { return };
    let mut req = submit(&f);
    req.deadline = f.grant.expires_at + Duration::seconds(1);
    assert!(f.store.submit(&req, &f.peer).await.is_err());
    req.deadline = Utc::now() + Duration::minutes(10);
    req.payload = json!({"kind":"inspect","execution_lease_seconds":30,"source_destinations":[]});
    let mut task = f.store.submit(&req, &f.peer).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    let task = f.store.get_internal(task.contract.id).await.unwrap();
    sqlx::query("UPDATE agency_tasks SET execution_lease_until=clock_timestamp()-interval '1 second' WHERE id=$1").bind(task.contract.id).execute(&f.db.pool).await.unwrap();
    assert!(
        f.store
            .authorize(task.contract.id, task.revision, Usage::default())
            .await
            .is_err()
    );
    assert!(f.store.live_authority(&task).await.is_err());
    f.store
        .renew_execution_lease(task.contract.id, &f.peer)
        .await
        .unwrap();
    f.store
        .authorize(task.contract.id, task.revision, Usage::default())
        .await
        .unwrap();
    let mut child = f.grant.clone();
    child.id = Uuid::new_v4();
    child.parent_id = Some(f.grant.id);
    child.budget = req.budget;
    child.budget.delegation_depth = 1;
    f.store
        .install_mandate(f.project, &child, &f.peer, f.cfg.instance_id)
        .await
        .unwrap();
    let mut child_req = req.clone();
    child_req.request_id = Uuid::new_v4().to_string();
    child_req.parent_id = Some(task.contract.id);
    child_req.mandate_id = child.id;
    child_req.budget.delegation_depth = 1;
    let mut ct = f.store.submit(&child_req, &f.peer).await.unwrap();
    ct.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&ct).await.unwrap();
    let ct = f.store.get_internal(ct.contract.id).await.unwrap();
    f.store
        .authorize(ct.contract.id, ct.revision, Usage::default())
        .await
        .unwrap();
    f.store.cancel(task.contract.id, &f.peer).await.unwrap();
    assert!(
        f.store
            .authorize(ct.contract.id, ct.revision, Usage::default())
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn task_activation_debits_once_and_model_destinations_survive_clarification() {
    let Some(f) = fixture().await else { return };
    let mut req = submit(&f);
    req.payload = json!({"kind":"inspect","source_destinations":[],"delegation_trace":[Uuid::new_v4()],"execution_lease_seconds":30});
    let mut task = f.store.submit(&req, &f.peer).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    let mut task = f.store.get_internal(task.contract.id).await.unwrap();
    f.store
        .start_task(task.contract.id, task.revision)
        .await
        .unwrap();
    f.store
        .start_task(task.contract.id, task.revision)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .get_internal(task.contract.id)
            .await
            .unwrap()
            .used_turns,
        1
    );
    task.contract.clarify("Detail?".into(), Utc::now()).unwrap();
    f.store.update_contract(&task).await.unwrap();
    let answered=f.store.respond(task.contract.id,"response","Yes",Some(json!({"kind":"inspect","source_destinations":[{"kind":"cloud","provider":"any","region":"any"}]})),&f.peer).await.unwrap();
    assert_eq!(answered.request.payload["source_destinations"], json!([]));
    assert_eq!(
        answered.request.payload["delegation_trace"],
        req.payload["delegation_trace"]
    );
    assert!(
        f.store
            .authorize(answered.contract.id, answered.revision, Usage::default())
            .await
            .unwrap()
            .destinations
            .is_empty()
    );
    f.db.cleanup().await.unwrap();
}
#[tokio::test]
async fn orphaned_and_denied_schedules_cannot_starve_valid_work() {
    let Some(f) = fixture().await else { return };
    let template = serde_json::to_value(submit(&f)).unwrap();
    for _ in 0..16 {
        sqlx::query("INSERT INTO agency_schedules(id,mandate_id,requester,interval_seconds,next_at,template,enabled) VALUES($1,$2,'removed-peer',60,clock_timestamp()-interval '60 seconds',$3,true)").bind(Uuid::new_v4()).bind(f.grant.id).bind(&template).execute(&f.db.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO agency_schedules(id,mandate_id,requester,interval_seconds,next_at,template,enabled) VALUES($1,$2,$3,60,clock_timestamp(),$4,true)").bind(Uuid::new_v4()).bind(f.grant.id).bind(&f.peer.id).bind(template).execute(&f.db.pool).await.unwrap();
    codifix_agency_runtime::worker::schedules(&f.store, &f.cfg)
        .await
        .unwrap();
    codifix_agency_runtime::worker::schedules(&f.store, &f.cfg)
        .await
        .unwrap();
    assert_eq!(f.store.list(&f.peer).await.unwrap().len(), 1);
    f.db.cleanup().await.unwrap();
}
