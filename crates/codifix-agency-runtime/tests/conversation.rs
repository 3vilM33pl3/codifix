#![allow(clippy::unwrap_used)]
//! Synthetic model and authenticated A2A regression tests; no live provider.
#[path = "linear/delegation.rs"]
mod linear_delegation;
#[path = "linear/graphql.rs"]
mod linear_graphql;
#[path = "social/roundtrip.rs"]
mod social_roundtrip;
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
    reply: Option<Value>,
}
async fn complete(
    State(state): State<Arc<Mutex<Provider>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let mut state = state.lock().unwrap();
    state.calls.push(body);
    let reply = if let Some(reply) = &state.reply {
        reply.clone()
    } else if state.invalid {
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
            capabilities: BTreeSet::from([
                "conversation_turn".into(),
                "delegate".into(),
                "inspect".into(),
            ]),
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
            observation_enabled: false,
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
            social: None,
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

#[tokio::test]
async fn human_gateway_executes_granted_actions_and_revocation_stops_queued_dispatch() {
    let Some(mut f) = Fixture::new().await else {
        return;
    };
    f.cfg.inbox_enabled = true;
    let human = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'human_oidc','Synthetic human','admin')").bind(human).execute(&f.db.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source) SELECT $1,$2,project_id,'reader','direct' FROM agency_mandates WHERE id=$3")
        .bind(Uuid::new_v4()).bind(human).bind(f.mandate.id).execute(&f.db.pool).await.unwrap();
    let inbox = codifix_inbox::store::Store {
        pool: f.db.pool.clone(),
        home_instance_id: f.cfg.instance_id,
    };
    let request = f.request(f.cfg.instance_id);
    let mut action = request.clone();
    action.capability = "inspect".into();
    action.acceptance_criteria = vec!["status_observed".into()];
    action.payload = json!({"kind":"inspect"});
    action.budget = Budget {
        micro_usd: 1,
        calls: 1,
        turns: 1,
        delegation_depth: 0,
    };
    let contact = Uuid::new_v4();
    let grant = codifix_inbox::InvocationGrant {
        id: Uuid::new_v4(),
        home_instance_id: f.cfg.instance_id,
        principal_id: human,
        contact: codifix_inbox::Specialist {
            id: contact,
            instance_id: f.cfg.instance_id,
            specialist_id: "planner".into(),
            display_name: "Planner".into(),
            summary: "Synthetic".into(),
            capabilities: vec!["conversation_turn".into(), "inspect".into()],
            enrolled: true,
            available: true,
            grant_id: None,
            mandate_id: Some(f.mandate.id),
            remote: false,
        },
        request: serde_json::to_value(&request).unwrap(),
        routing: request.payload["routing"].clone(),
        actions: vec![codifix_inbox::ActionTemplate {
            key: "inspect".into(),
            request: serde_json::to_value(action).unwrap(),
        }],
        expires_at: Utc::now() + Duration::minutes(30),
    };
    inbox.install_grant(&grant).await.unwrap();
    let chat = inbox
        .create(
            human,
            &codifix_inbox::CreateConversation {
                client_request_id: Uuid::new_v4(),
                title: "Private synthetic chat".into(),
                specialist_id: contact,
            },
        )
        .await
        .unwrap();
    f.provider.lock().unwrap().reply = Some(
        json!({"version":1,"reply":"I will inspect the permitted service.","question":null,"actions":[{"key":"inspect","payload":{}}],"peer_requests":[]}),
    );
    let message = codifix_inbox::SendMessage {
        client_message_id: Uuid::new_v4(),
        body: "Inspect the service".into(),
        format: "text".into(),
        artifact: None,
        recipient_member_ids: vec![],
    };
    inbox
        .send(human, chat.conversation.id, &message)
        .await
        .unwrap();
    let ticket: Uuid =
        sqlx::query_scalar("SELECT id FROM inbox_dispatches WHERE root_message_id=$1")
            .bind(message.client_message_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    inbox
        .pause(human, chat.conversation.id, true)
        .await
        .unwrap();
    assert!(!worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM inbox_dispatches WHERE id=$1")
            .bind(ticket)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        "queued"
    );
    assert!(f.provider.lock().unwrap().calls.is_empty());
    inbox
        .pause(human, chat.conversation.id, false)
        .await
        .unwrap();
    assert!(
        codifix_agency_runtime::inbox::dispatch_once(&f.store, &f.cfg)
            .await
            .unwrap()
    );
    codifix_agency_runtime::inbox::reconcile(&f.store, &f.cfg)
        .await
        .unwrap();
    let submitted = inbox.detail(human, chat.conversation.id).await.unwrap();
    assert_eq!(submitted.tasks[0].status, "submitted");
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM inbox_dispatches WHERE task_id=$1")
            .bind(submitted.tasks[0].id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        ticket
    );
    // Pausing after ticket redemption also holds the queued worker job rather
    // than rejecting the message or consuming a worker attempt.
    inbox
        .pause(human, chat.conversation.id, true)
        .await
        .unwrap();
    assert!(!worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT attempts FROM background_jobs WHERE operation_id=$1")
            .bind(submitted.tasks[0].id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        f.store
            .get_internal(submitted.tasks[0].id)
            .await
            .unwrap()
            .contract
            .status,
        TaskStatus::Submitted
    );
    inbox
        .pause(human, chat.conversation.id, false)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
    let mut working = f.store.get_internal(submitted.tasks[0].id).await.unwrap();
    working
        .contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    f.store.update_contract(&working).await.unwrap();
    codifix_agency_runtime::inbox::reconcile(&f.store, &f.cfg)
        .await
        .unwrap();
    assert_eq!(
        inbox
            .detail(human, chat.conversation.id)
            .await
            .unwrap()
            .tasks[0]
            .status,
        "working"
    );
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert!(
        worker::run_once(&f.store, &f.cfg).await.unwrap(),
        "granted action runs without another human approval"
    );
    let action_done = inbox.detail(human, chat.conversation.id).await.unwrap();
    assert_eq!(action_done.messages.len(), 3);
    assert!(action_done.messages.iter().any(|m| {
        serde_json::from_str::<Value>(&m.body)
            .ok()
            .is_some_and(|v| v["type"] == "codifix.action_result.v1")
    }));
    f.provider.lock().unwrap().reply = Some(
        json!({"version":1,"reply":"The authorized inspection completed successfully.","question":null,"actions":[],"peer_requests":[]}),
    );
    assert!(
        worker::run_once(&f.store, &f.cfg).await.unwrap(),
        "action result continues the same response under its remaining budget"
    );
    let detail = inbox.detail(human, chat.conversation.id).await.unwrap();
    assert_eq!(detail.messages.len(), 4);
    assert_eq!(detail.messages[0].delivery, "confirmed");
    assert_eq!(detail.tasks.len(), 3);
    assert!(
        detail
            .tasks
            .iter()
            .all(|t| t.root_id == message.client_message_id)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT turns FROM inbox_roots WHERE id=$1")
            .bind(message.client_message_id)
            .fetch_one(&f.db.pool)
            .await
            .unwrap(),
        2
    );
    let second_prompt = f.provider.lock().unwrap().calls[1]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(second_prompt.contains("codifix.action_result.v1"));
    assert!(detail.tasks.iter().all(|t| t.status == "completed"));
    assert_eq!(f.provider.lock().unwrap().calls.len(), 2);
    inbox
        .send(human, chat.conversation.id, &message)
        .await
        .unwrap();
    assert!(!worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert_eq!(f.provider.lock().unwrap().calls.len(), 2);
    inbox
        .send(
            human,
            chat.conversation.id,
            &codifix_inbox::SendMessage {
                client_message_id: Uuid::new_v4(),
                body: "Inspect again".into(),
                ..message
            },
        )
        .await
        .unwrap();
    inbox.revoke_grant(grant.id).await.unwrap();
    assert!(!worker::run_once(&f.store, &f.cfg).await.unwrap());
    assert_eq!(f.provider.lock().unwrap().calls.len(), 2);
    f.cleanup().await;
}

#[tokio::test]
async fn human_remote_inbox_roundtrip_preserves_owner_disclosure_and_root_budget() {
    human_remote_roundtrip(
        "human_remote_inbox_roundtrip_preserves_owner_disclosure_and_root_budget",
        false,
    )
    .await
}
#[tokio::test]
async fn signed_xrpc_workers_reply_to_direct_and_group_messages() {
    human_remote_roundtrip(
        "signed_xrpc_workers_reply_to_direct_and_group_messages",
        true,
    )
    .await
}
async fn human_remote_roundtrip(test_name: &str, social: bool) {
    const CHILD: &str = "CODIFIX_HUMAN_REMOTE_INBOX_CHILD";
    const TOKEN: &str = "synthetic-human-remote-inbox-peer-only-token";
    if std::env::var_os(CHILD).is_none() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
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
    human_remote_inbox(None, social).await;
}

#[tokio::test]
#[ignore = "requires tests/federation-pds/run.mjs --agency"]
async fn pds_social_group_chat_delivers_scoped_turns_and_replies() {
    let fixture =
        std::fs::read(std::env::var_os("CODIFIX_FEDERATION_PDS_FIXTURE").unwrap()).unwrap();
    human_remote_inbox(Some(serde_json::from_slice(&fixture).unwrap()), false).await;
}

async fn human_remote_inbox(pds: Option<Value>, social: bool) {
    const TOKEN: &str = "synthetic-human-remote-inbox-peer-only-token";
    let Some(mut home) = Fixture::new().await else {
        return;
    };
    let Some(mut remote) = Fixture::new().await else {
        return;
    };
    home.cfg.inbox_enabled = true;
    let source = Peer {
        id: "human-home".into(),
        instance_id: home.cfg.instance_id,
        token_env: "UNUSED_REMOTE_INBOUND".into(),
        operator: false,
        mandates: BTreeSet::from([remote.mandate.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    let synthetic_pds = if social {
        Some(social_roundtrip::install(&mut home.cfg, &mut remote.cfg).await)
    } else {
        None
    };
    remote.cfg.peers.push(source.clone());
    let notice_chat = Uuid::new_v4();
    let notice_owner = Uuid::new_v4();
    if let Some(pds) = &pds {
        use codifix_agency_runtime::social::{SocialConfig, SocialPeer};
        let audience = pds["recipient_did"].as_str().unwrap();
        let sender = pds["repository"].as_str().unwrap();
        let social = |did: &str, peer: &str, peer_did: &str, key: &str| SocialConfig {
            did: did.into(),
            pds: pds["base"].as_str().unwrap().into(),
            access_token_env: "AGENCY_FIXTURE_ACCESS_TOKEN".into(),
            access_token_file: None,
            peers: BTreeMap::from([(
                peer.into(),
                SocialPeer {
                    incident_conversation: None,
                    notice_inbox: None,
                    did: peer_did.into(),
                    signing_key: key.into(),
                },
            )]),
        };
        remote.cfg.social = Some(social(
            audience,
            "human-home",
            sender,
            pds["signing_key"].as_str().unwrap(),
        ));
        remote.cfg.inbox_enabled = true;
        remote
            .cfg
            .social
            .as_mut()
            .unwrap()
            .peers
            .get_mut("human-home")
            .unwrap()
            .notice_inbox = Some(codifix_agency_runtime::social::NoticeInbox {
            conversation_id: notice_chat,
            unseal_url: Some("https://ca.example.test/unseal".into()),
        });
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Synthetic custodian')")
            .bind(notice_owner).execute(&remote.db.pool).await.unwrap();
        sqlx::query("INSERT INTO inbox_conversations(id,home_instance_id,principal_id,request_digest,title) VALUES($1,$2,$3,'fixture','Custodian notices')")
            .bind(notice_chat).bind(remote.cfg.instance_id).bind(notice_owner).execute(&remote.db.pool).await.unwrap();
        home.cfg.social = Some(social(
            sender,
            "remote",
            audience,
            pds["recipient_key"].as_str().unwrap(),
        ));
    }
    if social || pds.is_some() {
        remote.cfg.inbox_enabled = true;
        remote
            .cfg
            .social
            .as_mut()
            .unwrap()
            .peers
            .get_mut("human-home")
            .unwrap()
            .incident_conversation = Some(notice_chat);
        if pds.is_none() {
            sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Incident recipient')").bind(notice_owner).execute(&remote.db.pool).await.unwrap();
            sqlx::query("INSERT INTO inbox_conversations(id,home_instance_id,principal_id,request_digest,title) VALUES($1,$2,$3,'fixture','Incident updates')").bind(notice_chat).bind(remote.cfg.instance_id).bind(notice_owner).execute(&remote.db.pool).await.unwrap();
        }
    }
    let app = codifix_agency_runtime::server::App {
        store: remote.store.clone(),
        config: Arc::new(remote.cfg.clone()),
        auth: Auth::from_credentials(vec![(source, TOKEN.into())]).unwrap(),
    };
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", tcp.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(tcp, codifix_agency_runtime::server::router(app))
            .await
            .unwrap();
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
    if pds.is_some() {
        use codifix_agency_runtime::notice::{NoticeEvent, ScenarioNotice};
        let mut notice = ScenarioNotice {
            task_id: Uuid::new_v4(),
            attempt: Uuid::new_v4(),
            subject: "disposable-openbao".into(),
            participant: Some("Olivier".into()),
            event: NoticeEvent::AwaitingOperator,
        };
        for event in [
            NoticeEvent::AwaitingOperator,
            NoticeEvent::AwaitingOperator,
            NoticeEvent::UnsealedReady,
        ] {
            notice.event = event;
            notice.send(&home.store, &home.cfg, "remote").await.unwrap();
        }
        let inbox = codifix_inbox::store::Store {
            pool: remote.db.pool.clone(),
            home_instance_id: remote.cfg.instance_id,
        };
        let detail = inbox.detail(notice_owner, notice_chat).await.unwrap();
        assert_eq!(detail.messages.len(), 2, "same notice delivered only once");
        assert!(detail.messages[0].body.contains("Waiting for the operator"));
        assert_eq!(
            detail.messages[0].artifact.as_ref().unwrap()["protected_unseal_url"],
            "https://ca.example.test/unseal"
        );
        assert!(detail.messages[1].body.contains("unsealed and ready"));
        assert!(detail.messages[1].artifact.is_none());
    }
    if social || pds.is_some() {
        let notice = codifix_agency_runtime::notice::IncidentNotice {
            incident_id: Uuid::new_v4(),
            event_id: 1,
            service: "catalogue".into(),
            state: "recovered".into(),
            summary: "Health and catalogue item request passed".into(),
        };
        for _ in 0..2 {
            notice.send(&home.store, &home.cfg, "remote").await.unwrap();
        }
        let inbox = codifix_inbox::store::Store {
            pool: remote.db.pool.clone(),
            home_instance_id: remote.cfg.instance_id,
        };
        let messages = inbox
            .detail(notice_owner, notice_chat)
            .await
            .unwrap()
            .messages;
        assert_eq!(
            messages
                .iter()
                .filter(|m| m.body.contains("Health and catalogue item request passed"))
                .count(),
            1
        );
    }
    let human = Uuid::new_v4();
    let other = Uuid::new_v4();
    for principal in [human, other] {
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'human_oidc','Synthetic human','admin')").bind(principal).execute(&home.db.pool).await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source) SELECT $1,$2,project_id,'reader','direct' FROM agency_mandates WHERE id=$3")
            .bind(Uuid::new_v4()).bind(principal).bind(home.mandate.id).execute(&home.db.pool).await.unwrap();
    }
    let inbox = codifix_inbox::store::Store {
        pool: home.db.pool.clone(),
        home_instance_id: home.cfg.instance_id,
    };
    let nested = remote.request(home.cfg.instance_id);
    let routing = nested.payload["routing"].clone();
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
    let contact = Uuid::new_v4();
    let grant = codifix_inbox::InvocationGrant {
        id: Uuid::new_v4(),
        home_instance_id: home.cfg.instance_id,
        principal_id: human,
        contact: codifix_inbox::Specialist {
            id: contact,
            instance_id: remote.cfg.instance_id,
            specialist_id: "planner".into(),
            display_name: "Remote planner".into(),
            summary: "Synthetic remote".into(),
            capabilities: vec!["conversation_turn".into()],
            enrolled: true,
            available: true,
            grant_id: None,
            mandate_id: Some(remote.mandate.id),
            remote: true,
        },
        request: serde_json::to_value(request).unwrap(),
        routing,
        actions: vec![],
        expires_at: Utc::now() + Duration::minutes(30),
    };
    inbox.install_grant(&grant).await.unwrap();
    let chat = inbox
        .create(
            human,
            &codifix_inbox::CreateConversation {
                client_request_id: Uuid::new_v4(),
                title: "Human and remote planner".into(),
                specialist_id: contact,
            },
        )
        .await
        .unwrap();
    let message = codifix_inbox::SendMessage {
        client_message_id: Uuid::new_v4(),
        body: "Please discuss the private roadmap".into(),
        format: "markdown".into(),
        artifact: None,
        recipient_member_ids: vec![],
    };
    inbox
        .send(human, chat.conversation.id, &message)
        .await
        .unwrap();
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
    let first = inbox.detail(human, chat.conversation.id).await.unwrap();
    assert_eq!(first.messages.len(), 2);
    assert_eq!(first.messages[0].delivery, "confirmed");
    assert!(
        first.messages[1]
            .body
            .contains("Which milestone comes first?")
    );
    assert_eq!(first.tasks.len(), 1);
    assert_eq!(first.tasks[0].status, "input_required");
    assert_eq!(first.tasks[0].root_id, message.client_message_id);
    assert_eq!(
        first.tasks[0].result.as_ref().unwrap()["independent_verification"],
        false
    );
    assert!(inbox.detail(other, chat.conversation.id).await.is_err());
    assert!(
        inbox
            .respond(
                other,
                chat.conversation.id,
                first.tasks[0].id,
                Uuid::new_v4(),
                "Forged answer".into()
            )
            .await
            .is_err()
    );
    assert!(
        inbox
            .cancel(other, chat.conversation.id, first.tasks[0].id)
            .await
            .is_err()
    );
    let linked: (Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT principal_id,conversation_id,member_id FROM inbox_task_origins WHERE task_id=$1",
    )
    .bind(first.tasks[0].id)
    .fetch_one(&home.db.pool)
    .await
    .unwrap();
    assert_eq!(linked, (human, chat.conversation.id, chat.members[0].id));
    remote.provider.lock().unwrap().reply = Some(
        json!({"version":1,"reply":"Milestone one is the agreed first step.","question":null,"actions":[],"peer_requests":[]}),
    );
    inbox
        .respond(
            human,
            chat.conversation.id,
            first.tasks[0].id,
            Uuid::new_v4(),
            "Milestone one comes first".into(),
        )
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
    let final_chat = inbox.detail(human, chat.conversation.id).await.unwrap();
    assert_eq!(final_chat.messages.len(), 4);
    assert_eq!(final_chat.tasks.len(), 2);
    assert!(
        final_chat
            .tasks
            .iter()
            .all(|t| t.root_id == message.client_message_id)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT turns FROM inbox_roots WHERE id=$1")
            .bind(message.client_message_id)
            .fetch_one(&home.db.pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM inbox_roots WHERE conversation_id=$1")
            .bind(chat.conversation.id)
            .fetch_one(&home.db.pool)
            .await
            .unwrap(),
        1
    );
    let calls = remote.provider.lock().unwrap().calls.clone();
    assert_eq!(calls.len(), 2);
    assert!(home.provider.lock().unwrap().calls.is_empty());
    let turn: ConversationTurn =
        serde_json::from_str(calls[1]["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(turn.home_instance_id, home.cfg.instance_id);
    assert_eq!(turn.recipient_instance_id, remote.cfg.instance_id);
    assert_eq!(turn.root_message_id, message.client_message_id);
    assert!(
        turn.context
            .iter()
            .any(|m| m.body == "Milestone one comes first")
    );
    assert!(!serde_json::to_string(&turn).unwrap().contains(TOKEN));
    if pds.is_some() {
        let mut local = grant.clone();
        local.id = Uuid::new_v4();
        local.contact.id = Uuid::new_v4();
        local.contact.instance_id = home.cfg.instance_id;
        local.contact.remote = false;
        local.contact.mandate_id = Some(home.mandate.id);
        local.request = serde_json::to_value(home.request(home.cfg.instance_id)).unwrap();
        inbox.install_grant(&local).await.unwrap();
        let local_member = Uuid::new_v4();
        inbox
            .add_member(
                human,
                chat.conversation.id,
                &codifix_inbox::AddMember {
                    client_request_id: local_member,
                    specialist_id: local.contact.id,
                    confirmed: true,
                    selected_message_ids: vec![],
                    summary: "Group review".into(),
                },
            )
            .await
            .unwrap();
        let mut group = message.clone();
        group.client_message_id = Uuid::new_v4();
        group.body = "PRIVATE_SOCIAL_SENTINEL please review the group milestone".into();
        group.recipient_member_ids = vec![chat.members[0].id, local_member];
        inbox
            .send(human, chat.conversation.id, &group)
            .await
            .unwrap();
        inbox
            .send(human, chat.conversation.id, &group)
            .await
            .unwrap();
        let detail = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                worker::run_once(&home.store, &home.cfg).await.unwrap();
                let detail = inbox.detail(human, chat.conversation.id).await.unwrap();
                if detail
                    .messages
                    .iter()
                    .filter(|m| m.root_id == group.client_message_id && m.member_id.is_some())
                    .count()
                    >= 2
                {
                    break detail;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("both asynchronous group replies");
        let replies: BTreeSet<_> = detail
            .messages
            .iter()
            .filter(|m| m.root_id == group.client_message_id)
            .filter_map(|m| m.member_id)
            .collect();
        assert_eq!(
            replies,
            group.recipient_member_ids.iter().copied().collect()
        );
        assert_eq!(home.provider.lock().unwrap().calls.len(), 1);
        assert_eq!(remote.provider.lock().unwrap().calls.len(), 3);
        assert!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_social_used_tokens")
                .fetch_one(&remote.db.pool)
                .await
                .unwrap()
                > 0,
            "peer messages must arrive over private XRPC"
        );
    }
    if let Some(pds) = synthetic_pds {
        social_roundtrip::group(&inbox, human, chat.conversation.id, &grant, &home, &remote).await;
        pds.abort();
    }
    runner.abort();
    server.abort();
    home.cleanup().await;
    remote.cleanup().await;
}
