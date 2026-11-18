#![allow(clippy::unwrap_used)]
//! Private conversation worker tests against an authenticated loopback A2A peer.
//! Every fake outbound credential is set before a dedicated child starts; no
//! concurrent test mutates its process environment or contacts a live peer.
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use codifix_agency::{
    a2a::{self, Part},
    *,
};
use codifix_agency_runtime::{
    config::{Config, Peer},
    store::{Store, StoredTask, Submit},
    worker,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

const TOKEN_ENV: &str = "CODIFIX_EXCHANGE_TEST_OUTBOUND";
const WRONG_ENV: &str = "CODIFIX_EXCHANGE_TEST_WRONG";
const MISSING_ENV: &str = "CODIFIX_EXCHANGE_TEST_UNSET";
const CHILD: &str = "CODIFIX_EXCHANGE_TEST_CHILD";
const TOKEN: &str = "synthetic-a2a-outbound-capability-no-live-authority";
const REMOTE: &str = "private-remote-task-012345";
const CONTEXT: &str = "private-remote-context-012345";
const PRIVATE: &str = "private synthetic delivery evidence";

fn child(test: &str) -> bool {
    if std::env::var(CHILD).ok().as_deref() == Some(test) {
        return true;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, test)
        .env(TOKEN_ENV, TOKEN)
        .env(WRONG_ENV, "synthetic-wrong-peer-capability-no-authority")
        .env_remove(MISSING_ENV)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "child stdout:\n{}\nchild stderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    false
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Behavior {
    Ordinary,
    UnknownSubmit,
    UnknownAnswer,
    PollWrongId,
    PollWrongContext,
    AnswerWrongId,
    AnswerWrongContext,
    LeaseCompleted,
    LeaseDeniedActive,
    LeaseCompletedWrongContext,
}
struct Remote {
    behavior: Behavior,
    phase: a2a::TaskState,
    sends: Vec<Value>,
    polls: Vec<String>,
    cancellations: Vec<String>,
    denied: usize,
    renewals: usize,
}
impl Default for Remote {
    fn default() -> Self {
        Self {
            behavior: Behavior::Ordinary,
            phase: a2a::TaskState::Working,
            sends: vec![],
            polls: vec![],
            cancellations: vec![],
            denied: 0,
            renewals: 0,
        }
    }
}
fn authenticate(state: &mut Remote, headers: &HeaderMap) -> bool {
    let valid = headers.get("authorization").and_then(|h| h.to_str().ok())
        == Some(concat!(
            "Bearer ",
            "synthetic-a2a-outbound-capability-no-live-authority"
        ))
        && headers.get("a2a-version").and_then(|h| h.to_str().ok()) == Some("1.0");
    if !valid {
        state.denied += 1;
    }
    valid
}
fn remote_task(id: &str, context: &str, state: a2a::TaskState) -> a2a::Task {
    a2a::Task {
        id: id.into(),
        context_id: context.into(),
        status: a2a::TaskStatus {
            state,
            timestamp: Utc::now(),
            message: if matches!(
                state,
                a2a::TaskState::InputRequired | a2a::TaskState::AuthRequired
            ) {
                Some(a2a::Message {
                    message_id: "private-question-1".into(),
                    role: a2a::Role::Agent,
                    parts: vec![Part::Text("Which reviewed scope should I use?".into())],
                    task_id: Some(id.into()),
                    context_id: Some(context.into()),
                })
            } else {
                None
            },
        },
        artifacts: if state == a2a::TaskState::Completed {
            vec![a2a::Artifact {
                artifact_id: "private-delivery-1".into(),
                parts: vec![Part::Data(json!({"result":PRIVATE}))],
            }]
        } else {
            vec![]
        },
    }
}
async fn send(
    State(state): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let mut state = state.lock().unwrap();
    if !authenticate(&mut state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        );
    }
    let followup = body["message"]["taskId"].is_string();
    state.sends.push(body);
    if (!followup && state.behavior == Behavior::UnknownSubmit)
        || (followup && state.behavior == Behavior::UnknownAnswer)
    {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":"synthetic response lost after acceptance"})),
        );
    }
    let id = if followup && state.behavior == Behavior::AnswerWrongId {
        "unrelated-task"
    } else {
        REMOTE
    };
    let context = if followup && state.behavior == Behavior::AnswerWrongContext {
        "unrelated-context"
    } else {
        CONTEXT
    };
    (
        StatusCode::OK,
        Json(
            serde_json::to_value(a2a::SendMessageResponse::Task(remote_task(
                id,
                context,
                a2a::TaskState::Submitted,
            )))
            .unwrap(),
        ),
    )
}
async fn poll(
    State(state): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let mut state = state.lock().unwrap();
    if !authenticate(&mut state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        );
    }
    state.polls.push(id);
    let id = if state.behavior == Behavior::PollWrongId {
        "unrelated-task"
    } else {
        REMOTE
    };
    let context = if state.behavior == Behavior::PollWrongContext
        || (state.behavior == Behavior::LeaseCompletedWrongContext && state.polls.len() > 1)
    {
        "unrelated-context"
    } else {
        CONTEXT
    };
    let phase = state.phase;
    if matches!(
        state.behavior,
        Behavior::LeaseCompleted | Behavior::LeaseCompletedWrongContext
    ) {
        state.phase = a2a::TaskState::Completed;
    }
    (
        StatusCode::OK,
        Json(serde_json::to_value(remote_task(id, context, phase)).unwrap()),
    )
}
async fn cancel(
    State(state): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
    Path(action): Path<String>,
) -> (StatusCode, Json<Value>) {
    let mut state = state.lock().unwrap();
    if !authenticate(&mut state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        );
    }
    let Some(id) = action.strip_suffix(":cancel") else {
        return (StatusCode::NOT_FOUND, Json(json!({})));
    };
    state.cancellations.push(id.into());
    state.phase = a2a::TaskState::Canceled;
    (
        StatusCode::OK,
        Json(serde_json::to_value(remote_task(REMOTE, CONTEXT, a2a::TaskState::Canceled)).unwrap()),
    )
}
async fn renew(
    State(state): State<Arc<Mutex<Remote>>>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let mut state = state.lock().unwrap();
    if authenticate(&mut state, &headers) {
        state.renewals += 1;
        if matches!(
            state.behavior,
            Behavior::LeaseCompleted
                | Behavior::LeaseCompletedWrongContext
                | Behavior::LeaseDeniedActive
        ) {
            return (
                StatusCode::CONFLICT,
                Json(json!({"error":"task_closed_or_revoked"})),
            );
        }
        (StatusCode::OK, Json(json!({"renewed":true})))
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        )
    }
}
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    operator: Peer,
    grant: Mandate,
    remote: Arc<Mutex<Remote>>,
    server: JoinHandle<()>,
}
impl Fixture {
    async fn new() -> Option<Self> {
        let db = codifix_test_support::IsolatedDatabase::new().await?;
        let store = Store {
            pool: db.pool.clone(),
        };
        let instance = Uuid::new_v4();
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'private exchange fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
        let grant = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: instance,
            subject_instance_id: instance,
            specialist_id: "coordinator".into(),
            capabilities: BTreeSet::from(["delegate".into()]),
            resources: BTreeSet::from([ResourceScope {
                kind: "peer".into(),
                id: "enrolled-peer".into(),
            }]),
            destinations: BTreeSet::new(),
            expires_at: Utc::now() + Duration::hours(1),
            status: MandateStatus::Active,
            budget: Budget {
                micro_usd: 10000,
                calls: 100,
                turns: 100,
                delegation_depth: 4,
            },
        };
        let operator = Peer {
            id: "local-operator".into(),
            instance_id: instance,
            token_env: "UNREAD_EXCHANGE_INBOUND_TOKEN".into(),
            operator: true,
            mandates: BTreeSet::from([grant.id]),
            endpoint: None,
            outbound_token_env: None,
        };
        store
            .install_mandate(project, &grant, &operator, instance)
            .await
            .unwrap();
        let remote = Arc::new(Mutex::new(Remote::default()));
        let router = Router::new()
            .route("/message:send", post(send))
            .route("/v1/tasks/{id}/lease", post(renew))
            .route("/tasks/{id}", get(poll).post(cancel))
            .with_state(remote.clone());
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", tcp.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(tcp, router).await.unwrap();
        });
        let enrolled = Peer {
            id: "enrolled-peer".into(),
            instance_id: Uuid::new_v4(),
            token_env: "UNREAD_REMOTE_INBOUND_TOKEN".into(),
            operator: false,
            mandates: BTreeSet::new(),
            endpoint: Some(endpoint),
            outbound_token_env: Some(TOKEN_ENV.into()),
        };
        // Constructed trusted test fixture only. Config::load/validate reject HTTP peers.
        let cfg = Config {
            observation_enabled: false,
            inbox_enabled: false,
            instance_id: instance,
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: "https://agency.example.test".into(),
            database_env: "UNREAD_EXCHANGE_DB_ENV".into(),
            specialists: vec![SpecialistManifest {
                schema_version: 1,
                instance_id: instance,
                specialist_id: "coordinator".into(),
                name: "Synthetic private coordinator".into(),
                capabilities: grant.capabilities.clone(),
            }],
            peers: vec![operator.clone(), enrolled],
            models: vec![],
            model_profiles: vec![],
            public_profile: None,
            private_repository_dids: vec![],
            linear: None,
            openbao_controllers: BTreeMap::new(),
        };
        Some(Self {
            db,
            store,
            cfg,
            operator,
            grant,
            remote,
            server,
        })
    }
    fn request(&self) -> Submit {
        let deadline = Utc::now() + Duration::minutes(10);
        let nested = Submit {
            request_id: "will-be-replaced-by-stable-local-task-id".into(),
            mandate_id: Uuid::new_v4(),
            specialist_id: "remote-specialist".into(),
            objective: "Private synthetic delegated objective".into(),
            acceptance_criteria: vec!["status_observed".into()],
            capability: "inspect".into(),
            resource: ResourceScope {
                kind: "service".into(),
                id: "synthetic-service".into(),
            },
            deadline,
            budget: Budget {
                micro_usd: 10,
                calls: 2,
                turns: 2,
                delegation_depth: 1,
            },
            payload: json!({"kind":"inspect","private_context":PRIVATE}),
            parent_id: None,
        };
        Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: self.grant.id,
            specialist_id: "coordinator".into(),
            objective: "Delegate private bounded fixture".into(),
            acceptance_criteria: vec!["delegated_result_reported".into()],
            capability: "delegate".into(),
            resource: ResourceScope {
                kind: "peer".into(),
                id: "enrolled-peer".into(),
            },
            deadline,
            budget: Budget {
                micro_usd: 100,
                calls: 10,
                turns: 20,
                delegation_depth: 2,
            },
            payload: json!({"kind":"delegate","peer":"enrolled-peer","request":nested}),
            parent_id: None,
        }
    }
    fn start(&self) -> JoinHandle<anyhow::Result<bool>> {
        let store = self.store.clone();
        let cfg = self.cfg.clone();
        tokio::spawn(async move { worker::run_once(&store, &cfg).await })
    }
    async fn settle(&self, id: Uuid) -> StoredTask {
        for _ in 0..5 {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                worker::run_once(&self.store, &self.cfg),
            )
            .await
            .expect("bounded synthetic worker")
            .unwrap();
            let task = self.store.get_internal(id).await.unwrap();
            if task.contract.status != TaskStatus::Working
                && task.contract.status != TaskStatus::Submitted
            {
                return task;
            }
        }
        panic!("synthetic worker did not settle")
    }
    async fn wait_polls(&self, count: usize) {
        for _ in 0..250 {
            if self.remote.lock().unwrap().polls.len() >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("private mock did not observe expected poll");
    }
    async fn expire_worker(&self, id: Uuid) {
        // Simulate a crashed process and expired durable lease; never synthesize
        // new remote state or clear the physical-attempt guard.
        sqlx::query("UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE operation_id=$1 AND status='running'").bind(id).execute(&self.db.pool).await.unwrap();
        sqlx::query("UPDATE background_project_slots SET lease_until=clock_timestamp()-interval '1 second' WHERE job_id IN (SELECT id FROM background_jobs WHERE operation_id=$1) AND job_id IS NOT NULL").bind(id).execute(&self.db.pool).await.unwrap();
    }
    async fn no_outcome(&self, id: Uuid) {
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_outcomes WHERE task_id=$1")
                .bind(id)
                .fetch_one(&self.db.pool)
                .await
                .unwrap(),
            0
        );
    }
    async fn cleanup(self) {
        self.server.abort();
        self.db.cleanup().await.unwrap();
    }
}

#[tokio::test]
async fn private_delegate_persists_remote_identity_and_worker_restart_only_polls() {
    if !child("private_delegate_persists_remote_identity_and_worker_restart_only_polls") {
        return;
    }
    let Some(f) = Fixture::new().await else {
        return;
    };
    assert!(
        f.cfg.validate().is_err(),
        "production configuration must reject the loopback HTTP test peer"
    );
    let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
    let running = f.start();
    f.wait_polls(1).await;
    let binding: (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT peer_id,remote_id,context_id FROM agency_remote_tasks WHERE task_id=$1",
    )
    .bind(task.contract.id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(
        binding,
        (
            "enrolled-peer".into(),
            Some(REMOTE.into()),
            Some(CONTEXT.into())
        )
    );
    running.abort();
    let _ = running.await;
    f.expire_worker(task.contract.id).await;
    f.remote.lock().unwrap().phase = a2a::TaskState::Completed;
    let finished = f.settle(task.contract.id).await;
    assert_eq!(finished.contract.status, TaskStatus::Completed);
    let evidence: Value =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(evidence["reported_result"]["id"], REMOTE);
    assert_eq!(evidence["reported_result"]["contextId"], CONTEXT);
    assert_eq!(evidence["independent_verification"], false);
    {
        let state = f.remote.lock().unwrap();
        assert_eq!(state.sends.len(), 1);
        assert!(state.polls.len() >= 2);
        assert!(state.polls.iter().all(|id| id == REMOTE));
        let sent: &Value = &state.sends[0];
        assert_eq!(sent["message"]["messageId"], task.contract.id.to_string());
        assert_eq!(
            sent["message"]["parts"][0]["data"]["request_id"],
            task.contract.id.to_string()
        );
        assert_eq!(sent["configuration"]["returnImmediately"], true);
        assert!(!sent.to_string().contains(TOKEN));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_attempts WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        1
    );
    let mut outsider = f.operator.clone();
    outsider.id = "unrelated-private-peer".into();
    outsider.operator = false;
    outsider.mandates.clear();
    assert!(f.store.get(task.contract.id, &outsider).await.is_err());
    assert!(f.store.list(&outsider).await.unwrap().is_empty());
    f.cleanup().await;
}

#[tokio::test]
async fn delegated_input_is_answered_once_with_the_same_remote_conversation() {
    if !child("delegated_input_is_answered_once_with_the_same_remote_conversation") {
        return;
    }
    let Some(f) = Fixture::new().await else {
        return;
    };
    f.remote.lock().unwrap().phase = a2a::TaskState::InputRequired;
    let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
    let waiting = f.settle(task.contract.id).await;
    assert_eq!(waiting.contract.status, TaskStatus::InputRequired);
    assert_eq!(
        waiting.contract.clarification.as_ref().unwrap().question,
        "Which reviewed scope should I use?"
    );
    let answered = f
        .store
        .respond(
            task.contract.id,
            "source-answer-1",
            "Use the reviewed synthetic service only.",
            None,
            &f.operator,
        )
        .await
        .unwrap();
    let duplicate = f
        .store
        .respond(
            task.contract.id,
            "source-answer-1",
            "Use the reviewed synthetic service only.",
            None,
            &f.operator,
        )
        .await
        .unwrap();
    assert_eq!(answered.revision, duplicate.revision);
    assert!(
        f.store
            .respond(
                task.contract.id,
                "source-answer-1",
                "Conflicting answer",
                None,
                &f.operator
            )
            .await
            .is_err()
    );
    f.remote.lock().unwrap().phase = a2a::TaskState::Working;
    let polls = f.remote.lock().unwrap().polls.len();
    let running = f.start();
    f.wait_polls(polls + 1).await;
    let replies: Option<i64> =
        sqlx::query_scalar("SELECT reply_revision FROM agency_remote_tasks WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(replies, Some(answered.revision));
    running.abort();
    let _ = running.await;
    f.expire_worker(task.contract.id).await;
    f.remote.lock().unwrap().phase = a2a::TaskState::Completed;
    assert_eq!(
        f.settle(task.contract.id).await.contract.status,
        TaskStatus::Completed
    );
    {
        let state = f.remote.lock().unwrap();
        assert_eq!(state.sends.len(), 2);
        let reply = &state.sends[1]["message"];
        assert_eq!(reply["taskId"], REMOTE);
        assert_eq!(reply["contextId"], CONTEXT);
        assert_eq!(
            reply["messageId"],
            format!("{}:{}", task.contract.id, answered.revision)
        );
        assert_eq!(
            reply["parts"][0]["data"]["answer"],
            "Use the reviewed synthetic service only."
        );
    }
    f.cleanup().await;
}

#[tokio::test]
async fn delegation_rejects_missing_credentials_and_disclosure_scope_before_submission() {
    if !child("delegation_rejects_missing_credentials_and_disclosure_scope_before_submission") {
        return;
    }
    for case in ["missing", "wrong", "scope", "unenrolled", "self"] {
        let Some(mut f) = Fixture::new().await else {
            return;
        };
        let mut request = f.request();
        match case {
            "missing" => f.cfg.peers[1].outbound_token_env = Some(MISSING_ENV.into()),
            "wrong" => f.cfg.peers[1].outbound_token_env = Some(WRONG_ENV.into()),
            "scope" => request.payload["peer"] = json!("different-peer"),
            "unenrolled" => {
                f.cfg.peers.pop();
            }
            _ => f.cfg.peers[1].instance_id = f.cfg.instance_id,
        };
        let task = f.store.submit(&request, &f.operator).await.unwrap();
        assert_eq!(
            f.settle(task.contract.id).await.contract.status,
            TaskStatus::InputRequired,
            "{case}"
        );
        f.no_outcome(task.contract.id).await;
        {
            let remote = f.remote.lock().unwrap();
            assert!(remote.sends.is_empty(), "{case}");
            assert!(remote.polls.is_empty(), "{case}");
            assert_eq!(remote.denied, usize::from(case == "wrong"));
        }
        f.cleanup().await;
    }
}

#[tokio::test]
async fn mismatched_remote_poll_or_answer_identity_cannot_complete_local_task() {
    if !child("mismatched_remote_poll_or_answer_identity_cannot_complete_local_task") {
        return;
    }
    for behavior in [
        Behavior::PollWrongId,
        Behavior::PollWrongContext,
        Behavior::AnswerWrongId,
        Behavior::AnswerWrongContext,
    ] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        let answer = matches!(
            behavior,
            Behavior::AnswerWrongId | Behavior::AnswerWrongContext
        );
        {
            let mut remote = f.remote.lock().unwrap();
            remote.behavior = behavior;
            remote.phase = if answer {
                a2a::TaskState::InputRequired
            } else {
                a2a::TaskState::Completed
            };
        }
        let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
        assert_eq!(
            f.settle(task.contract.id).await.contract.status,
            TaskStatus::InputRequired
        );
        if answer {
            f.store
                .respond(
                    task.contract.id,
                    "answer-for-peer",
                    "Reviewed fixture scope",
                    None,
                    &f.operator,
                )
                .await
                .unwrap();
            f.remote.lock().unwrap().phase = a2a::TaskState::Completed;
            assert_eq!(
                f.settle(task.contract.id).await.contract.status,
                TaskStatus::InputRequired,
                "mismatched clarification acknowledgement must stop before accepting a later valid poll"
            );
        }
        f.no_outcome(task.contract.id).await;
        f.cleanup().await;
    }
}

#[tokio::test]
async fn unknown_remote_submission_or_answer_outcome_is_never_resent() {
    if !child("unknown_remote_submission_or_answer_outcome_is_never_resent") {
        return;
    }
    for behavior in [Behavior::UnknownSubmit, Behavior::UnknownAnswer] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        let answer = behavior == Behavior::UnknownAnswer;
        {
            let mut remote = f.remote.lock().unwrap();
            remote.behavior = behavior;
            remote.phase = a2a::TaskState::InputRequired;
        }
        let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
        assert_eq!(
            f.settle(task.contract.id).await.contract.status,
            TaskStatus::InputRequired
        );
        if answer {
            f.store
                .respond(
                    task.contract.id,
                    "first-answer",
                    "Reviewed answer accepted but response lost",
                    None,
                    &f.operator,
                )
                .await
                .unwrap();
            assert_eq!(
                f.settle(task.contract.id).await.contract.status,
                TaskStatus::InputRequired
            );
        }
        let sends = f.remote.lock().unwrap().sends.len();
        assert_eq!(sends, if answer { 2 } else { 1 });
        f.store
            .respond(
                task.contract.id,
                "operator-reconcile",
                "Inspect original IDs; do not submit again",
                None,
                &f.operator,
            )
            .await
            .unwrap();
        f.remote.lock().unwrap().phase = a2a::TaskState::Completed;
        let reconciled = f.settle(task.contract.id).await;
        assert_eq!(
            f.remote.lock().unwrap().sends.len(),
            sends,
            "unknown effects must never become another physical send"
        );
        // A known remote task may be reconciled by polling its existing ID, or
        // remain at a private escalation. A lost initial ID cannot be guessed.
        if answer {
            assert!(matches!(
                reconciled.contract.status,
                TaskStatus::InputRequired | TaskStatus::Completed
            ));
        } else {
            assert_eq!(reconciled.contract.status, TaskStatus::InputRequired);
        }
        if reconciled.contract.status != TaskStatus::Completed {
            f.no_outcome(task.contract.id).await;
        }
        f.cleanup().await;
    }
}

#[tokio::test]
async fn revocation_and_local_cancellation_cancel_the_active_remote_task() {
    if !child("revocation_and_local_cancellation_cancel_the_active_remote_task") {
        return;
    }
    for revoke in [true, false] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
        let running = f.start();
        f.wait_polls(1).await;
        if revoke {
            f.store
                .control_mandate(f.grant.id, "revoke", &f.operator)
                .await
                .unwrap();
        } else {
            f.store.cancel(task.contract.id, &f.operator).await.unwrap();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), running)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        );
        let current = f.store.get_internal(task.contract.id).await.unwrap();
        assert_eq!(
            current.contract.status,
            if revoke {
                TaskStatus::InputRequired
            } else {
                TaskStatus::Canceled
            }
        );
        f.no_outcome(task.contract.id).await;
        {
            let remote = f.remote.lock().unwrap();
            assert_eq!(remote.sends.len(), 1);
            assert_eq!(remote.cancellations, vec![REMOTE]);
        }
        f.cleanup().await;
    }
}

#[tokio::test]
async fn lease_completion_race_reconciles_once_without_ignoring_revocation_or_binding() {
    if !child("lease_completion_race_reconciles_once_without_ignoring_revocation_or_binding") {
        return;
    }
    for behavior in [
        Behavior::LeaseCompleted,
        Behavior::LeaseDeniedActive,
        Behavior::LeaseCompletedWrongContext,
    ] {
        let Some(f) = Fixture::new().await else {
            return;
        };
        f.remote.lock().unwrap().behavior = behavior;
        let task = f.store.submit(&f.request(), &f.operator).await.unwrap();
        let done = f.settle(task.contract.id).await;
        assert_eq!(
            done.contract.status,
            if behavior == Behavior::LeaseCompleted {
                TaskStatus::Completed
            } else {
                TaskStatus::InputRequired
            }
        );
        {
            let state = f.remote.lock().unwrap();
            assert_eq!(state.sends.len(), 1);
            assert_eq!(state.renewals, 1);
            assert_eq!(state.polls.len(), 2);
        }
        if behavior != Behavior::LeaseCompleted {
            f.no_outcome(task.contract.id).await;
        }
        f.cleanup().await;
    }
}
