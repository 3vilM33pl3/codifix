#![allow(clippy::unwrap_used)]
//! Actual loopback HTTP tests; private tasks use only isolated disposable databases.
use chrono::{Duration, Utc};
use codifix_agency::{
    discovery::{PublicCapabilityProfile, PublicProfileApproval},
    *,
};
use codifix_agency_runtime::{
    config::{Auth, Config, Peer},
    server::{App, router},
    store::{Store, Submit},
    worker,
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use uuid::Uuid;
const OPERATOR_TOKEN: &str = "agency-http-operator-credential-test-only";
const MEMBER_TOKEN: &str = "agency-http-member-credential-test-only-01";
const OUTSIDER_TOKEN: &str = "agency-http-outsider-credential-test-only";
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    member: Peer,
    grant: Mandate,
    project: Uuid,
    url: String,
    server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        version: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = reqwest::Client::new().request(method, format!("{}{path}", self.url));
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(version) = version {
            request = request.header("A2A-Version", version);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let bytes = response.bytes().await.unwrap();
        let value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"non_json_response":String::from_utf8_lossy(&bytes)}));
        (status, value)
    }
    fn submit(&self) -> Submit {
        Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: self.grant.id,
            specialist_id: "operations".into(),
            objective: "Observe synthetic fixture".into(),
            acceptance_criteria: vec!["status_observed".into()],
            capability: "inspect".into(),
            resource: ResourceScope {
                kind: "service".into(),
                id: "http-fixture".into(),
            },
            deadline: Utc::now() + Duration::minutes(10),
            budget: Budget {
                micro_usd: 100,
                calls: 10,
                turns: 10,
                delegation_depth: 1,
            },
            payload: json!({"kind":"inspect"}),
            parent_id: None,
        }
    }
    fn message(&self) -> Value {
        json!({"message":{"messageId":Uuid::new_v4().to_string(),"role":"ROLE_USER","parts":[{"data":self.submit()}]},"configuration":{"returnImmediately":true}})
    }
    async fn send(&self) -> Value {
        let (status, value) = self
            .request(
                Method::POST,
                "/message:send",
                Some(MEMBER_TOKEN),
                Some("1.0"),
                Some(self.message()),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value["task"].clone()
    }
    async fn cleanup(self) {
        self.server.abort();
        self.db.cleanup().await.unwrap();
    }
}
async fn fixture() -> Option<Fixture> {
    let db = codifix_test_support::IsolatedDatabase::new().await?;
    let store = Store {
        pool: db.pool.clone(),
    };
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'agency HTTP fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
    let grant = Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: instance,
        subject_instance_id: instance,
        specialist_id: "operations".into(),
        capabilities: BTreeSet::from(["inspect".into()]),
        resources: BTreeSet::from([ResourceScope {
            kind: "service".into(),
            id: "http-fixture".into(),
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
    let operator = Peer {
        id: "operator".into(),
        instance_id: instance,
        token_env: "UNREAD_HTTP_OPERATOR_TOKEN".into(),
        operator: true,
        mandates: BTreeSet::new(),
        endpoint: None,
        outbound_token_env: None,
    };
    let member = Peer {
        id: "member".into(),
        instance_id: instance,
        token_env: "UNREAD_HTTP_MEMBER_TOKEN".into(),
        operator: false,
        mandates: BTreeSet::from([grant.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    let outsider = Peer {
        id: "outsider".into(),
        instance_id: Uuid::new_v4(),
        token_env: "UNREAD_HTTP_OUTSIDER_TOKEN".into(),
        operator: false,
        mandates: BTreeSet::new(),
        endpoint: None,
        outbound_token_env: None,
    };
    let manifest = SpecialistManifest {
        schema_version: 1,
        instance_id: instance,
        specialist_id: "operations".into(),
        name: "Private operations specialist".into(),
        capabilities: grant.capabilities.clone(),
    };
    let profile = PublicCapabilityProfile::from_approved(
        &manifest,
        PublicProfileApproval {
            display_name: "Public operations".into(),
            summary: "Inspect approved service state".into(),
            capabilities: grant.capabilities.clone(),
            agent_card_url: "https://agency.example.test/.well-known/agent-card.json".into(),
        },
        Utc::now(),
    )
    .unwrap();
    let cfg = Config {
        inbox_enabled: false,
        instance_id: instance,
        listen: "127.0.0.1:0".parse().unwrap(),
        public_url: "https://agency.example.test".into(),
        database_env: "UNREAD_HTTP_DATABASE".into(),
        specialists: vec![manifest],
        peers: vec![operator.clone(), member.clone(), outsider.clone()],
        models: vec![],
        model_profiles: vec![],
        public_profile: Some(serde_json::to_value(profile).unwrap()),
        private_repository_dids: vec!["did:plc:private-never-public".into()],
        linear: None,
        openbao_controllers: BTreeMap::new(),
    };
    store
        .install_mandate(project, &grant, &operator, instance)
        .await
        .unwrap();
    let auth = Auth::from_credentials(vec![
        (operator, OPERATOR_TOKEN.to_string()),
        (member.clone(), MEMBER_TOKEN.to_string()),
        (outsider, OUTSIDER_TOKEN.to_string()),
    ])
    .unwrap();
    let app = router(App {
        store: store.clone(),
        config: Arc::new(cfg.clone()),
        auth,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Some(Fixture {
        db,
        store,
        cfg,
        member,
        grant,
        project,
        url,
        server,
    })
}

#[tokio::test]
async fn authenticated_a2a_send_get_list_cancel_and_peer_isolation() {
    let Some(f) = fixture().await else { return };
    let request = f.message();
    for token in [None, Some("unknown-peer-token-should-never-work")] {
        let (status, _) = f
            .request(
                Method::POST,
                "/message:send",
                token,
                Some("1.0"),
                Some(request.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, created) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(request.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let task = &created["task"];
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["status"]["state"], "TASK_STATE_SUBMITTED");
    let (_, replay) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(request),
        )
        .await;
    assert_eq!(replay["task"]["id"], id);
    let (status, read) = f
        .request(
            Method::GET,
            &format!("/tasks/{id}"),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["contextId"], task["contextId"]);
    let (status, list) = f
        .request(Method::GET, "/tasks", Some(MEMBER_TOKEN), Some("1.0"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["tasks"].as_array().unwrap().len(), 1);
    let (status, private) = f
        .request(
            Method::GET,
            &format!("/tasks/{id}"),
            Some(OUTSIDER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(!private.to_string().contains(id));
    let (status, list) = f
        .request(
            Method::GET,
            "/tasks",
            Some(OUTSIDER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["tasks"], json!([]));
    let (status, _) = f
        .request(
            Method::POST,
            &format!("/tasks/{id}:cancel"),
            Some(OUTSIDER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, cancel) = f
        .request(
            Method::POST,
            &format!("/tasks/{id}:cancel"),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{cancel}");
    assert_eq!(cancel["status"]["state"], "TASK_STATE_CANCELED");
    assert_eq!(
        f.store
            .get_internal(Uuid::parse_str(id).unwrap())
            .await
            .unwrap()
            .contract
            .status,
        TaskStatus::Canceled
    );
    f.cleanup().await;
}

#[tokio::test]
async fn a2a_private_methods_require_exact_version() {
    let Some(f) = fixture().await else { return };
    let task = f.send().await;
    let id = task["id"].as_str().unwrap();
    for version in [None, Some("0.3"), Some("1.1")] {
        for (method, path, body) in [
            (Method::GET, "/tasks".to_string(), None),
            (Method::GET, format!("/tasks/{id}"), None),
            (Method::POST, format!("/tasks/{id}:cancel"), None),
            (Method::POST, "/message:send".into(), Some(f.message())),
        ] {
            let (status, error) = f
                .request(method, &path, Some(MEMBER_TOKEN), version, body)
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {error}");
            assert_eq!(error["error"]["code"], "VersionNotSupportedError");
        }
    }
    assert_eq!(f.store.list(&f.member).await.unwrap().len(), 1);
    f.cleanup().await;
}

#[tokio::test]
async fn public_card_advertises_authentication_without_granting_task_access() {
    let Some(f) = fixture().await else { return };
    let (status, card) = f
        .request(
            Method::GET,
            "/.well-known/agent-card.json",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{card}");
    assert_eq!(
        card["securitySchemes"]["bearer"]["httpAuthSecurityScheme"]["scheme"],
        "Bearer"
    );
    assert_eq!(card["supportedInterfaces"][0]["protocolVersion"], "1.0");
    let encoded = card.to_string();
    for private in [
        f.grant.id.to_string(),
        f.project.to_string(),
        OPERATOR_TOKEN.into(),
        MEMBER_TOKEN.into(),
        "UNREAD_HTTP_DATABASE".into(),
        "private-never-public".into(),
    ] {
        assert!(!encoded.contains(&private));
    }
    let (status, error) = f
        .request(
            Method::POST,
            "/message:send",
            Some(OUTSIDER_TOKEN),
            Some("1.0"),
            Some(f.message()),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(f.store.list(&f.member).await.unwrap().len(), 0);
    f.cleanup().await;
}

#[tokio::test]
async fn context_mismatch_rejected_and_clarification_continues_existing_task() {
    let Some(f) = fixture().await else { return };
    let created = f.send().await;
    let id = created["id"].as_str().unwrap();
    let uuid = Uuid::parse_str(id).unwrap();
    let mut task = f.store.get_internal(uuid).await.unwrap();
    task.contract
        .transition(TaskStatus::Working, Utc::now())
        .unwrap();
    task.contract
        .clarify(
            "Which synthetic revision should be inspected?".into(),
            Utc::now(),
        )
        .unwrap();
    f.store.update_contract(&task).await.unwrap();
    let (status, waiting) = f
        .request(
            Method::GET,
            &format!("/tasks/{id}"),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(waiting["status"]["state"], "TASK_STATE_INPUT_REQUIRED");
    assert!(
        waiting["status"]["message"]["parts"][0]["text"]
            .as_str()
            .unwrap()
            .contains("synthetic revision")
    );
    let answer = json!({"message":{"messageId":"answer-1","role":"ROLE_USER","taskId":id,"contextId":created["contextId"],"parts":[{"data":{"answer":"Use revision two","replacement_payload":{"kind":"inspect","revision":2}}}]},"configuration":{"returnImmediately":true}});
    let mut bad = answer.clone();
    bad["message"]["contextId"] = json!(Uuid::new_v4());
    let (status, error) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(bad),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_context");
    let (status, continued) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(answer.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{continued}");
    assert_eq!(continued["task"]["id"], id);
    assert_eq!(continued["task"]["contextId"], created["contextId"]);
    let (status, _) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(answer),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(f.store.get_internal(uuid).await.unwrap().used_turns, 1);
    for _ in 0..3 {
        worker::run_once(&f.store, &f.cfg).await.unwrap();
        if f.store.get_internal(uuid).await.unwrap().contract.status == TaskStatus::Completed {
            break;
        }
    }
    let (status, completed) = f
        .request(
            Method::GET,
            &format!("/tasks/{id}"),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["status"]["state"], "TASK_STATE_COMPLETED");
    assert!(completed["artifacts"].as_array().unwrap().len() >= 2);
    let mut unbound = f.message();
    unbound["message"]["contextId"] = json!("caller-created-context");
    let (status, _) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(unbound),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    f.cleanup().await;
}

#[tokio::test]
async fn operator_controls_and_http_schedule_run_real_persisted_work() {
    let Some(f) = fixture().await else { return };
    let schedule = json!({"id":Uuid::new_v4(),"interval_seconds":60,"template":f.submit()});
    for (path, body) in [
        ("/v1/schedules".to_string(), schedule.clone()),
        ("/v1/improvements".into(), json!({"action":"propose"})),
    ] {
        let (status, _) = f
            .request(Method::POST, &path, Some(MEMBER_TOKEN), None, Some(body))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = f
        .request(
            Method::POST,
            &format!("/v1/mandates/{}/pause", f.grant.id),
            Some(MEMBER_TOKEN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let mut forged = f.grant.clone();
    forged.id = Uuid::new_v4();
    let (status, _) = f
        .request(
            Method::POST,
            "/v1/mandates",
            Some(MEMBER_TOKEN),
            None,
            Some(json!({"project_id":f.project,"mandate":forged})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = f
        .request(
            Method::POST,
            &format!("/v1/mandates/{}/pause", f.grant.id),
            Some(OPERATOR_TOKEN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(f.message()),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) = f
        .request(
            Method::POST,
            &format!("/v1/mandates/{}/resume", f.grant.id),
            Some(OPERATOR_TOKEN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, response) = f
        .request(
            Method::POST,
            "/v1/schedules",
            Some(OPERATOR_TOKEN),
            None,
            Some(schedule),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    worker::schedules(&f.store, &f.cfg).await.unwrap();
    assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
    let (status, list) = f
        .request(
            Method::GET,
            "/tasks?includeArtifacts=true",
            Some(OPERATOR_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let tasks = list["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["status"]["state"], "TASK_STATE_COMPLETED");
    assert!(
        tasks[0]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["parts"][0]["data"]["mandate_active"] == true)
    );
    worker::schedules(&f.store, &f.cfg).await.unwrap();
    assert_eq!(f.store.list(&f.cfg.peers[0]).await.unwrap().len(), 1);
    let (status, _) = f
        .request(
            Method::POST,
            &format!("/v1/mandates/{}/revoke", f.grant.id),
            Some(OPERATOR_TOKEN),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = f
        .request(
            Method::POST,
            "/message:send",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            Some(f.message()),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    f.cleanup().await;
}

#[tokio::test]
async fn default_send_message_waits_for_worker_completion() {
    let Some(f) = fixture().await else { return };
    let mut request = f.message();
    request.as_object_mut().unwrap().remove("configuration");
    let response = f.request(
        Method::POST,
        "/message:send",
        Some(MEMBER_TOKEN),
        Some("1.0"),
        Some(request),
    );
    let run_worker = async {
        for _ in 0..100 {
            if !f.store.list(&f.member).await.unwrap().is_empty() {
                assert!(worker::run_once(&f.store, &f.cfg).await.unwrap());
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("HTTP task was not admitted before worker deadline");
    };
    let ((status, response), ()) = tokio::join!(response, run_worker);
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["task"]["status"]["state"], "TASK_STATE_COMPLETED");
    assert!(response["task"]["artifacts"].as_array().unwrap().len() >= 2);
    f.cleanup().await;
}

#[tokio::test]
async fn list_tasks_paginates_filters_and_rejects_foreign_or_tampered_cursors() {
    let Some(f) = fixture().await else { return };
    let mut ids = BTreeSet::new();
    let mut sample = Value::Null;
    for _ in 0..5 {
        let task = f.send().await;
        ids.insert(task["id"].as_str().unwrap().to_string());
        sample = task;
    }
    let (status, page) = f
        .request(
            Method::GET,
            "/tasks?pageSize=2",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["pageSize"], 2);
    assert_eq!(page["totalSize"], 5);
    assert_eq!(page["tasks"].as_array().unwrap().len(), 2);
    let token = page["nextPageToken"].as_str().unwrap();
    assert!(!token.is_empty());
    let cursor_path = |token: &str| {
        let mut url = reqwest::Url::parse(&format!("{}/tasks", f.url)).unwrap();
        url.query_pairs_mut()
            .append_pair("pageSize", "2")
            .append_pair("pageToken", token);
        url.as_str().strip_prefix(&f.url).unwrap().to_string()
    };
    let (status, foreign) = f
        .request(
            Method::GET,
            &cursor_path(token),
            Some(OUTSIDER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{foreign}");
    let (status, tampered) = f
        .request(
            Method::GET,
            &cursor_path(&format!("{token}tampered")),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{tampered}");
    let mut seen = BTreeSet::new();
    let mut current = page;
    loop {
        assert_eq!(current["totalSize"], 5);
        for task in current["tasks"].as_array().unwrap() {
            assert!(
                seen.insert(task["id"].as_str().unwrap().to_string()),
                "duplicate task across cursor pages"
            );
        }
        let next = current["nextPageToken"].as_str().unwrap();
        if next.is_empty() {
            break;
        }
        let (status, page) = f
            .request(
                Method::GET,
                &cursor_path(next),
                Some(MEMBER_TOKEN),
                Some("1.0"),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        current = page;
    }
    assert_eq!(seen, ids);
    let context = sample["contextId"].as_str().unwrap();
    let (status, filtered) = f
        .request(
            Method::GET,
            &format!("/tasks?contextId={context}&status=TASK_STATE_SUBMITTED"),
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{filtered}");
    assert_eq!(filtered["totalSize"], 1);
    assert_eq!(filtered["tasks"][0]["id"], sample["id"]);
    let (status, empty) = f
        .request(
            Method::GET,
            "/tasks?status=TASK_STATE_COMPLETED",
            Some(MEMBER_TOKEN),
            Some("1.0"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty["totalSize"], 0);
    assert_eq!(empty["tasks"], json!([]));
    for query in [
        "pageSize=0",
        "pageSize=101",
        "status=invalid-state",
        "contextId=invalid-context",
    ] {
        let (status, error) = f
            .request(
                Method::GET,
                &format!("/tasks?{query}"),
                Some(MEMBER_TOKEN),
                Some("1.0"),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {error}");
    }
    f.cleanup().await;
}

#[tokio::test]
async fn private_task_reads_and_cancellation_reject_unknown_credentials() {
    let Some(f) = fixture().await else { return };
    let task = f.send().await;
    let id = task["id"].as_str().unwrap();
    for token in [None, Some("unregistered-peer-token")] {
        for (method, path) in [
            (Method::GET, "/tasks".to_string()),
            (Method::GET, format!("/tasks/{id}")),
            (Method::POST, format!("/tasks/{id}:cancel")),
        ] {
            let (status, error) = f.request(method, &path, token, Some("1.0"), None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}: {error}");
            assert_eq!(error["error"]["code"], "unauthorized");
        }
    }
    assert_eq!(
        f.store
            .get_internal(Uuid::parse_str(id).unwrap())
            .await
            .unwrap()
            .contract
            .status,
        TaskStatus::Submitted
    );
    f.cleanup().await;
}
