#![allow(clippy::unwrap_used)]
//! End-to-end worker/GraphQL/SQL journal test. Fake credentials exist only in a
//! child process configured before startup; the test never mutates process env.
use axum::{Json, Router, extract::State, routing::post};
use chrono::{Duration, Utc};
use codifix_agency::*;
use codifix_agency_projects::{Authority, Plan, Scope, StepPhase, StepReceipt};
use codifix_agency_runtime::{
    config::{Config, Peer},
    store::{Store, Submit},
    worker,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use uuid::Uuid;
#[derive(Default)]
struct Graphql {
    objects: BTreeMap<String, Value>,
    reads: usize,
    writes: Vec<Value>,
}
async fn graphql(State(state): State<Arc<Mutex<Graphql>>>, Json(body): Json<Value>) -> Json<Value> {
    let mut state = state.lock().unwrap();
    let query = body["query"].as_str().unwrap();
    if query.starts_with("query") {
        state.reads += 1;
        let id = body["variables"]["id"].as_str().unwrap();
        return Json(match state.objects.get(id) {
            Some(object) => json!({"data":{"object":object}}),
            None => json!({"errors":[{"extensions":{"code":"ENTITY_NOT_FOUND"}}]}),
        });
    }
    state.writes.push(body.clone());
    let n = state.writes.len();
    let input = &body["variables"]["input"];
    let create = query.contains("CreateInput");
    let id = if create {
        input["id"].as_str().unwrap()
    } else {
        body["variables"]["id"].as_str().unwrap()
    };
    if create && state.objects.contains_key(id) {
        return Json(json!({"errors":[{"extensions":{"code":"DUPLICATE"}}]}));
    }
    let mut object = state
        .objects
        .get(id)
        .cloned()
        .unwrap_or_else(|| json!({"id":id}));
    object["updatedAt"] = json!(format!("2026-10-31T19:00:{n:02}Z"));
    for (key, value) in input.as_object().unwrap() {
        let relation = match key.as_str() {
            "projectId" => Some("project"),
            "projectMilestoneId" => Some("projectMilestone"),
            "teamId" => Some("team"),
            "leadId" => Some("lead"),
            "assigneeId" => Some("assignee"),
            "issueId" => Some("issue"),
            "relatedIssueId" => Some("relatedIssue"),
            _ => None,
        };
        if let Some(name) = relation {
            object[name] = if value.is_null() {
                Value::Null
            } else {
                json!({"id":value})
            };
        } else if key == "teamIds" {
            object["teams"] = json!({"nodes":value.as_array().unwrap().iter().map(|id|json!({"id":id})).collect::<Vec<_>>(),"pageInfo":{"hasNextPage":false}});
        } else {
            object[key] = value.clone();
        }
    }
    state.objects.insert(id.into(), object);
    let field = if query.contains("ProjectMilestone") {
        "projectMilestone"
    } else if query.contains("IssueRelation") {
        "issueRelation"
    } else if query.contains("Project") {
        "project"
    } else {
        "issue"
    };
    Json(json!({"data":{"effect":{"success":true,field:{"id":id}}}}))
}
fn request(grant: &Mandate, plan: &Plan, kind: &str, payload: Value) -> Submit {
    Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: grant.id,
        specialist_id: "project-manager".into(),
        objective: format!("Synthetic {kind}"),
        acceptance_criteria: vec![
            if kind == "plan_prepare" {
                "plan_valid"
            } else {
                "linear_readback_verified"
            }
            .into(),
        ],
        capability: kind.into(),
        resource: ResourceScope {
            kind: "plan".into(),
            id: plan.id.to_string(),
        },
        deadline: Utc::now() + Duration::minutes(10),
        budget: Budget {
            micro_usd: 100,
            calls: 30,
            turns: 10,
            delegation_depth: 1,
        },
        payload,
        parent_id: None,
    }
}
async fn execute(
    store: &Store,
    cfg: &Config,
    peer: &Peer,
    request: &Submit,
) -> codifix_agency_runtime::store::StoredTask {
    let task = store.submit(request, peer).await.unwrap();
    for _ in 0..3 {
        assert!(worker::run_once(store, cfg).await.unwrap());
        let current = store.get_internal(task.contract.id).await.unwrap();
        if current.contract.status.is_terminal()
            || current.contract.status == TaskStatus::InputRequired
        {
            return current;
        }
    }
    panic!("worker did not finish synthetic plan operation");
}
#[tokio::test]
async fn linear_prepare_apply_preserves_reviewed_baseline_and_durable_step_receipts() {
    const CHILD: &str = "CODIFIX_LINEAR_RUNTIME_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "linear_prepare_apply_preserves_reviewed_baseline_and_durable_step_receipts",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env(
                "CODIFIX_LINEAR_RUNTIME_TEST_TOKEN",
                "synthetic-local-graphql-token-no-live-permissions",
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child stdout:\n{}\nchild stderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let store = Store {
        pool: db.pool.clone(),
    };
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'agency Linear fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
    let plan: Plan = serde_json::from_str(include_str!(
        "../../codifix-agency-projects/examples/plan.json"
    ))
    .unwrap();
    let grant = Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: instance,
        subject_instance_id: instance,
        specialist_id: "project-manager".into(),
        capabilities: BTreeSet::from(["plan_prepare".into(), "plan_apply".into()]),
        resources: BTreeSet::from([ResourceScope {
            kind: "plan".into(),
            id: plan.id.to_string(),
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
        token_env: "UNREAD_LINEAR_PEER_TOKEN".into(),
        operator: true,
        mandates: BTreeSet::from([grant.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    store
        .install_mandate(project, &grant, &peer, instance)
        .await
        .unwrap();
    let state = Arc::new(Mutex::new(Graphql::default()));
    let app = Router::new()
        .route("/graphql", post(graphql))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let auth = Authority::Standing {
        reference: "policy://synthetic-plan".into(),
        source_reference: plan.source.reference.clone(),
        scope: Scope {
            plan_id: plan.id,
            existing_resources: BTreeMap::new(),
            project_keys: BTreeSet::from(["launch".into()]),
            team_ids: plan.projects[0].team_ids.clone(),
            create: true,
            update: true,
        },
    };
    let cfg = Config {
        inbox_enabled: false,
        instance_id: instance,
        listen: "127.0.0.1:0".parse().unwrap(),
        public_url: "https://agency.example.test".into(),
        database_env: "UNREAD_LINEAR_DB".into(),
        specialists: vec![SpecialistManifest {
            schema_version: 1,
            instance_id: instance,
            specialist_id: "project-manager".into(),
            name: "Synthetic project manager".into(),
            capabilities: grant.capabilities.clone(),
        }],
        peers: vec![peer.clone()],
        models: vec![],
        model_profiles: vec![],
        public_profile: None,
        private_repository_dids: vec![],
        linear: Some(
            json!({"endpoint":endpoint,"token_env":"CODIFIX_LINEAR_RUNTIME_TEST_TOKEN","authorities":{grant.id.to_string():auth}}),
        ),
        openbao_controllers: BTreeMap::new(),
    };
    let prepared = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &plan,
            "plan_prepare",
            json!({"kind":"plan_prepare","plan":plan}),
        ),
    )
    .await;
    assert_eq!(
        prepared.contract.status,
        TaskStatus::Completed,
        "{:?}",
        prepared.contract.clarification
    );
    assert_eq!(state.lock().unwrap().writes.len(), 0);
    let evidence: Value =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(prepared.contract.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let prepared_id = evidence["prepared_id"].as_str().unwrap();
    assert_eq!(evidence["changes"]["steps"].as_array().unwrap().len(), 5);
    let apply = request(
        &grant,
        &plan,
        "plan_apply",
        json!({"kind":"plan_apply","prepared_id":prepared_id}),
    );
    let applied = execute(&store, &cfg, &peer, &apply).await;
    assert_eq!(
        applied.contract.status,
        TaskStatus::Completed,
        "{:?}",
        applied.contract.clarification
    );
    assert_eq!(state.lock().unwrap().writes.len(), 5);
    let receipts: Vec<Value> = sqlx::query_scalar("SELECT receipt FROM agency_steps ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(receipts.len(), 5);
    for value in &receipts {
        let receipt: StepReceipt = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(receipt.phase, StepPhase::Verified);
        assert!(receipt.observed.is_some());
    }
    // A new worker/task instance reconciles persisted effects without re-sending.
    let repeated = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &plan,
            "plan_apply",
            json!({"kind":"plan_apply","prepared_id":prepared_id}),
        ),
    )
    .await;
    assert_eq!(repeated.contract.status, TaskStatus::Completed);
    assert_eq!(state.lock().unwrap().writes.len(), 5);
    let after: Vec<Value> = sqlx::query_scalar("SELECT receipt FROM agency_steps ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(after, receipts);
    // Prepare an update against the reviewed remote baseline, then simulate a human edit.
    let mut revised = plan.clone();
    revised.revision += 1;
    revised.projects[0].name = "Agency proposed rename".into();
    let prepared_update = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &revised,
            "plan_prepare",
            json!({"kind":"plan_prepare","plan":revised}),
        ),
    )
    .await;
    assert_eq!(prepared_update.contract.status, TaskStatus::Completed);
    let evidence: Value =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(prepared_update.contract.id)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    // New accepted revisions invalidate older preparation IDs, even if the
    // older desired state currently matches and would otherwise need no writes.
    let stale = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &plan,
            "plan_apply",
            json!({"kind":"plan_apply","prepared_id":prepared_id}),
        ),
    )
    .await;
    assert_eq!(stale.contract.status, TaskStatus::InputRequired);
    assert_eq!(state.lock().unwrap().writes.len(), 5);
    // A reused revision number with different content cannot replace the head.
    let mut conflicting = revised.clone();
    conflicting.projects[0].name = "Conflicting content with reused revision".into();
    let rejected = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &conflicting,
            "plan_prepare",
            json!({"kind":"plan_prepare","plan":conflicting}),
        ),
    )
    .await;
    assert_eq!(rejected.contract.status, TaskStatus::InputRequired);
    let head: Uuid = sqlx::query_scalar(
        "SELECT prepared_id FROM agency_plan_heads WHERE plan_id=$1 AND mandate_id=$2",
    )
    .bind(plan.id)
    .bind(grant.id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(head.to_string(), evidence["prepared_id"].as_str().unwrap());
    let remote_id = codifix_agency_projects::stable_id(plan.id, "launch").to_string();
    {
        let mut state = state.lock().unwrap();
        let object = state.objects.get_mut(&remote_id).unwrap();
        object["name"] = json!("Human reviewed rename");
        object["updatedAt"] = json!("manual-update-after-prepare");
    }
    let conflict = execute(
        &store,
        &cfg,
        &peer,
        &request(
            &grant,
            &revised,
            "plan_apply",
            json!({"kind":"plan_apply","prepared_id":evidence["prepared_id"]}),
        ),
    )
    .await;
    assert_eq!(conflict.contract.status, TaskStatus::InputRequired);
    assert_eq!(state.lock().unwrap().writes.len(), 5);
    assert_eq!(
        state.lock().unwrap().objects[&remote_id]["name"],
        "Human reviewed rename"
    );
    server.abort();
    db.cleanup().await.unwrap();
}
