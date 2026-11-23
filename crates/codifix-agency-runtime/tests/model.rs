#![allow(clippy::unwrap_used)]
//! Fake-provider/worker/Postgres tests. Only synthetic keys enter child envs;
//! local HTTP providers are trusted test construction, never production config.
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{Duration, Utc};
use codifix_agency::{
    routing::{ModelEvaluation, ModelProfile, RoutingRequest},
    *,
};
use codifix_agency_runtime::{
    config::{Config, ModelEndpoint, Peer},
    model,
    store::{Store, StoredTask, Submit},
    worker,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tokio::{sync::Notify, task::JoinHandle};
use uuid::Uuid;
const CHILD: &str = "CODIFIX_MODEL_DB_TEST_CHILD";
const TOKEN_ENV: &str = "CODIFIX_MODEL_DB_TEST_TOKEN";
const MISSING_ENV: &str = "CODIFIX_MODEL_DB_TEST_MISSING_TOKEN";
const TOKEN: &str = "synthetic-model-capability-with-no-live-permissions";
const PRIVATE: &str = "Private synthetic prompt; draft only, no actions performed.";
const RESERVE: u64 = 50;
fn child(test: &str) -> bool {
    if std::env::var(CHILD).ok().as_deref() == Some(test) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, test)
        .env(TOKEN_ENV, TOKEN)
        .env_remove(MISSING_ENV)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child stdout:\n{}\nchild stderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}
fn local() -> ModelDestination {
    ModelDestination::Local {
        runtime: "synthetic-local".into(),
    }
}
fn cloud() -> ModelDestination {
    ModelDestination::Cloud {
        provider: "synthetic-provider".into(),
        region: "test-region".into(),
    }
}
fn both() -> BTreeSet<ModelDestination> {
    BTreeSet::from([local(), cloud()])
}
#[derive(Clone, Copy)]
enum Mode {
    Valid,
    HttpFailure,
    MalformedEnvelope,
    MalformedArtifact,
    MissingUsage,
    NegativeUsage,
    InputExceeded,
    OutputExceeded,
    TotalExceeded,
    CostExceeded,
    Oversized,
    Block,
}
struct Provider {
    mode: Mode,
    calls: Vec<(String, Value)>,
    denied: usize,
    release: Arc<Notify>,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            mode: Mode::Valid,
            calls: vec![],
            denied: 0,
            release: Arc::new(Notify::new()),
        }
    }
}
async fn provider(
    State(state): State<Arc<Mutex<Provider>>>,
    Path(destination): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let (mode, release) = {
        let mut state = state.lock().unwrap();
        if headers.get("authorization").and_then(|h| h.to_str().ok())
            != Some("Bearer synthetic-model-capability-with-no-live-permissions")
        {
            state.denied += 1;
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error":"invalid fixture credential"})),
            )
                .into_response();
        }
        state.calls.push((destination, body));
        (state.mode, state.release.clone())
    };
    if matches!(mode, Mode::Block) {
        release.notified().await;
    }
    if matches!(mode, Mode::HttpFailure) {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":"synthetic acceptance response lost"})),
        )
            .into_response();
    }
    if matches!(mode, Mode::MalformedEnvelope) {
        return (StatusCode::OK, "{not-json").into_response();
    }
    let artifact = if matches!(mode, Mode::MalformedArtifact) {
        "Plain prose, not a JSON artifact".into()
    } else if matches!(mode, Mode::Oversized) {
        "x".repeat(270000)
    } else {
        json!({"draft":"Synthetic private draft","actions_performed":false}).to_string()
    };
    let input = match mode {
        Mode::InputExceeded => json!(129),
        Mode::TotalExceeded => json!(128),
        Mode::NegativeUsage => json!(-1),
        _ => json!(8),
    };
    let output = if matches!(mode, Mode::OutputExceeded) {
        17
    } else {
        2
    };
    let mut response = json!({"choices":[{"message":{"content":artifact}}],"usage":{"prompt_tokens":input,"completion_tokens":output}});
    if matches!(mode, Mode::MissingUsage) {
        response.as_object_mut().unwrap().remove("usage");
    }
    Json(response).into_response()
}
fn profile(id: &str, destination: ModelDestination, cost: u64) -> ModelProfile {
    ModelProfile {
        id: id.into(),
        model_revision: format!("synthetic-{id}-v1"),
        destination,
        accepted_locally: true,
        context_tokens: 128,
        local_memory_mib: if id == "local" { 1024 } else { 0 },
        evaluations: vec![ModelEvaluation {
            evidence_id: format!("held-out-{id}-evaluation"),
            model_revision: format!("synthetic-{id}-v1"),
            workload: "private-draft".into(),
            evaluated_at: Utc::now() - Duration::minutes(10),
            valid_until: Utc::now() + Duration::hours(1),
            held_out_cases: 40,
            quality_ppm: 950000,
            micro_usd_per_call: cost,
            latency_ms: 50,
        }],
    }
}
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    operator: Peer,
    grant: Mandate,
    provider: Arc<Mutex<Provider>>,
    server: JoinHandle<()>,
}
impl Fixture {
    async fn new(destinations: BTreeSet<ModelDestination>) -> Option<Self> {
        let db = codifix_test_support::IsolatedDatabase::new().await?;
        let store = Store {
            pool: db.pool.clone(),
        };
        let instance = Uuid::new_v4();
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'model provider fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
        let grant = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: instance,
            subject_instance_id: instance,
            specialist_id: "drafting".into(),
            capabilities: BTreeSet::from(["model_draft".into()]),
            resources: BTreeSet::from([ResourceScope {
                kind: "document".into(),
                id: "private-fixture".into(),
            }]),
            destinations,
            expires_at: Utc::now() + Duration::hours(1),
            status: MandateStatus::Active,
            budget: Budget {
                micro_usd: 10000,
                calls: 100,
                turns: 100,
                delegation_depth: 2,
            },
        };
        let operator = Peer {
            id: "local-test-operator".into(),
            instance_id: instance,
            token_env: "UNREAD_MODEL_TEST_INBOUND".into(),
            operator: true,
            mandates: BTreeSet::from([grant.id]),
            endpoint: None,
            outbound_token_env: None,
        };
        store
            .install_mandate(project, &grant, &operator, instance)
            .await
            .unwrap();
        let provider_state = Arc::new(Mutex::new(Provider::default()));
        let router = Router::new()
            .route("/{destination}", post(provider))
            .with_state(provider_state.clone());
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", tcp.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(tcp, router).await.unwrap();
        });
        let profiles = vec![profile("local", local(), 2), profile("cloud", cloud(), 10)];
        let cfg = Config {
            observation_enabled: false,
            inbox_enabled: false,
            instance_id: instance,
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: "https://agency.example.test".into(),
            database_env: "UNREAD_MODEL_TEST_DATABASE".into(),
            specialists: vec![SpecialistManifest {
                schema_version: 1,
                instance_id: instance,
                specialist_id: "drafting".into(),
                name: "Private drafting fixture".into(),
                capabilities: grant.capabilities.clone(),
            }],
            peers: vec![operator.clone()],
            models: profiles
                .iter()
                .map(|p| ModelEndpoint {
                    profile_id: p.id.clone(),
                    url: format!("{base}/{}", p.id),
                    token_env: Some(TOKEN_ENV.into()),
                    model: p.model_revision.clone(),
                    max_input_bytes: 1024,
                    max_output_tokens: 16,
                    reserve_micro_usd: RESERVE,
                    input_micro_usd_per_million: 1000,
                    output_micro_usd_per_million: 2000,
                })
                .collect(),
            model_profiles: profiles,
            public_profile: None,
            private_repository_dids: vec![],
            linear: None,
            social: None,
            openbao_controllers: BTreeMap::new(),
        };
        Some(Self {
            db,
            store,
            cfg,
            operator,
            grant,
            provider: provider_state,
            server,
        })
    }
    fn request(&self, permitted: BTreeSet<ModelDestination>) -> Submit {
        Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: self.grant.id,
            specialist_id: "drafting".into(),
            objective: "Draft a private synthetic artifact".into(),
            acceptance_criteria: vec!["model_output_valid_json".into()],
            capability: "model_draft".into(),
            resource: ResourceScope {
                kind: "document".into(),
                id: "private-fixture".into(),
            },
            deadline: Utc::now() + Duration::minutes(10),
            budget: Budget {
                micro_usd: 100,
                calls: 3,
                turns: 4,
                delegation_depth: 1,
            },
            payload: json!({"kind":"model_draft","prompt":PRIVATE,"routing":RoutingRequest{workload:"private-draft".into(),minimum_quality_ppm:900000,minimum_held_out_cases:20,context_tokens:64,max_latency_ms:200,available_local_memory_mib:2048,permitted_destinations:permitted}}),
            parent_id: None,
        }
    }
    async fn execute(&self, request: &Submit) -> StoredTask {
        let task = self.store.submit(request, &self.operator).await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_secs(8),
                worker::run_once(&self.store, &self.cfg)
            )
            .await
            .expect("bounded fake model worker")
            .unwrap()
        );
        self.store.get_internal(task.contract.id).await.unwrap()
    }
    async fn working(&self, request: &Submit) -> StoredTask {
        let mut task = self.store.submit(request, &self.operator).await.unwrap();
        task.contract
            .transition(TaskStatus::Working, Utc::now())
            .unwrap();
        self.store.update_contract(&task).await.unwrap();
        self.store.get_internal(task.contract.id).await.unwrap()
    }
    async fn evidence(&self, id: Uuid) -> Value {
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(id)
            .fetch_one(&self.db.pool)
            .await
            .unwrap()
    }
    async fn no_evidence(&self, id: Uuid) {
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_outcomes WHERE task_id=$1")
                .bind(id)
                .fetch_one(&self.db.pool)
                .await
                .unwrap(),
            0
        );
    }
    async fn effect(&self, id: Uuid) -> String {
        sqlx::query_scalar("SELECT status FROM external_effects WHERE id IN (SELECT id FROM agency_attempts WHERE task_id=$1)").bind(id).fetch_one(&self.db.pool).await.unwrap()
    }
    async fn cleanup(self) {
        self.provider.lock().unwrap().release.notify_waiters();
        self.server.abort();
        self.db.cleanup().await.unwrap();
    }
}

#[tokio::test]
async fn evaluated_model_routing_uses_current_quality_and_measured_cost() {
    if !child("evaluated_model_routing_uses_current_quality_and_measured_cost") {
        return;
    }
    for case in [
        "cheapest-local",
        "newer-regression",
        "expired",
        "not-accepted",
        "wrong-revision",
        "insufficient-memory",
    ] {
        let Some(mut f) = Fixture::new(both()).await else {
            return;
        };
        let mut request = f.request(both());
        match case {
            "newer-regression" => {
                let mut e = f.cfg.model_profiles[0].evaluations[0].clone();
                e.evidence_id = "newer-regression".into();
                e.evaluated_at = Utc::now() - Duration::minutes(1);
                e.quality_ppm = 100000;
                f.cfg.model_profiles[0].evaluations.push(e);
            }
            "expired" => {
                f.cfg.model_profiles[0].evaluations[0].valid_until =
                    Utc::now() - Duration::seconds(1)
            }
            "not-accepted" => f.cfg.model_profiles[0].accepted_locally = false,
            "wrong-revision" => {
                f.cfg.model_profiles[0].evaluations[0].model_revision = "other-model-v0".into()
            }
            "insufficient-memory" => {
                request.payload["routing"]["available_local_memory_mib"] = json!(512)
            }
            _ => {}
        };
        let completed = f.execute(&request).await;
        assert_eq!(completed.contract.status, TaskStatus::Completed, "{case}");
        let expected = if case == "cheapest-local" {
            "local"
        } else {
            "cloud"
        };
        let evidence = f.evidence(completed.contract.id).await;
        assert_eq!(evidence["route"]["profile_id"], expected);
        assert_eq!(
            evidence["route"]["model_revision"],
            format!("synthetic-{expected}-v1")
        );
        assert_eq!(
            evidence["route"]["evaluation_id"],
            format!("held-out-{expected}-evaluation")
        );
        assert_eq!(evidence["artifact"]["actions_performed"], false);
        assert_eq!(evidence["reserved_micro_usd"], RESERVE);
        assert_eq!(evidence["provider_usage"]["prompt_tokens"], 8);
        assert_eq!(completed.used_micro_usd, RESERVE);
        assert_eq!(completed.used_calls, 1);
        assert_eq!(f.effect(completed.contract.id).await, "confirmed");
        {
            let provider = f.provider.lock().unwrap();
            assert_eq!(provider.calls.len(), 1);
            let (destination, body) = &provider.calls[0];
            assert_eq!(destination, expected);
            assert_eq!(body["model"], format!("synthetic-{expected}-v1"));
            assert_eq!(body["max_tokens"], 16);
            assert_eq!(body["stream"], false);
            assert_eq!(body["messages"][1]["content"], PRIVATE);
            assert!(!body.to_string().contains(TOKEN));
        }
        assert!(!evidence.to_string().contains(TOKEN));
        f.cleanup().await;
    }
}
#[tokio::test]
async fn cloud_and_local_disclosure_require_both_mandate_and_task_permission() {
    if !child("cloud_and_local_disclosure_require_both_mandate_and_task_permission") {
        return;
    }
    for (allowed, permitted, expected) in [
        (BTreeSet::from([local()]), BTreeSet::from([cloud()]), None),
        (BTreeSet::from([cloud()]), BTreeSet::from([local()]), None),
        (both(), BTreeSet::from([local()]), Some("local")),
        (BTreeSet::from([cloud()]), both(), Some("cloud")),
    ] {
        let Some(mut f) = Fixture::new(allowed).await else {
            return;
        };
        f.cfg.model_profiles[1].evaluations[0].micro_usd_per_call = 1;
        assert!(
            f.cfg.validate().is_err(),
            "production cloud endpoints must require HTTPS even for loopback"
        );
        let task = f.execute(&f.request(permitted)).await;
        if let Some(expected) = expected {
            assert_eq!(task.contract.status, TaskStatus::Completed);
            assert_eq!(
                f.evidence(task.contract.id).await["route"]["profile_id"],
                expected
            );
            assert_eq!(f.provider.lock().unwrap().calls[0].0, expected);
        } else {
            assert_eq!(task.contract.status, TaskStatus::InputRequired);
            assert!(f.provider.lock().unwrap().calls.is_empty());
            assert_eq!(task.used_micro_usd, 0);
            f.no_evidence(task.contract.id).await;
        }
        f.cleanup().await;
    }
}
#[tokio::test]
async fn malformed_provider_artifacts_usage_and_costs_never_complete_a_task() {
    if !child("malformed_provider_artifacts_usage_and_costs_never_complete_a_task") {
        return;
    }
    for mode in [
        Mode::HttpFailure,
        Mode::MalformedEnvelope,
        Mode::MalformedArtifact,
        Mode::MissingUsage,
        Mode::NegativeUsage,
        Mode::InputExceeded,
        Mode::OutputExceeded,
        Mode::CostExceeded,
        Mode::Oversized,
    ] {
        let Some(mut f) = Fixture::new(BTreeSet::from([local()])).await else {
            return;
        };
        f.provider.lock().unwrap().mode = mode;
        if matches!(mode, Mode::CostExceeded) {
            f.cfg.models[0].reserve_micro_usd = 1;
            f.cfg.models[0].input_micro_usd_per_million = 1000000;
            f.cfg.models[0].output_micro_usd_per_million = 1000000;
        }
        let task = f.execute(&f.request(BTreeSet::from([local()]))).await;
        assert_eq!(task.contract.status, TaskStatus::InputRequired);
        assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
        assert_eq!(task.used_micro_usd, f.cfg.models[0].reserve_micro_usd);
        assert_eq!(task.used_calls, 1);
        assert_eq!(f.effect(task.contract.id).await, "uncertain");
        f.no_evidence(task.contract.id).await;
        f.cleanup().await;
    }
}
#[tokio::test]
async fn total_provider_input_and_output_must_fit_the_evaluated_context_window() {
    if !child("total_provider_input_and_output_must_fit_the_evaluated_context_window") {
        return;
    }
    let Some(f) = Fixture::new(BTreeSet::from([local()])).await else {
        return;
    };
    f.provider.lock().unwrap().mode = Mode::TotalExceeded;
    let task = f.execute(&f.request(BTreeSet::from([local()]))).await;
    assert_eq!(
        task.contract.status,
        TaskStatus::InputRequired,
        "128 input + 2 output exceeds a total native context window of 128"
    );
    assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
    assert_eq!(f.effect(task.contract.id).await, "uncertain");
    f.no_evidence(task.contract.id).await;
    f.cleanup().await;
}
#[tokio::test]
async fn task_and_shared_mandate_budget_exhaustion_prevents_any_provider_send() {
    if !child("task_and_shared_mandate_budget_exhaustion_prevents_any_provider_send") {
        return;
    }
    for case in [
        "task-money",
        "task-calls",
        "task-turns",
        "mandate-money",
        "prompt-bytes",
        "missing-credential",
    ] {
        let Some(mut f) = Fixture::new(BTreeSet::from([local()])).await else {
            return;
        };
        let mut request = f.request(BTreeSet::from([local()]));
        if case == "task-money" {
            request.budget.micro_usd = RESERVE - 1;
        }
        if case == "task-calls" {
            request.budget.calls = 0;
        }
        if case == "prompt-bytes" {
            f.cfg.models[0].max_input_bytes = PRIVATE.len() - 1;
        }
        if case == "missing-credential" {
            f.cfg.models[0].token_env = Some(MISSING_ENV.into());
        }
        let submitted = f.store.submit(&request, &f.operator).await.unwrap();
        if case == "task-turns" {
            sqlx::query("UPDATE agency_tasks SET used_turns=$2 WHERE id=$1")
                .bind(submitted.contract.id)
                .bind(i64::from(request.budget.turns))
                .execute(&f.db.pool)
                .await
                .unwrap();
        }
        if case == "mandate-money" {
            sqlx::query("UPDATE agency_mandates SET used_microusd=$2 WHERE id=$1")
                .bind(f.grant.id)
                .bind(f.grant.budget.micro_usd as i64)
                .execute(&f.db.pool)
                .await
                .unwrap();
        }
        assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
        let task = f.store.get_internal(submitted.contract.id).await.unwrap();
        assert_eq!(task.contract.status, TaskStatus::InputRequired, "{case}");
        assert!(f.provider.lock().unwrap().calls.is_empty(), "{case}");
        f.no_evidence(task.contract.id).await;
        // Missing credentials are conservatively debited after admission. Other
        // policy/input failures must not spend a physical-call reservation.
        assert_eq!(
            task.used_micro_usd,
            if case == "missing-credential" {
                RESERVE
            } else {
                0
            },
            "{case}"
        );
        f.cleanup().await;
    }
}
#[tokio::test]
async fn same_model_attempt_has_one_physical_send_even_when_outcome_is_uncertain() {
    if !child("same_model_attempt_has_one_physical_send_even_when_outcome_is_uncertain") {
        return;
    }
    for uncertain in [false, true] {
        let Some(f) = Fixture::new(BTreeSet::from([local()])).await else {
            return;
        };
        if uncertain {
            f.provider.lock().unwrap().mode = Mode::HttpFailure;
        }
        let request = f.request(BTreeSet::from([local()]));
        let task = f.working(&request).await;
        let routing: RoutingRequest =
            serde_json::from_value(request.payload["routing"].clone()).unwrap();
        let (a, b) = tokio::join!(
            model::draft(&f.store, &f.cfg, &task, PRIVATE, &routing),
            model::draft(&f.store, &f.cfg, &task, PRIVATE, &routing)
        );
        if uncertain {
            assert!(a.is_err() && b.is_err());
        } else {
            assert_ne!(a.is_ok(), b.is_ok());
        }
        assert!(
            model::draft(&f.store, &f.cfg, &task, PRIVATE, &routing)
                .await
                .is_err()
        );
        assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
        let used = f.store.get_internal(task.contract.id).await.unwrap();
        assert_eq!(used.used_micro_usd, RESERVE);
        assert_eq!(used.used_calls, 1);
        assert_eq!(used.used_turns, 1);
        assert_eq!(
            f.effect(task.contract.id).await,
            if uncertain { "uncertain" } else { "confirmed" }
        );
        f.cleanup().await;
    }
}
#[tokio::test]
async fn crashed_model_worker_reclaims_lease_without_resending_an_unknown_attempt() {
    if !child("crashed_model_worker_reclaims_lease_without_resending_an_unknown_attempt") {
        return;
    }
    let Some(f) = Fixture::new(BTreeSet::from([local()])).await else {
        return;
    };
    f.provider.lock().unwrap().mode = Mode::Block;
    let task = f
        .store
        .submit(&f.request(BTreeSet::from([local()])), &f.operator)
        .await
        .unwrap();
    let store = f.store.clone();
    let cfg = f.cfg.clone();
    let running = tokio::spawn(async move { worker::run_once(&store, &cfg).await });
    let mut sent = false;
    for _ in 0..250 {
        if f.provider.lock().unwrap().calls.len() == 1 {
            sent = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(sent);
    running.abort();
    let _ = running.await;
    sqlx::query("UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE operation_id=$1 AND status='running'").bind(task.contract.id).execute(&f.db.pool).await.unwrap();
    sqlx::query("UPDATE background_project_slots SET lease_until=clock_timestamp()-interval '1 second' WHERE job_id IN (SELECT id FROM background_jobs WHERE operation_id=$1) AND job_id IS NOT NULL").bind(task.contract.id).execute(&f.db.pool).await.unwrap();
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    let stopped = f.store.get_internal(task.contract.id).await.unwrap();
    assert_eq!(stopped.contract.status, TaskStatus::InputRequired);
    assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
    assert_eq!(stopped.used_micro_usd, RESERVE);
    assert_eq!(stopped.used_calls, 1);
    assert_eq!(f.effect(task.contract.id).await, "pending");
    f.no_evidence(task.contract.id).await;
    f.cleanup().await;
}
