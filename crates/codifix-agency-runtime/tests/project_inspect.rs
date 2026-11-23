#![allow(clippy::unwrap_used)]
//! Synthetic, read-only GraphQL fixtures against isolated PostgreSQL schemas.
use axum::{Json, Router, extract::State, routing::post};
use chrono::{Duration, Utc};
use codifix_agency::*;
use codifix_agency_projects::{Authority, Plan, Scope, stable_id};
use codifix_agency_runtime::{config, inspection, store};
use config::{Config, Peer};
use inspection::{InspectionBlocker, ObservationStatus, PlanInspection};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use store::{Store, Submit};
use uuid::Uuid;
struct Graphql {
    objects: BTreeMap<String, Value>,
    reads: usize,
    mutations: usize,
    revoke: Option<(sqlx::PgPool, Uuid)>,
}
async fn graphql(State(state): State<Arc<Mutex<Graphql>>>, Json(body): Json<Value>) -> Json<Value> {
    let (response, revoke) = {
        let mut s = state.lock().unwrap();
        let query = body["query"].as_str().unwrap();
        if !query.starts_with("query AgencyInspect") {
            s.mutations += 1;
            return Json(json!({"errors":[{"message":"unexpected mutation"}]}));
        }
        s.reads += 1;
        let id = body["variables"]["id"].as_str().unwrap();
        let response = match s.objects.get(id) {
            Some(object) => json!({"data":{"object":object}}),
            None => json!({"data":{"object":null}}),
        };
        (response, s.revoke.take())
    };
    if let Some((pool, mandate)) = revoke {
        sqlx::query("UPDATE agency_mandates SET revoked=true WHERE id=$1")
            .bind(mandate)
            .execute(&pool)
            .await
            .unwrap();
    }
    Json(response)
}
async fn working(
    store: &Store,
    peer: &Peer,
    grant: &Mandate,
    plan: &Plan,
    calls: u32,
) -> store::StoredTask {
    let req = Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: grant.id,
        specialist_id: "project-manager".into(),
        objective: "Observe selected resources".into(),
        acceptance_criteria: vec!["linear_state_observed".into()],
        capability: "plan_inspect".into(),
        resource: ResourceScope {
            kind: "plan".into(),
            id: plan.id.to_string(),
        },
        deadline: Utc::now() + Duration::minutes(10),
        budget: Budget {
            micro_usd: 0,
            calls,
            turns: 10,
            delegation_depth: 0,
        },
        payload: json!({"kind":"plan_inspect","plan":plan}),
        parent_id: None,
    };
    let mut task = store.submit(&req, peer).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    store.update_contract(&task).await.unwrap();
    store.get_internal(task.contract.id).await.unwrap()
}
#[tokio::test]
async fn plan_inspect_enforces_authority_per_read_budget_revocation_and_factual_snapshots() {
    const CHILD: &str = "CODIFIX_PLAN_INSPECT_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "plan_inspect_enforces_authority_per_read_budget_revocation_and_factual_snapshots",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env(
                "CODIFIX_PLAN_INSPECT_TEST_TOKEN",
                "synthetic-readonly-token-no-live-permissions",
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
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'inspection fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
    let mut plan: Plan = serde_json::from_str(include_str!(
        "../../codifix-agency-projects/examples/plan.json"
    ))
    .unwrap();
    plan.unresolved_questions = vec!["Confirm a source requirement".into()];
    let grant = Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: instance,
        subject_instance_id: instance,
        specialist_id: "project-manager".into(),
        capabilities: BTreeSet::from(["plan_inspect".into()]),
        resources: BTreeSet::from([ResourceScope {
            kind: "plan".into(),
            id: plan.id.to_string(),
        }]),
        destinations: BTreeSet::new(),
        expires_at: Utc::now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 0,
            calls: 100,
            turns: 100,
            delegation_depth: 1,
        },
    };
    let peer = Peer {
        id: "operator".into(),
        instance_id: instance,
        token_env: "UNREAD_INSPECTION_TOKEN".into(),
        operator: true,
        mandates: BTreeSet::from([grant.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    store
        .install_mandate(project, &grant, &peer, instance)
        .await
        .unwrap();
    let p = &plan.projects[0];
    let p_id = stable_id(plan.id, &p.identity.key);
    let i = &plan.issues[0];
    let i_id = stable_id(plan.id, &i.identity.key);
    let state = Arc::new(Mutex::new(Graphql {
        objects: BTreeMap::from([
            (
                p_id.to_string(),
                json!({"id":p_id,"updatedAt":"2026-10-31T12:00:00Z","name":p.name,"description":p.description,"content":"Observed provider content","lead":null,"teams":{"nodes":p.team_ids.iter().map(|id|json!({"id":id})).collect::<Vec<_>>(),"pageInfo":{"hasNextPage":false}},"url":"https://linear.example/project/launch","status":{"id":Uuid::new_v4(),"name":"In progress","type":"started"}}),
            ),
            (
                i_id.to_string(),
                json!({"id":i_id,"updatedAt":"2026-10-31T12:00:00Z","title":i.title,"description":i.description,"project":{"id":p_id},"projectMilestone":null,"team":{"id":i.team_id},"assignee":null,"url":"https://linear.example/issue/TEST-1","state":{"id":Uuid::new_v4(),"name":"In progress","type":"started"}}),
            ),
        ]),
        reads: 0,
        mutations: 0,
        revoke: None,
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
    let router = Router::new()
        .route("/graphql", post(graphql))
        .with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let auth = Authority::Standing {
        reference: "policy://synthetic-readonly".into(),
        source_reference: plan.source.reference.clone(),
        scope: Scope {
            plan_id: plan.id,
            existing_resources: BTreeMap::new(),
            project_keys: BTreeSet::from([p.identity.key.clone()]),
            team_ids: p.team_ids.clone(),
            create: false,
            update: false,
        },
    };
    let mut cfg: Config = serde_json::from_value(json!({
        "instance_id":instance,"listen":"127.0.0.1:0","public_url":"https://agency.example.test",
        "database_env":"UNREAD_INSPECTION_DB","specialists":[],"peers":[peer.clone()],
        "linear":{"endpoint":endpoint,"token_env":"CODIFIX_PLAN_INSPECT_TEST_TOKEN","authorities":{grant.id.to_string():auth}}
    })).unwrap();
    let task = working(&store, &peer, &grant, &plan, 20).await;
    let mut wrong = task.clone();
    wrong.request.resource.id = Uuid::new_v4().to_string();
    assert!(
        inspection::plan_inspect(&store, &cfg, &wrong)
            .await
            .is_err()
    );
    assert_eq!(state.lock().unwrap().reads, 0);
    let good = cfg.linear.clone();
    cfg.linear.as_mut().unwrap()["authorities"][grant.id.to_string()]["scope"]["plan_id"] =
        json!(Uuid::new_v4());
    assert!(inspection::plan_inspect(&store, &cfg, &task).await.is_err());
    assert_eq!(state.lock().unwrap().reads, 0);
    cfg.linear = good;
    let wire = inspection::plan_inspect(&store, &cfg, &task).await.unwrap();
    let report: PlanInspection = serde_json::from_value(wire).unwrap();
    let count =
        plan.projects.len() + plan.milestones.len() + plan.issues.len() + plan.dependencies.len();
    assert_eq!(report.resources.len(), count);
    assert_eq!(state.lock().unwrap().reads, count);
    assert_eq!(state.lock().unwrap().mutations, 0);
    let observed = report.resources.iter().find(|r| r.id == i_id).unwrap();
    assert_eq!(observed.status, ObservationStatus::Present);
    let snapshot = observed.snapshot.as_ref().unwrap();
    assert_eq!(
        snapshot.url.as_deref(),
        Some("https://linear.example/issue/TEST-1")
    );
    assert_eq!(
        snapshot.workflow_state.as_ref().unwrap().category,
        "started"
    );
    assert_eq!(snapshot.entity.fields["title"], i.title);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| matches!(b, InspectionBlocker::UnresolvedSourceQuestion { .. }))
    );
    assert!(
        report
            .blockers
            .iter()
            .any(|b| matches!(b, InspectionBlocker::MissingResource { .. }))
    );
    assert!(report.interpretation.contains("no delivery forecast"));
    assert_eq!(
        store
            .get_internal(task.contract.id)
            .await
            .unwrap()
            .used_calls,
        count as u32
    );
    state.lock().unwrap().reads = 0;
    let limited = working(&store, &peer, &grant, &plan, 1).await;
    assert!(
        inspection::plan_inspect(&store, &cfg, &limited)
            .await
            .is_err()
    );
    assert_eq!(state.lock().unwrap().reads, 1);
    state.lock().unwrap().reads = 0;
    let revoked = working(&store, &peer, &grant, &plan, 20).await;
    state.lock().unwrap().revoke = Some((db.pool.clone(), grant.id));
    assert!(
        inspection::plan_inspect(&store, &cfg, &revoked)
            .await
            .is_err()
    );
    assert_eq!(state.lock().unwrap().reads, 1);
    server.abort();
    db.cleanup().await.unwrap();
}
