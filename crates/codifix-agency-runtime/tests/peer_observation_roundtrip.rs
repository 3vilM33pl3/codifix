// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
//! Two real companion routers/workers on separate disposable databases.
use chrono::{Duration, Utc};
use codifix_agency::{Budget, Mandate, MandateStatus, ResourceScope, TaskStatus};
use codifix_agency_runtime::{
    config::{Auth, Config, Peer},
    server::{App, router},
    store::{Store, StoredTask, Submit},
    worker,
};
use codifix_test_support::IsolatedDatabase;
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration as Delay};
use tokio::{net::TcpListener, task::JoinHandle};
use uuid::Uuid;

const CHILD: &str = "CODIFIX_TEST_PEER_OBSERVATION_CHILD";
const PEER_TOKEN: &str = "synthetic-companion-observation-only";
const OPERATOR_TOKEN: &str = "synthetic-local-operator-observation-only";

struct Instance {
    db: IsolatedDatabase,
    store: Store,
    cfg: Config,
    peer: Peer,
    inspect: Uuid,
    delegate: Uuid,
    origin: String,
    server: JoinHandle<()>,
}

fn budget(depth: u8) -> Budget {
    Budget {
        micro_usd: 0,
        calls: 0,
        turns: 6,
        delegation_depth: depth.into(),
    }
}

fn scope(kind: &str, id: impl ToString) -> ResourceScope {
    ResourceScope {
        kind: kind.into(),
        id: id.to_string(),
    }
}

impl Instance {
    async fn new(
        db: IsolatedDatabase,
        home: Uuid,
        remote: Uuid,
        remote_label: &str,
        listener: TcpListener,
        endpoint: &str,
    ) -> Self {
        let project = Uuid::new_v4();
        sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,'peer-scope','Peer Scope','/synthetic')")
            .bind(project).execute(&db.pool).await.unwrap();
        let store = Store {
            pool: db.pool.clone(),
        };
        let operator = Peer {
            id: "operator".into(),
            instance_id: home,
            token_env: "UNUSED_TEST_OPERATOR".into(),
            operator: true,
            mandates: BTreeSet::new(),
            endpoint: None,
            outbound_token_env: None,
        };
        let inspect = Uuid::new_v4();
        let delegate = Uuid::new_v4();
        for (id, cap, resource) in [
            (inspect, "inspect", scope("instance", home)),
            (delegate, "delegate", scope("peer", remote_label)),
        ] {
            let grant = Mandate {
                id,
                parent_id: None,
                issuer_instance_id: home,
                subject_instance_id: home,
                specialist_id: "observer".into(),
                capabilities: BTreeSet::from([cap.into()]),
                resources: BTreeSet::from([resource]),
                destinations: BTreeSet::new(),
                expires_at: Utc::now() + Duration::minutes(5),
                status: MandateStatus::Active,
                budget: Budget {
                    turns: 40,
                    ..budget(1)
                },
            };
            store
                .install_mandate(project, &grant, &operator, home)
                .await
                .unwrap();
        }
        let peer = Peer {
            id: remote_label.into(),
            instance_id: remote,
            token_env: CHILD.into(),
            operator: false,
            mandates: BTreeSet::from([inspect]),
            endpoint: Some(endpoint.into()),
            outbound_token_env: Some(CHILD.into()),
        };
        // Use loopback test transports without altering production TLS validation.
        let cfg: Config = serde_json::from_value(json!({
            "observation_enabled":true,
            "instance_id":home,
            "listen":listener.local_addr().unwrap(),
            "public_url":"https://synthetic.invalid",
            "database_env":"UNUSED_TEST_DB",
            "specialists":[{"schema_version":1,"instance_id":home,"specialist_id":"observer","name":"Observer","capabilities":["inspect","delegate"]}],
            "peers":[operator,peer],
        })).unwrap();
        store.heartbeat(&cfg).await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let app = App {
            store: store.clone(),
            config: Arc::new(cfg.clone()),
            auth: Auth::from_credentials(vec![
                (operator, OPERATOR_TOKEN.into()),
                (peer.clone(), PEER_TOKEN.into()),
            ])
            .unwrap(),
        };
        let server = tokio::spawn(async move { axum::serve(listener, router(app)).await.unwrap() });
        Self {
            db,
            store,
            cfg,
            peer,
            inspect,
            delegate,
            origin,
            server,
        }
    }

    fn inspection(&self, criterion: &str) -> Submit {
        Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: self.inspect,
            specialist_id: "observer".into(),
            objective: "Synthetic peer status check".into(),
            acceptance_criteria: vec![criterion.into()],
            capability: "inspect".into(),
            resource: scope("instance", self.cfg.instance_id),
            deadline: Utc::now() + Duration::minutes(1),
            budget: budget(0),
            payload: json!({"kind":"inspect"}),
            parent_id: None,
        }
    }

    async fn submit(&self, client: &reqwest::Client, token: &str, data: &Submit) -> StoredTask {
        client
            .post(format!("{}/v1/tasks", self.origin))
            .bearer_auth(token)
            .json(data)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn journal(&self, id: Uuid) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT kind,body FROM agency_observation_events WHERE task_id=$1 ORDER BY sequence",
        )
        .bind(id)
        .fetch_all(&self.db.pool)
        .await
        .unwrap()
    }

    fn start_worker(&self) -> JoinHandle<()> {
        let store = self.store.clone();
        let cfg = self.cfg.clone();
        tokio::spawn(async move {
            loop {
                worker::run_once(&store, &cfg).await.unwrap();
                tokio::time::sleep(Delay::from_millis(10)).await;
            }
        })
    }

    async fn stop(self) {
        self.server.abort();
        self.db.cleanup().await.unwrap();
    }
}

async fn bilateral_roundtrip(source: &Instance, target: &Instance, client: &reqwest::Client) {
    let mut nested = target.inspection("status_observed");
    nested.budget.turns = 2;
    let request = Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: source.delegate,
        specialist_id: nested.specialist_id.clone(),
        objective: nested.objective.clone(),
        acceptance_criteria: vec!["delegated_result_reported".into()],
        capability: "delegate".into(),
        resource: scope("peer", &source.peer.id),
        deadline: nested.deadline,
        budget: budget(1),
        payload: json!({"kind":"delegate","peer":source.peer.id,"request":nested}),
        parent_id: None,
    };
    let task = source.submit(client, OPERATOR_TOKEN, &request).await;
    let worker = target.start_worker();
    assert!(
        tokio::time::timeout(
            Delay::from_secs(15),
            worker::run_once(&source.store, &source.cfg)
        )
        .await
        .unwrap()
        .unwrap()
    );
    worker.abort();
    let done = source.store.get_internal(task.contract.id).await.unwrap();
    assert_eq!(done.contract.status, TaskStatus::Completed);
    let result: Value = sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
        .bind(done.contract.id)
        .fetch_one(&source.db.pool)
        .await
        .unwrap();
    let remote_id = result["reported_result"]["id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let remote: Value = client
        .get(format!("{}/tasks/{remote_id}", target.origin))
        .bearer_auth(PEER_TOKEN)
        .header("A2A-Version", "1.0")
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(result["reported_result"], remote);
    for (instance, id) in [(source, task.contract.id), (target, remote_id)] {
        let events = instance.journal(id).await;
        assert_eq!(events.first().unwrap().0, "request");
        assert!(events.iter().any(|(kind, _)| kind == "result"));
    }
}

#[tokio::test]
async fn bilateral_exchange_and_clarification_survive_worker_restart() {
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "bilateral_exchange_and_clarification_survive_worker_restart",
                "--nocapture",
            ])
            .env(CHILD, PEER_TOKEN)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let Some(a_db) = IsolatedDatabase::new().await else {
        return;
    };
    let Some(b_db) = IsolatedDatabase::new().await else {
        return;
    };
    let a_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a_url = format!("http://{}", a_listener.local_addr().unwrap());
    let b_url = format!("http://{}", b_listener.local_addr().unwrap());
    let a_id = Uuid::new_v4();
    let b_id = Uuid::new_v4();
    let a = Instance::new(a_db, a_id, b_id, "bravo", a_listener, &b_url).await;
    let b = Instance::new(b_db, b_id, a_id, "alpha", b_listener, &a_url).await;
    let client = reqwest::Client::new();
    bilateral_roundtrip(&a, &b, &client).await;
    bilateral_roundtrip(&b, &a, &client).await;

    let invalid = b.inspection("unsupported_synthetic_criterion");
    let task = b.submit(&client, PEER_TOKEN, &invalid).await;
    assert!(worker::run_once(&b.store, &b.cfg).await.unwrap());
    let id = task.contract.id;
    assert_eq!(
        b.store.get_internal(id).await.unwrap().contract.status,
        TaskStatus::InputRequired
    );
    let control = json!({"message":{"messageId":Uuid::new_v4(),"role":"ROLE_USER",
        "parts":[{"data":{"answer":"Use the documented runtime check."}}],
        "taskId":id,"contextId":id},"configuration":{"returnImmediately":true}});
    client
        .post(format!("{}/message:send", b.origin))
        .bearer_auth(PEER_TOKEN)
        .header("A2A-Version", "1.0")
        .json(&control)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // A fresh worker reconstructs the task from PostgreSQL. The peer answer
    // survives another request for clarification instead of being overwritten.
    let restarted = Store {
        pool: b.db.pool.clone(),
    };
    assert!(worker::run_once(&restarted, &b.cfg).await.unwrap());
    let events = b.journal(id).await;
    let conversational: Vec<_> = events
        .iter()
        .filter(|(kind, _)| matches!(kind.as_str(), "request" | "clarification" | "answer"))
        .collect();
    assert_eq!(
        conversational
            .iter()
            .map(|(kind, _)| kind.as_str())
            .collect::<Vec<_>>(),
        vec!["request", "clarification", "answer", "clarification"]
    );
    assert_eq!(conversational[2].1, "Use the documented runtime check.");
    a.stop().await;
    b.stop().await;
}
