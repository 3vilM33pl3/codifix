#![allow(clippy::unwrap_used)]
//! Synthetic model and authenticated A2A regression tests; no live provider.
use axum::{Json, Router, extract::State, routing::post};
use chrono::{Duration, Utc};
use codifix_agency::{conversation::*, routing::*, *};
use codifix_agency_runtime::{
    config::{Auth, Config, ModelEndpoint, Peer},
    store::{Store, Submit},
    worker,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Default)]
struct Provider {
    calls: Vec<Value>,
    invalid: bool,
}
async fn complete(
    State(state): State<Arc<Mutex<Provider>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut state = state.lock().unwrap();
    state.calls.push(body);
    let reply = if state.invalid {
        json!({"version":1,"reply":"I deployed it","actions":[{"key":"ungranted-deploy","payload":{}}]})
    } else {
        json!({"version":1,"reply":"The roadmap is ready to discuss.","question":"Which milestone comes first?","actions":[],"peer_requests":[]})
    };
    Json(
        json!({"choices":[{"message":{"content":reply.to_string()}}],"usage":{"prompt_tokens":20,"completion_tokens":20}}),
    )
}
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    operator: Peer,
    mandate: Mandate,
    provider: Arc<Mutex<Provider>>,
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
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'inbox conversation fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
        let destination = ModelDestination::Local {
            runtime: "fixture-model".into(),
        };
        let mandate = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: instance,
            subject_instance_id: instance,
            specialist_id: "planner".into(),
            capabilities: BTreeSet::from(["conversation_turn".into(), "delegate".into()]),
            resources: BTreeSet::from([
                ResourceScope {
                    kind: "conversation".into(),
                    id: "fixture".into(),
                },
                ResourceScope {
                    kind: "peer".into(),
                    id: "remote".into(),
                },
            ]),
            destinations: BTreeSet::from([destination.clone()]),
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
            id: "operator".into(),
            instance_id: instance,
            token_env: "UNUSED_INBOX_OPERATOR".into(),
            operator: true,
            mandates: BTreeSet::from([mandate.id]),
            endpoint: None,
            outbound_token_env: None,
        };
        store
            .install_mandate(project, &mandate, &operator, instance)
            .await
            .unwrap();
        let provider = Arc::new(Mutex::new(Provider::default()));
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", tcp.local_addr().unwrap());
        let app = Router::new()
            .route("/", post(complete))
            .with_state(provider.clone());
        let server = tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        });
        let cfg = Config {
            inbox_enabled: false,
            instance_id: instance,
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: "https://agency.invalid".into(),
            database_env: "UNUSED_INBOX_DB".into(),
            specialists: vec![SpecialistManifest {
                schema_version: 1,
                instance_id: instance,
                specialist_id: "planner".into(),
                name: "Planner".into(),
                capabilities: mandate.capabilities.clone(),
            }],
            peers: vec![operator.clone()],
            models: vec![ModelEndpoint {
                profile_id: "fixture".into(),
                url: endpoint,
                token_env: None,
                model: "fixture-v1".into(),
                max_input_bytes: 8192,
                max_output_tokens: 512,
                reserve_micro_usd: 10,
                input_micro_usd_per_million: 1,
                output_micro_usd_per_million: 1,
            }],
            model_profiles: vec![ModelProfile {
                id: "fixture".into(),
                model_revision: "fixture-v1".into(),
                destination,
                accepted_locally: true,
                context_tokens: 4096,
                local_memory_mib: 1024,
                evaluations: vec![ModelEvaluation {
                    evidence_id: "fixture-eval".into(),
                    model_revision: "fixture-v1".into(),
                    workload: "conversation".into(),
                    evaluated_at: Utc::now() - Duration::minutes(1),
                    valid_until: Utc::now() + Duration::hours(1),
                    held_out_cases: 20,
                    quality_ppm: 950000,
                    micro_usd_per_call: 1,
                    latency_ms: 10,
                }],
            }],
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
            mandate,
            provider,
            server,
        })
    }
    fn request(&self, home: Uuid) -> Submit {
        let turn = ConversationTurn {
            version: 1,
            home_instance_id: home,
            conversation_id: Uuid::new_v4(),
            root_message_id: Uuid::new_v4(),
            turn_id: Uuid::new_v4(),
            recipient_instance_id: self.cfg.instance_id,
            recipient_specialist_id: "planner".into(),
            disclosure_digest: "a".repeat(64),
            context: vec![ContextMessage {
                message_id: Uuid::new_v4(),
                role: ContextRole::User,
                speaker: "owner".into(),
                body: "Help plan this project".into(),
            }],
            available_actions: vec![],
            available_peers: vec![],
        };
        let routing = RoutingRequest {
            workload: "conversation".into(),
            minimum_quality_ppm: 900000,
            minimum_held_out_cases: 10,
            context_tokens: 2048,
            max_latency_ms: 1000,
            available_local_memory_mib: 2048,
            permitted_destinations: self.mandate.destinations.clone(),
        };
        Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: self.mandate.id,
            specialist_id: "planner".into(),
            objective: "Discuss the project".into(),
            acceptance_criteria: vec![ACCEPTANCE.into()],
            capability: "conversation_turn".into(),
            resource: ResourceScope {
                kind: "conversation".into(),
                id: "fixture".into(),
            },
            deadline: Utc::now() + Duration::minutes(10),
            budget: Budget {
                micro_usd: 100,
                calls: 3,
                turns: 4,
                delegation_depth: 1,
            },
            payload: json!({"kind":"conversation_turn","turn":turn,"routing":routing}),
            parent_id: None,
        }
    }
    async fn cleanup(self) {
        self.server.abort();
        self.db.cleanup().await.unwrap();
    }
}
#[tokio::test]
async fn conversation_proposals_are_bound_and_successful_output_survives_retry() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    let request = f.request(f.cfg.instance_id);
    let task = f.store.submit(&request, &f.operator).await.unwrap();
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    let task = f.store.get_internal(task.contract.id).await.unwrap();
    assert_eq!(task.contract.status, TaskStatus::Completed);
    let evidence: Value =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(
        evidence["conversation_reply"]["question"],
        "Which milestone comes first?"
    );
    assert_eq!(evidence["actions_performed"], false);
    assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
    assert!(
        f.store
            .respond(
                task.contract.id,
                "answer-1",
                "First milestone",
                None,
                &f.operator
            )
            .await
            .is_err()
    );
    // Recovered worker can reuse a successful private artifact without repeating
    // its provider send, even when task publication itself was interrupted.
    sqlx::query("DELETE FROM agency_outcomes WHERE task_id=$1")
        .bind(task.contract.id)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let mut working = task.clone();
    working.contract.status = TaskStatus::Working;
    working.contract.outcome = None;
    f.store.update_contract(&working).await.unwrap();
    let working = f.store.get_internal(task.contract.id).await.unwrap();
    let result = codifix_agency_runtime::model::conversation(
        &f.store,
        &f.cfg,
        &working,
        &serde_json::from_value(request.payload["turn"].clone()).unwrap(),
        &serde_json::from_value(request.payload["routing"].clone()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result, evidence);
    assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
    f.cleanup().await;
}
#[tokio::test]
async fn wrong_sender_and_invented_actions_do_not_execute() {
    let Some(f) = Fixture::new().await else {
        return;
    };
    assert!(
        f.store
            .submit(&f.request(Uuid::new_v4()), &f.operator)
            .await
            .is_err()
    );
    f.provider.lock().unwrap().invalid = true;
    let task = f
        .store
        .submit(&f.request(f.cfg.instance_id), &f.operator)
        .await
        .unwrap();
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert_eq!(
        f.store
            .get_internal(task.contract.id)
            .await
            .unwrap()
            .contract
            .status,
        TaskStatus::InputRequired
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_outcomes WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(f.provider.lock().unwrap().calls.len(), 1);
    f.cleanup().await;
}

#[tokio::test]
async fn two_instances_exchange_one_bounded_private_conversation_turn() {
    const CHILD: &str = "CODIFIX_INBOX_TWO_INSTANCE_CHILD";
    const TOKEN: &str = "synthetic-inbox-two-instance-credential-only";
    if std::env::var_os(CHILD).is_none() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "two_instances_exchange_one_bounded_private_conversation_turn",
                "--nocapture",
            ])
            .env(CHILD, "yes")
            .env("CODIFIX_INBOX_TEST_PEER_TOKEN", TOKEN)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    for lose_submit_response in [false, true] {
        let Some(mut home) = Fixture::new().await else {
            return;
        };
        let Some(mut remote) = Fixture::new().await else {
            return;
        };
        let source = Peer {
            id: "enrolled-home".into(),
            instance_id: home.cfg.instance_id,
            token_env: "UNUSED_REMOTE_INBOUND".into(),
            operator: false,
            mandates: BTreeSet::from([remote.mandate.id]),
            endpoint: None,
            outbound_token_env: None,
        };
        remote.cfg.peers.push(source.clone());
        let app = codifix_agency_runtime::server::App {
            store: remote.store.clone(),
            config: Arc::new(remote.cfg.clone()),
            auth: Auth::from_credentials(vec![(source, TOKEN.into())]).unwrap(),
        };
        let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", tcp.local_addr().unwrap());
        let lose_once = Arc::new(std::sync::atomic::AtomicBool::new(lose_submit_response));
        let app = codifix_agency_runtime::server::router(app).layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let lose_once = lose_once.clone();
                async move {
                    let lose = request.uri().path() == "/message:send"
                        && lose_once.swap(false, std::sync::atomic::Ordering::SeqCst);
                    let response = next.run(request).await;
                    if lose {
                        use axum::response::IntoResponse;
                        (
                            axum::http::StatusCode::BAD_GATEWAY,
                            "synthetic lost submission response",
                        )
                            .into_response()
                    } else {
                        response
                    }
                }
            },
        ));
        let server = tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        });
        home.cfg.peers.push(Peer {
            id: "remote".into(),
            instance_id: remote.cfg.instance_id,
            token_env: "UNUSED_HOME_INBOUND".into(),
            operator: false,
            mandates: BTreeSet::new(),
            endpoint: Some(endpoint),
            outbound_token_env: Some("CODIFIX_INBOX_TEST_PEER_TOKEN".into()),
        });
        let nested = remote.request(home.cfg.instance_id);
        let mut request = home.request(home.cfg.instance_id);
        request.capability = "delegate".into();
        request.resource = ResourceScope {
            kind: "peer".into(),
            id: "remote".into(),
        };
        request.acceptance_criteria = vec!["delegated_result_reported".into()];
        request.budget = Budget {
            micro_usd: 200,
            calls: 8,
            turns: 8,
            delegation_depth: 2,
        };
        request.payload = json!({"kind":"delegate","peer":"remote","request":nested});
        let task = home.store.submit(&request, &home.operator).await.unwrap();
        let store = remote.store.clone();
        let cfg = remote.cfg.clone();
        let runner = tokio::spawn(async move {
            loop {
                worker::run_once(&store, &cfg).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                worker::run_once(&home.store, &home.cfg)
            )
            .await
            .unwrap()
            .unwrap()
        );
        if lose_submit_response {
            assert_eq!(
                home.store
                    .get_internal(task.contract.id)
                    .await
                    .unwrap()
                    .contract
                    .status,
                TaskStatus::InputRequired
            );
            codifix_agency_runtime::exchange::reconcile_submissions(&home.store, &home.cfg)
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    worker::run_once(&home.store, &home.cfg)
                )
                .await
                .unwrap()
                .unwrap()
            );
        }
        let done = home.store.get_internal(task.contract.id).await.unwrap();
        assert_eq!(done.contract.status, TaskStatus::Completed);
        let evidence: Value =
            sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
                .bind(task.contract.id)
                .fetch_one(&home.db.pool)
                .await
                .unwrap();
        assert_eq!(evidence["independent_verification"], false);
        assert!(
            evidence["reported_result"]["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["parts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["data"]["conversation_reply"]["reply"]
                        == "The roadmap is ready to discuss."))
        );
        assert_eq!(remote.provider.lock().unwrap().calls.len(), 1);
        assert!(home.provider.lock().unwrap().calls.is_empty());
        runner.abort();
        server.abort();
        home.cleanup().await;
        remote.cleanup().await;
    }
}
