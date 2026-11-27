//! Native command transport, human HTTP gateway, ticket and worker integration.
#![allow(clippy::unwrap_used)]

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{Duration, Utc};
use codifix_agency::{
    Budget, Mandate, MandateStatus, ModelDestination, ResourceScope, SpecialistManifest,
    conversation::{ACCEPTANCE, ConversationTurn},
    routing::{ModelEvaluation, ModelProfile, RoutingRequest},
};
use codifix_agency_projects::{Authority, Plan, Scope, StepPhase, StepReceipt, stable_id};
use codifix_agency_runtime::{
    config::{Config, ModelEndpoint, Peer},
    store::{Store, Submit},
    worker,
};
use codifix_inbox::{ActionTemplate, Detail, InvocationGrant, Specialist, TaskCard};
use codifix_record::AuthMode;
use codifix_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use sqlx::{ConnectOptions, PgPool};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{
    io::AsyncWriteExt, net::TcpListener, process::Command, task::JoinHandle, time::timeout,
};
use uuid::Uuid;

const LINEAR_TOKEN_ENV: &str = "CODIFIX_INBOX_CANDIDATE_LINEAR_TOKEN";
const LINEAR_TOKEN: &str = "synthetic-inbox-candidate-linear-token";
const CONVERSATIONS: &str = "/v1/inbox/conversations";
const SPECIALIST: &str = "candidate-planner";
const ROOT_BUDGET: Budget = Budget {
    micro_usd: 200,
    calls: 40,
    turns: 8,
    delegation_depth: 0,
};

struct Server {
    origin: String,
    task: JoinHandle<()>,
}
impl Server {
    async fn start(router: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        Self::serve(listener, router)
    }
    fn serve(listener: TcpListener, router: Router) -> Self {
        Self {
            origin: format!("http://{}", listener.local_addr().unwrap()),
            task: tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn native(
    origin: &str,
    token: Option<&str>,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let request = json!({"origin":origin,"token":token,"path":path,
        "method": if body.is_some() { "POST" } else { "GET" },"body":body});
    let mut child = Command::new(std::env::var_os("CODIFIX_INBOX_NATIVE_TEST").unwrap())
        .args([
            "--ignored",
            "--exact",
            "inbox_candidate::native_inbox_request",
            "--nocapture",
        ])
        .env_remove(LINEAR_TOKEN_ENV)
        .env_remove("CODIFIX_TEST_DATABASE_URL")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    let output = timeout(std::time::Duration::from_secs(20), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "native helper failed: {stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response = stdout
        .lines()
        .find_map(|line| line.strip_prefix("INBOX_NATIVE_RESPONSE "))
        .unwrap();
    serde_json::from_str(response).unwrap()
}

#[derive(Default)]
struct Model {
    replies: VecDeque<Value>,
    turns: Vec<ConversationTurn>,
}
impl Model {
    fn propose(&mut self, key: &str, payload: Value) {
        self.replies.push_back(
            json!({"version":1,"reply":"Reviewing the authorized plan operation.","question":null,
            "actions":[{"key":key,"payload":payload}],"peer_requests":[]}),
        );
    }
}
async fn model(
    State(model): State<Arc<Mutex<Model>>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Json<Value> {
    assert!(!headers.contains_key("authorization"));
    assert_eq!(request["model"], "fixture-v1");
    let turn = serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
    let mut model = model.lock().unwrap();
    model.turns.push(turn);
    let reply = model.replies.pop_front().expect("unexpected model call");
    Json(
        json!({"choices":[{"message":{"content":reply.to_string()}}],"usage":{"prompt_tokens":20,"completion_tokens":20}}),
    )
}

#[derive(Debug, PartialEq)]
enum Event {
    Read(Uuid),
    Create(Uuid),
}
struct Linear {
    project: Uuid,
    issue: Uuid,
    events: Vec<Event>,
    objects: BTreeMap<Uuid, Value>,
    interrupt_first_write: bool,
}
async fn graphql(
    State(linear): State<Arc<Mutex<Linear>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(headers["authorization"], LINEAR_TOKEN);
    let mut linear = linear.lock().unwrap();
    let query = body["query"].as_str().unwrap();
    let variables = &body["variables"];
    let id: Uuid = serde_json::from_value(variables["id"].clone()).unwrap();
    assert!(
        id == linear.project || id == linear.issue,
        "out-of-scope GraphQL resource"
    );
    let kind = if id == linear.project {
        "project"
    } else {
        "issue"
    };
    if query.starts_with("query AgencyRead") {
        assert!(query.contains(&format!("object: {kind}(id: $id)")));
        linear.events.push(Event::Read(id));
        return Json(json!({"data":{"object":linear.objects.get(&id)}})).into_response();
    }
    assert!(
        query.contains(&format!("effect: {kind}Create(")),
        "unexpected GraphQL operation"
    );
    assert_eq!(variables["input"]["id"], json!(id));
    assert_eq!(variables["input"]["useDefaultTemplate"], false);
    let input = &variables["input"];
    let relation = |field: &str| {
        if input[field].is_null() {
            Value::Null
        } else {
            json!({"id":input[field]})
        }
    };
    let object = if id == linear.project {
        json!({"id":id,"updatedAt":"2026-10-31T19:00:00Z","name":input["name"],"description":input["description"],
            "content":input["content"],"lead":relation("leadId"),"teams":{
                "nodes":input["teamIds"].as_array().unwrap().iter().map(|id|json!({"id":id})).collect::<Vec<_>>(),
                "pageInfo":{"hasNextPage":false}}})
    } else {
        assert_eq!(input["projectId"], json!(linear.project));
        assert!(linear.objects.contains_key(&linear.project));
        json!({"id":id,"updatedAt":"2026-10-31T19:00:00Z","title":input["title"],"description":input["description"],
            "team":relation("teamId"),"project":relation("projectId"),"projectMilestone":relation("projectMilestoneId"),"assignee":relation("assigneeId")})
    };
    assert!(
        linear.objects.insert(id, object).is_none(),
        "duplicate mutation for {id}"
    );
    linear.events.push(Event::Create(id));
    if std::mem::take(&mut linear.interrupt_first_write) {
        // The external object exists, but the adapter receives no success acknowledgement.
        return StatusCode::BAD_GATEWAY.into_response();
    }
    Json(json!({"data":{"effect":{"success":true,(kind):{"id":id}}}})).into_response()
}

async fn human(pool: &PgPool) -> (Uuid, String) {
    let id = Uuid::new_v4();
    let token = format!("mld_{}", Uuid::new_v4());
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Synthetic Inbox user')")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,session_kind,expires_at) VALUES($1,$2,$3,$4,'desktop',now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(id).bind(crate::auth::hash_secret(&token)).bind(vec![0_u8; 32]).execute(pool).await.unwrap();
    (id, token)
}

struct Candidate {
    service: Server,
    model: Arc<Mutex<Model>>,
    linear: Arc<Mutex<Linear>>,
    store: Store,
    inbox: codifix_inbox::store::Store,
    config: Config,
    grant: InvocationGrant,
    owner_token: String,
    other_token: String,
    plan: Plan,
    _model_server: Server,
    _linear_server: Server,
}
impl Candidate {
    async fn start(pool: PgPool) -> Self {
        let home = Uuid::new_v4();
        let project = Uuid::new_v4();
        let (owner, owner_token) = human(&pool).await;
        let (_, other_token) = human(&pool).await;
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'candidate-inbox','Inbox candidate fixture','/synthetic')")
            .bind(project).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source) VALUES($1,$2,$3,'reader','direct')")
            .bind(Uuid::new_v4()).bind(owner).bind(project).execute(&pool).await.unwrap();
        let mut plan: Plan = serde_json::from_str(include_str!(
            "../../../codifix-agency-projects/examples/plan.json"
        ))
        .unwrap();
        plan.revision = 2;
        plan.issues.truncate(1);
        plan.issues[0].milestone_key = None;
        plan.milestones.clear();
        plan.dependencies.clear();
        plan.validate().unwrap();
        let project_scope = ResourceScope {
            kind: "plan".into(),
            id: plan.id.to_string(),
        };
        let conversation_scope = ResourceScope {
            kind: "conversation".into(),
            id: "candidate-inbox".into(),
        };
        let destination = ModelDestination::Local {
            runtime: "candidate-fixture".into(),
        };
        let capabilities = BTreeSet::from([
            "conversation_turn".into(),
            "plan_prepare".into(),
            "plan_apply".into(),
        ]);
        let mandate = Mandate {
            id: Uuid::new_v4(),
            parent_id: None,
            issuer_instance_id: home,
            subject_instance_id: home,
            specialist_id: SPECIALIST.into(),
            capabilities: capabilities.clone(),
            resources: BTreeSet::from([project_scope.clone(), conversation_scope.clone()]),
            destinations: BTreeSet::from([destination.clone()]),
            expires_at: Utc::now() + Duration::hours(1),
            status: MandateStatus::Active,
            budget: Budget {
                micro_usd: 1_000,
                calls: 200,
                turns: 100,
                delegation_depth: 0,
            },
        };
        let operator = Peer {
            id: "fixture-operator".into(),
            instance_id: home,
            token_env: "UNUSED_FIXTURE_OPERATOR_TOKEN".into(),
            operator: true,
            mandates: BTreeSet::from([mandate.id]),
            endpoint: None,
            outbound_token_env: None,
        };
        let store = Store { pool: pool.clone() };
        store
            .install_mandate(project, &mandate, &operator, home)
            .await
            .unwrap();
        let inbox = codifix_inbox::store::Store {
            pool: pool.clone(),
            home_instance_id: home,
        };
        let routing = RoutingRequest {
            workload: "conversation".into(),
            minimum_quality_ppm: 900_000,
            minimum_held_out_cases: 10,
            context_tokens: 4_096,
            max_latency_ms: 1_000,
            available_local_memory_mib: 2_048,
            permitted_destinations: mandate.destinations.clone(),
        };
        let submit =
            |capability: &str, acceptance: &str, resource: ResourceScope, budget: Budget| Submit {
                request_id: Uuid::new_v4().to_string(),
                mandate_id: mandate.id,
                specialist_id: SPECIALIST.into(),
                objective: capability.into(),
                acceptance_criteria: vec![acceptance.into()],
                capability: capability.into(),
                resource,
                deadline: Utc::now() + Duration::minutes(10),
                budget,
                payload: json!({"kind":capability}),
                parent_id: None,
            };
        let action_budget = Budget {
            micro_usd: 0,
            calls: 10,
            turns: 1,
            delegation_depth: 0,
        };
        let actions = [
            ("prepare", "plan_prepare", "plan_valid"),
            ("apply", "plan_apply", "linear_readback_verified"),
        ]
        .into_iter()
        .map(|(key, cap, acceptance)| ActionTemplate {
            key: key.into(),
            request: json!(submit(
                cap,
                acceptance,
                project_scope.clone(),
                action_budget
            )),
        })
        .collect();
        let grant = InvocationGrant {
            id: Uuid::new_v4(),
            home_instance_id: home,
            principal_id: owner,
            contact: Specialist {
                id: Uuid::new_v4(),
                instance_id: home,
                specialist_id: SPECIALIST.into(),
                display_name: "Candidate planner".into(),
                summary: "Loopback fixture specialist".into(),
                capabilities: capabilities.iter().cloned().collect(),
                enrolled: true,
                available: true,
                grant_id: None,
                mandate_id: Some(mandate.id),
                remote: false,
            },
            request: json!(submit(
                "conversation_turn",
                ACCEPTANCE,
                conversation_scope,
                ROOT_BUDGET
            )),
            routing: json!(routing),
            actions,
            expires_at: Utc::now() + Duration::minutes(20),
        };
        inbox.install_grant(&grant).await.unwrap();

        let model_state = Arc::new(Mutex::new(Model::default()));
        let model_server = Server::start(
            Router::new()
                .route("/", post(model))
                .with_state(model_state.clone()),
        )
        .await;
        let linear_state = Arc::new(Mutex::new(Linear {
            project: stable_id(plan.id, "launch"),
            issue: stable_id(plan.id, "build"),
            events: vec![],
            objects: BTreeMap::new(),
            interrupt_first_write: true,
        }));
        let linear_server = Server::start(
            Router::new()
                .route("/graphql", post(graphql))
                .with_state(linear_state.clone()),
        )
        .await;
        let authority = Authority::Agreed {
            reference: "review://inbox-candidate/approved-plan".into(),
            plan_digest: plan.digest(),
            source_version: plan.source.version.clone(),
            scope: Scope {
                plan_id: plan.id,
                existing_resources: BTreeMap::new(),
                project_keys: BTreeSet::from(["launch".into()]),
                team_ids: plan.projects[0].team_ids.clone(),
                create: true,
                update: false,
            },
        };
        let config = Config {
            observation_enabled: false,
            inbox_enabled: true,
            instance_id: home,
            listen: "127.0.0.1:0".parse().unwrap(),
            public_url: "https://agency.invalid".into(),
            database_env: "UNUSED_CANDIDATE_DATABASE".into(),
            specialists: vec![SpecialistManifest {
                schema_version: 1,
                instance_id: home,
                specialist_id: SPECIALIST.into(),
                name: "Candidate planner".into(),
                capabilities,
            }],
            peers: vec![],
            models: vec![ModelEndpoint {
                profile_id: "fixture".into(),
                url: model_server.origin.clone(),
                token_env: None,
                model: "fixture-v1".into(),
                max_input_bytes: 32_768,
                max_output_tokens: 1_024,
                reserve_micro_usd: 10,
                input_micro_usd_per_million: 1,
                output_micro_usd_per_million: 1,
            }],
            model_profiles: vec![ModelProfile {
                id: "fixture".into(),
                model_revision: "fixture-v1".into(),
                destination,
                accepted_locally: true,
                context_tokens: 8_192,
                local_memory_mib: 1_024,
                evaluations: vec![ModelEvaluation {
                    evidence_id: "fixture-eval".into(),
                    model_revision: "fixture-v1".into(),
                    workload: "conversation".into(),
                    evaluated_at: Utc::now() - Duration::minutes(1),
                    valid_until: Utc::now() + Duration::hours(1),
                    held_out_cases: 20,
                    quality_ppm: 950_000,
                    micro_usd_per_call: 1,
                    latency_ms: 10,
                }],
            }],
            public_profile: None,
            private_repository_dids: vec![],
            social: None,
            openbao_controllers: BTreeMap::new(),
            linear: Some(
                json!({"endpoint":format!("{}/graphql",linear_server.origin),"token_env":LINEAR_TOKEN_ENV,"authorities":{mandate.id.to_string():authority}}),
            ),
        };
        config.validate().unwrap();
        let mut service_config: codifix_config::AppConfig = serde_json::from_value(json!({"service":{"bind_addr":"127.0.0.1:0","api_token":"unused-fixture-service-token"},
            "database":{"url":pool.connect_options().to_url_lossy().as_str()},"offline":{"enabled":false},"cluster":{"enabled":false},
            "reinforcement":{"enabled":false},"consolidation":{"enabled":false},"background_work":{"enabled":false},
            "mcp":{"enabled":false},"embeddings":{"enabled":false,"create_enabled":false},"inbox":{"enabled":true,"home_instance_id":home}})).unwrap();
        service_config.auth.mode = AuthMode::MultiUser;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        service_config.auth.public_base_url =
            Some(format!("http://{}", listener.local_addr().unwrap()));
        let web = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
        service_config.service.web_root = Some(web.display().to_string());
        let state = crate::runtime::build_state(service_config, Arc::new(Mutex::new(None)))
            .await
            .unwrap();
        let service = Server::serve(listener, crate::routes::build_http_app(state));
        Self {
            service,
            model: model_state,
            linear: linear_state,
            store,
            inbox,
            config,
            grant,
            owner_token,
            other_token,
            plan,
            _model_server: model_server,
            _linear_server: linear_server,
        }
    }

    async fn request(&self, path: &str, body: Option<Value>) -> Detail {
        serde_json::from_value(
            native(&self.service.origin, Some(&self.owner_token), path, body)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    async fn send(&self, path: &str, root: Uuid, text: &str, plan: Option<&Plan>) -> Detail {
        self.request(&format!("{path}/messages"),Some(json!({"client_message_id":root,"body":text,
            "format":if plan.is_some() {"plan"} else {"text"},"artifact":plan,"recipient_member_ids":[]}))).await
    }

    async fn run_worker(&self) -> bool {
        timeout(
            std::time::Duration::from_secs(10),
            worker::run_once(&self.store, &self.config),
        )
        .await
        .unwrap()
        .unwrap()
    }

    async fn steps(&self) -> Vec<StepReceipt> {
        sqlx::query_scalar::<_, Value>("SELECT receipt FROM agency_steps ORDER BY id")
            .fetch_all(&self.store.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect()
    }
}

fn card(detail: &Detail, root: Uuid, objective: &str) -> TaskCard {
    detail
        .tasks
        .iter()
        .find(|task| task.root_id == root && task.objective == objective)
        .unwrap_or_else(|| panic!("missing {objective} card: {:?}", detail.tasks))
        .clone()
}

#[tokio::test]
#[ignore = "requires the native test binary and a disposable pgvector database"]
async fn native_inbox_linear_workflow() {
    assert_eq!(std::env::var(LINEAR_TOKEN_ENV).as_deref(), Ok(LINEAR_TOKEN));
    let db = IsolatedDatabase::new()
        .await
        .expect("run via scripts/test-inbox-candidate.sh with a disposable database");
    // Keep cleanup outside the scenario so its assertion failures still drop the database.
    let result = tokio::spawn(workflow(db.pool.clone())).await;
    db.cleanup().await.unwrap();
    result.unwrap();
    println!("INBOX_CANDIDATE_VERIFIED");
}

async fn workflow(pool: PgPool) {
    let migrations: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version BETWEEN 95 AND 97 AND success ORDER BY version")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(migrations, [95, 96, 97]);
    let f = Candidate::start(pool.clone()).await;
    let origin = &f.service.origin;
    assert!(
        native(origin, None, "/v1/network/status", None)
            .await
            .unwrap_err()
            .contains("HTTP 401")
    );
    assert_eq!(
        native(
            origin,
            Some(&f.owner_token),
            "/v1/inbox/mandates",
            Some(json!({}))
        )
        .await
        .unwrap_err(),
        "API operation is not allowed"
    );
    let status = native(origin, Some(&f.owner_token), "/v1/network/status", None)
        .await
        .unwrap();
    assert_eq!(status["enabled"], true);
    assert_eq!(status["home_instance_id"], json!(f.config.instance_id));
    let specialists = native(
        origin,
        Some(&f.owner_token),
        "/v1/network/specialists",
        None,
    )
    .await
    .unwrap();
    assert_eq!(specialists.as_array().unwrap().len(), 1);
    assert_eq!(specialists[0]["id"], json!(f.grant.contact.id));
    assert_eq!(specialists[0]["grant_id"], json!(f.grant.id));
    let chat = f
        .request(
            CONVERSATIONS,
            Some(
                json!({"client_request_id":Uuid::new_v4(), "title":"Candidate fixture plan",
        "specialist_id":f.grant.contact.id}),
            ),
        )
        .await;
    let conversation = chat.conversation.id;
    let path = format!("{CONVERSATIONS}/{conversation}");
    assert!(
        native(origin, Some(&f.other_token), &path, None)
            .await
            .is_err()
    );

    let mut outside = f.plan.clone();
    outside.id = Uuid::new_v4();
    // Give the first negative case a matching digest so it reaches the UUID
    // scope check. The canonical plan scope and invocation grant stay intact.
    let mut scope_probe = f.config.clone();
    let mandate = f.grant.contact.mandate_id.unwrap().to_string();
    scope_probe.linear.as_mut().unwrap()["authorities"][mandate]["plan_digest"] =
        json!(outside.digest());
    let mut stale = f.plan.clone();
    stale.revision -= 1;
    assert_ne!(stale.digest(), f.plan.digest());
    for (reason, proposed, config) in [
        ("wrong plan scope", outside, &scope_probe),
        ("stale reviewed digest", stale, &f.config),
    ] {
        let root = Uuid::new_v4();
        f.send(&path, root, reason, Some(&proposed)).await;
        f.model
            .lock()
            .unwrap()
            .propose("prepare", json!({"plan":proposed}));
        assert!(f.run_worker().await, "model proposal for {reason}");
        assert!(
            worker::run_once(&f.store, config).await.unwrap(),
            "admitted action reaches its {reason} validation boundary"
        );
        let detail = f.request(&path, None).await;
        let rejected = card(&detail, root, "plan_prepare");
        assert_eq!(rejected.status, "input_required", "{reason}");
        assert!(
            rejected.result.is_none(),
            "{reason} has no successful evidence"
        );
        assert!(
            f.linear.lock().unwrap().events.is_empty(),
            "{reason} reached Linear"
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM agency_prepared_plans")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );

    let root = Uuid::new_v4();
    let initial = f
        .send(
            &path,
            root,
            "Prepare the exact reviewed plan.",
            Some(&f.plan),
        )
        .await;
    assert!(
        initial
            .messages
            .iter()
            .any(|m| m.id == root && m.delivery == "queued")
    );
    f.send(
        &path,
        root,
        "Prepare the exact reviewed plan.",
        Some(&f.plan),
    )
    .await;
    let (ticket, status, task_id): (Uuid, String, Option<Uuid>) = sqlx::query_as(
        "SELECT id,status,task_id FROM inbox_dispatches WHERE root_message_id=$1 AND principal_id=$2 AND grant_id=$3")
        .bind(root).bind(f.grant.principal_id).bind(f.grant.id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "queued");
    assert!(task_id.is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM inbox_dispatches WHERE root_message_id=$1"
        )
        .bind(root)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );

    f.model
        .lock()
        .unwrap()
        .propose("prepare", json!({"plan":f.plan}));
    assert!(f.run_worker().await);
    assert!(f.run_worker().await);
    let prepared = f.request(&path, None).await;
    let prepare_card = card(&prepared, root, "plan_prepare");
    assert_eq!(prepare_card.status, "completed", "{prepare_card:?}");
    let result = prepare_card.result.unwrap();
    let prepared_id: Uuid = serde_json::from_value(result["prepared_id"].clone()).unwrap();
    assert_eq!(result["plan_digest"], f.plan.digest());
    assert_eq!(result["changes"]["steps"].as_array().unwrap().len(), 2);
    assert!(f.steps().await.is_empty());
    let (project, issue) = {
        let linear = f.linear.lock().unwrap();
        assert_eq!(
            linear.events,
            [Event::Read(linear.project), Event::Read(linear.issue)]
        );
        assert!(linear.objects.is_empty());
        (linear.project, linear.issue)
    };
    let binding: (Uuid, Uuid, Uuid, Uuid, Uuid) = sqlx::query_as(
        "SELECT home_instance_id,principal_id,conversation_id,root_message_id,grant_id FROM inbox_task_origins WHERE ticket_id=$1")
        .bind(ticket).fetch_one(&pool).await.unwrap();
    assert_eq!(
        binding,
        (
            f.config.instance_id,
            f.grant.principal_id,
            conversation,
            root,
            f.grant.id
        )
    );

    f.model
        .lock()
        .unwrap()
        .propose("apply", json!({"prepared_id":prepared_id}));
    assert!(f.run_worker().await);
    {
        let model = f.model.lock().unwrap();
        let turn = model.turns.last().unwrap();
        assert_eq!(turn.root_message_id, root);
        assert!(
            turn.context
                .iter()
                .any(|m| m.body.contains("codifix.action_result.v1")
                    && m.body.contains(&prepared_id.to_string()))
        );
        assert!(
            turn.available_actions
                .iter()
                .all(|a| a.resource.id == f.plan.id.to_string())
        );
    }
    assert!(f.run_worker().await);
    let interrupted = f.request(&path, None).await;
    let uncertain = card(&interrupted, root, "plan_apply");
    assert_eq!(uncertain.status, "input_required");
    assert!(
        uncertain
            .question
            .as_ref()
            .unwrap()
            .contains("uncertain-effect")
    );
    assert!(uncertain.result.is_none());
    let steps = f.steps().await;
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].phase, StepPhase::Attempted);
    assert!(steps[0].observed.is_none());
    assert_eq!(
        f.linear.lock().unwrap().events,
        [
            Event::Read(project),
            Event::Read(issue),
            Event::Read(project),
            Event::Create(project)
        ]
    );
    let effect: (Uuid, String) =
        sqlx::query_as("SELECT id,status FROM external_effects WHERE source='agency-linear'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(matches!(effect.1.as_str(), "pending" | "uncertain"));
    assert!(
        !f.run_worker().await,
        "automatic polling must wait for explicit input after an uncertain write"
    );
    assert_eq!(f.linear.lock().unwrap().events.len(), 4);
    let job_status: String =
        sqlx::query_scalar("SELECT status FROM background_jobs WHERE operation_id=$1")
            .bind(uncertain.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_status, "blocked");

    let answer = Uuid::new_v4();
    f.request(&format!("{path}/tasks/{}/respond",uncertain.id),Some(json!({"client_message_id":answer,
        "answer":"Read back the existing project and continue applying the same approved preparation."}))).await;
    f.model
        .lock()
        .unwrap()
        .propose("apply", json!({"prepared_id":prepared_id}));
    assert!(f.run_worker().await);
    assert!(f.run_worker().await);
    let verified = f.request(&path, None).await;
    let apply_card = verified
        .tasks
        .iter()
        .find(|t| t.root_id == root && t.objective == "plan_apply" && t.status == "completed")
        .unwrap_or_else(|| panic!("missing verified apply: {:?}", verified.tasks));
    let result = apply_card.result.as_ref().unwrap();
    assert_eq!(result["prepared_id"], json!(prepared_id));
    assert_eq!(result["plan_digest"], f.plan.digest());
    assert_eq!(result["objects"][0]["id"], json!(project));
    assert_eq!(
        result["objects"][0]["fields"]["name"],
        f.plan.projects[0].name
    );
    assert_eq!(result["objects"][1]["id"], json!(issue));
    assert_eq!(
        result["objects"][1]["fields"]["title"],
        f.plan.issues[0].title
    );
    assert!(verified.messages.iter().any(|m| m.root_id == root
        && m.role == "system"
        && m.body.contains(&apply_card.id.to_string())));
    let steps = f.steps().await;
    assert_eq!(steps.len(), 2);
    assert!(
        steps
            .iter()
            .all(|s| s.phase == StepPhase::Verified && s.observed.is_some())
    );
    let settled: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id,status FROM external_effects WHERE source='agency-linear'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(settled.len(), 2);
    assert!(settled.iter().all(|(_, status)| status == "confirmed"));
    assert!(settled.iter().any(|(id, _)| *id == effect.0));
    assert_eq!(
        f.linear.lock().unwrap().events,
        [
            Event::Read(project),
            Event::Read(issue),
            Event::Read(project),
            Event::Create(project),
            Event::Read(project),
            Event::Read(issue),
            Event::Create(issue),
            Event::Read(issue)
        ]
    );
    assert_eq!(f.linear.lock().unwrap().objects.len(), 2);

    f.model.lock().unwrap().replies.push_back(json!({"version":1,"reply":"Readback verified both changes.","question":null,"actions":[],"peer_requests":[]}));
    assert!(f.run_worker().await);
    let detail = f.request(&path, None).await;
    assert!(
        detail
            .messages
            .iter()
            .any(|m| m.root_id == root && m.body == "Readback verified both changes.")
    );
    assert!(
        detail
            .messages
            .iter()
            .any(|m| m.id == answer && m.root_id == root)
    );
    let (turns, calls, cost): (i32, i64, i64) =
        sqlx::query_as("SELECT turns,used_calls,used_microusd FROM inbox_roots WHERE id=$1")
            .bind(root)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(turns, 4);
    assert_eq!(calls, 12);
    assert!(cost > 0 && cost <= i64::try_from(ROOT_BUDGET.micro_usd).unwrap());

    for revoke_grant in [false, true] {
        let root = Uuid::new_v4();
        f.send(
            &path,
            root,
            "Queue the reviewed apply before authority changes.",
            None,
        )
        .await;
        f.model
            .lock()
            .unwrap()
            .propose("apply", json!({"prepared_id":prepared_id}));
        assert!(f.run_worker().await);
        let action: Uuid = sqlx::query_scalar("SELECT id FROM inbox_dispatches WHERE root_message_id=$1 AND status='queued' AND request->>'capability'='plan_apply'")
            .bind(root).fetch_one(&pool).await.unwrap();
        let membership: Option<(Uuid, Uuid)> = if revoke_grant {
            f.inbox.revoke_grant(f.grant.id).await.unwrap();
            None
        } else {
            Some(sqlx::query_as("DELETE FROM auth_project_memberships WHERE principal_id=$1 AND source='direct' RETURNING id,project_id")
                .bind(f.grant.principal_id).fetch_one(&pool).await.unwrap())
        };
        assert!(!f.run_worker().await);
        let denied: (String, Option<Uuid>) =
            sqlx::query_as("SELECT status,task_id FROM inbox_dispatches WHERE id=$1")
                .bind(action)
                .fetch_one(&pool)
                .await
                .unwrap();
        let status = if revoke_grant { "cancelled" } else { "failed" };
        assert_eq!(denied, (status.into(), None));
        assert_eq!(f.linear.lock().unwrap().events.len(), 8);
        if let Some((id, project)) = membership {
            sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source) VALUES($1,$2,$3,'reader','direct')")
                .bind(id).bind(f.grant.principal_id).bind(project).execute(&pool).await.unwrap();
        }
    }
    assert!(f.model.lock().unwrap().replies.is_empty());
    assert_eq!(f.model.lock().unwrap().turns.len(), 8);
}

#[tokio::test]
#[ignore = "requires built web assets, Playwright Chromium, and disposable PostgreSQL"]
async fn phone_browser_inbox_replies_and_clarifies() {
    let db = IsolatedDatabase::new()
        .await
        .expect("browser test requires a database");
    let result = tokio::spawn(phone_workflow(db.pool.clone())).await;
    db.cleanup().await.unwrap();
    result.unwrap();
}

async fn phone_workflow(pool: PgPool) {
    let web = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web");
    assert!(
        web.join("dist/index.html").is_file(),
        "build the web app first"
    );
    let f = Candidate::start(pool.clone()).await;
    let owner = f.grant.principal_id;
    let chat = f
        .inbox
        .create(
            owner,
            &codifix_inbox::CreateConversation {
                client_request_id: Uuid::new_v4(),
                title: "Phone Inbox rehearsal".into(),
                specialist_id: f.grant.contact.id,
            },
        )
        .await
        .unwrap()
        .conversation
        .id;
    f.inbox
        .send(
            owner,
            chat,
            &codifix_inbox::SendMessage {
                client_message_id: Uuid::new_v4(),
                body: "Check the release".into(),
                format: "text".into(),
                artifact: None,
                recipient_member_ids: vec![],
            },
        )
        .await
        .unwrap();
    f.model.lock().unwrap().replies.push_back(json!({
        "version":1, "reply":"Release review started.", "question":"Which build should I check?",
        "actions":[], "peer_requests":[]
    }));
    assert!(f.run_worker().await);
    let detail = f.inbox.detail(owner, chat).await.unwrap();
    assert!(detail.tasks.iter().any(|t| t.question.is_some()));
    // Same cookie credential type as the OIDC callback. No external identity provider is contacted.
    let session = format!("mls_{}", Uuid::new_v4());
    let csrf = format!("mlc_{}", Uuid::new_v4());
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,session_kind,expires_at) VALUES($1,$2,$3,$4,'browser',now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(owner).bind(crate::auth::hash_secret(&session)).bind(crate::auth::hash_secret(&csrf)).execute(&pool).await.unwrap();
    let mut child = Command::new("node")
        .arg("tests/inbox-phone-live.mjs")
        .current_dir(&web)
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input =
        json!({"origin":f.service.origin,"session":session,"csrf":csrf,"conversation":chat});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .await
        .unwrap();
    let status = timeout(std::time::Duration::from_secs(45), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success(), "phone browser scenario failed");
    let detail = f.inbox.detail(owner, chat).await.unwrap();
    for text in ["Use the ARM64 build", "Send the result here"] {
        assert_eq!(
            detail
                .messages
                .iter()
                .filter(|message| message.body == text)
                .count(),
            1
        );
    }
    assert!(detail.tasks.iter().all(|task| task.question.is_none()));
}
