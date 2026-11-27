use super::*;

#[tokio::test]
async fn signed_linear_delegation_corrects_confirms_and_returns_issue_link() {
    const NAME: &str =
        "linear_delegation::signed_linear_delegation_corrects_confirms_and_returns_issue_link";
    const CHILD: &str = "CODIFIX_SIGNED_LINEAR_TEST_CHILD";
    const LOCAL: &str = "synthetic-local-origin-token-for-this-test";
    if std::env::var_os(CHILD).is_none() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .env(
                "CODIFIX_INBOX_TEST_PEER_TOKEN",
                "synthetic-human-remote-inbox-peer-only-token",
            )
            .env("CODIFIX_FIXTURE_LINEAR_PAT", "synthetic-raw-linear-key")
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
    let Some(mut home) = Fixture::new().await else {
        return;
    };
    let mut remote = Fixture::new().await.unwrap();
    let pds = social_roundtrip::install(&mut home.cfg, &mut remote.cfg).await;
    let graphql = Arc::new(Mutex::new(linear_graphql::Graphql::default()));
    let (gql_url, provider) = serve(
        Router::new()
            .route("/", post(linear_graphql::graphql))
            .with_state(graphql.clone())
            .layer(axum::middleware::from_fn(
                |req: axum::extract::Request, next: axum::middleware::Next| async move {
                    assert_eq!(req.headers()["authorization"], "synthetic-raw-linear-key");
                    next.run(req).await
                },
            )),
    )
    .await;
    let project = Uuid::new_v4();
    let team = Uuid::new_v4();
    let mut mandate = remote.mandate.clone();
    mandate.id = Uuid::new_v4();
    mandate.capabilities = BTreeSet::from(["linear_track".into()]);
    mandate.resources = BTreeSet::from([ResourceScope {
        kind: "linear_project".into(),
        id: project.to_string(),
    }]);
    let local_project: Uuid =
        sqlx::query_scalar("SELECT project_id FROM agency_mandates WHERE id=$1")
            .bind(remote.mandate.id)
            .fetch_one(&remote.db.pool)
            .await
            .unwrap();
    remote
        .store
        .install_mandate(
            local_project,
            &mandate,
            &remote.operator,
            remote.cfg.instance_id,
        )
        .await
        .unwrap();
    remote.cfg.specialists[0]
        .capabilities
        .insert("linear_track".into());
    remote.cfg.linear = Some(
        json!({"endpoint":gql_url,"token_env":"CODIFIX_FIXTURE_LINEAR_PAT","require_origin_confirmation":true,"tracking":{mandate.id.to_string():{"project_id":project,"team_id":team}}}),
    );
    let source = Peer {
        id: "human-home".into(),
        instance_id: home.cfg.instance_id,
        token_env: "UNUSED".into(),
        operator: false,
        mandates: BTreeSet::from([mandate.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    remote.cfg.peers.push(source);
    let agency = |fixture: &Fixture, credentials| {
        codifix_agency_runtime::server::router(codifix_agency_runtime::server::App {
            store: fixture.store.clone(),
            config: Arc::new(fixture.cfg.clone()),
            auth: Auth::from_credentials(credentials).unwrap(),
        })
    };
    let (peer_url, remote_server) = serve(agency(&remote, vec![])).await;
    home.cfg.peers.push(Peer {
        id: "remote".into(),
        instance_id: remote.cfg.instance_id,
        token_env: "UNUSED".into(),
        operator: false,
        mandates: BTreeSet::new(),
        endpoint: Some(peer_url),
        outbound_token_env: None,
    });
    let (origin_url, home_server) =
        serve(agency(&home, vec![(home.operator.clone(), LOCAL.into())])).await;
    let rs = remote.store.clone();
    let rc = remote.cfg.clone();
    let remote_worker = tokio::spawn(async move {
        loop {
            worker::run_once(&rs, &rc).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }
    });
    let mut inner = remote.request(home.cfg.instance_id);
    inner.mandate_id = mandate.id;
    inner.resource = mandate.resources.first().unwrap().clone();
    inner.capability = "linear_track".into();
    inner.acceptance_criteria = vec!["linear_issue_readback_verified".into()];
    inner.budget = Budget {
        micro_usd: 0,
        calls: 32,
        turns: 8,
        delegation_depth: 0,
    };
    inner.payload = json!({"kind":"linear_track","title":"Initial issue title","description":"Initial issue description"});
    let mut outer = home.request(home.cfg.instance_id);
    outer.capability = "delegate".into();
    outer.resource = ResourceScope {
        kind: "peer".into(),
        id: "remote".into(),
    };
    outer.acceptance_criteria = vec!["delegated_result_reported".into()];
    outer.budget = Budget {
        micro_usd: 0,
        calls: 40,
        turns: 16,
        delegation_depth: 1,
    };
    outer.payload = json!({"kind":"delegate","peer":"remote","request":inner});
    let client = reqwest::Client::new();
    let send = async |msg: Value| {
        let resp = client
            .post(format!("{origin_url}/message:send"))
            .header("A2A-Version", "1.0")
            .bearer_auth(LOCAL)
            .json(&json!({"message":msg,"configuration":{"returnImmediately":true}}))
            .send()
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.json::<Value>().await.unwrap();
        assert!(status.is_success(), "{status} {body}");
        body
    };
    let ack =
        send(json!({"messageId":outer.request_id,"role":"ROLE_USER","parts":[{"data":outer}]}))
            .await;
    let id = ack["task"]["id"].as_str().unwrap();
    let context = &ack["task"]["contextId"];
    let run = async |id: &str| {
        tokio::time::timeout(
            std::time::Duration::from_secs(12),
            worker::run_once(&home.store, &home.cfg),
        )
        .await
        .unwrap()
        .unwrap();
        client
            .get(format!("{origin_url}/tasks/{id}"))
            .header("A2A-Version", "1.0")
            .bearer_auth(LOCAL)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    };
    let answer = async |msg: &str, data: Value| {
        send(json!({"messageId":msg,"taskId":id,"contextId":context,"role":"ROLE_USER","parts":[{"data":data}]})).await;
    };
    let question = |task: &Value| -> String {
        assert_eq!(
            task["status"]["state"], "TASK_STATE_INPUT_REQUIRED",
            "{task}"
        );
        task.pointer("/status/message/parts/0/text")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    let initial = question(&run(id).await);
    assert!(
        initial.contains("Initial issue title\n\nInitial issue description"),
        "{initial}"
    );
    let old_confirm = initial.split('`').nth(1).unwrap();
    answer("correct",json!({"answer":"Correct the wording","replacement_payload":{"kind":"linear_track","title":"Reviewed issue title","description":"Reviewed acceptance criteria"}})).await;
    let corrected = question(&run(id).await);
    assert!(
        corrected.contains("Reviewed issue title\n\nReviewed acceptance criteria"),
        "{corrected}"
    );
    assert_eq!(
        initial.lines().next(),
        corrected.lines().next(),
        "same proposed issue identity after correction"
    );
    let confirm = corrected.split('`').nth(1).unwrap();
    assert_ne!(confirm, old_confirm);
    answer("stale", json!({"answer":old_confirm})).await;
    assert_eq!(question(&run(id).await), corrected);
    assert!(graphql.lock().unwrap().writes.is_empty());
    answer("correct-confirmation", json!({"answer":confirm})).await;
    let completed = run(id).await;
    assert_eq!(
        completed["status"]["state"], "TASK_STATE_COMPLETED",
        "{completed}"
    );
    let evidence = evidence_for(&completed);
    assert_eq!(evidence["independent_verification"], false);
    let readback = evidence_for(&evidence["reported_result"]);
    assert_eq!(readback["readback_verified"], true);
    assert_eq!(readback["title"], "Reviewed issue title");
    let issue = readback["issue_id"].as_str().unwrap();
    assert_eq!(
        readback["url"],
        format!("https://linear.app/fixture/issue/{issue}")
    );
    {
        let state = graphql.lock().unwrap();
        assert_eq!(state.writes.len(), 1);
        assert_eq!(
            state.objects[issue]["description"],
            "Reviewed acceptance criteria"
        );
    }
    // A fresh task with the reviewed wording finds that same issue and returns its link.
    outer.request_id = Uuid::new_v4().to_string();
    outer.payload["request"]["payload"] = json!({"kind":"linear_track","title":"Reviewed issue title","description":"Reviewed acceptance criteria"});
    let next =
        send(json!({"messageId":outer.request_id,"role":"ROLE_USER","parts":[{"data":outer}]}))
            .await;
    let repeated = run(next["task"]["id"].as_str().unwrap()).await;
    assert_eq!(
        repeated["status"]["state"], "TASK_STATE_COMPLETED",
        "{repeated}"
    );
    assert_eq!(
        evidence_for(&evidence_for(&repeated)["reported_result"])["url"],
        readback["url"]
    );
    assert_eq!(graphql.lock().unwrap().writes.len(), 1);
    remote_worker.abort();
    home_server.abort();
    remote_server.abort();
    provider.abort();
    pds.abort();
    remote.cleanup().await;
    home.cleanup().await;
}
async fn serve(app: Router) -> (String, JoinHandle<()>) {
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", tcp.local_addr().unwrap());
    (
        url,
        tokio::spawn(async move {
            axum::serve(tcp, app).await.unwrap();
        }),
    )
}

fn evidence_for(task: &Value) -> &Value {
    let id = format!("{}:evidence", task["id"].as_str().unwrap());
    let artifact = task["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["artifactId"] == id)
        .unwrap();
    &artifact["parts"][0]["data"]
}
