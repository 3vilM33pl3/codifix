use super::*;
use atrium_crypto::keypair::{Did, Secp256k1Keypair};
use axum::extract::State;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codifix_agency_runtime::{
    config::Auth,
    server::{App, router},
    social::{SocialConfig, SocialPeer},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
const TOKEN: &str = "incident-synthetic-authenticated-peer-token";
const OPERATOR: &str = "incident-synthetic-local-operator-token";
const REPORTER_DID: &str = "did:web:incident-reporter.test";
const HOME_DID: &str = "did:web:incident-home.test";

fn reporter_token(signer: &Secp256k1Keypair) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256K","typ":"JWT"}"#);
    let claims = json!({
        "iss": REPORTER_DID,
        "aud": format!("{HOME_DID}#codifix_agency"),
        "lxm": "dev.codifix.agency.sendMessage",
        "jti": Uuid::new_v4(),
        "exp": Utc::now().timestamp() + 60,
    });
    let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
    let signed = format!("{header}.{body}");
    let signature = URL_SAFE_NO_PAD.encode(signer.sign(signed.as_bytes()).unwrap());
    format!("{signed}.{signature}")
}

fn base(instance: Uuid, specialist: &str, capability: &str) -> Config {
    serde_json::from_value(json!({"instance_id":instance,"listen":"127.0.0.1:0","public_url":"https://fixture.example.invalid","database_env":"UNUSED","specialists":[{"schema_version":1,"instance_id":instance,"name":specialist,"specialist_id":specialist,"capabilities":[capability]}],"peers":[]})).unwrap()
}
fn peer(id: &str, instance: Uuid, operator: bool, mandate: Uuid) -> Peer {
    Peer {
        id: id.into(),
        instance_id: instance,
        token_env: "UNUSED".into(),
        operator,
        mandates: BTreeSet::from([mandate]),
        endpoint: None,
        outbound_token_env: None,
    }
}
async fn grant(
    store: &Store,
    instance: Uuid,
    owner: &Peer,
    specialist: &str,
    capability: &str,
    resources: BTreeSet<ResourceScope>,
    id: Uuid,
) {
    let project = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'incident fixture','/fixture')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    store
        .install_mandate(
            project,
            &Mandate {
                id,
                parent_id: None,
                issuer_instance_id: instance,
                subject_instance_id: instance,
                specialist_id: specialist.into(),
                capabilities: BTreeSet::from([capability.into()]),
                resources,
                destinations: BTreeSet::new(),
                expires_at: Utc::now() + Duration::days(7),
                status: MandateStatus::Active,
                budget: Budget {
                    calls: 2000,
                    turns: 400,
                    micro_usd: 100,
                    delegation_depth: 2,
                },
            },
            owner,
            instance,
        )
        .await
        .unwrap();
}
async fn serve(
    store: Store,
    cfg: Config,
    credentials: Vec<(Peer, String)>,
) -> (String, tokio::task::JoinHandle<()>) {
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", tcp.local_addr().unwrap());
    let app = router(App {
        store,
        config: Arc::new(cfg),
        auth: Auth::from_credentials(credentials).unwrap(),
    });
    (
        url,
        tokio::spawn(async move { axum::serve(tcp, app).await.unwrap() }),
    )
}
// Independent health and catalogue endpoints model a service with a live process
// and a broken user-facing dependency. Each scenario begins with an actual HTTP outage.
async fn fixture_service(
    State(restored): State<Arc<Mutex<BTreeSet<String>>>>,
    axum::extract::Path((service, operation)): axum::extract::Path<(String, String)>,
    method: axum::http::Method,
) -> (axum::http::StatusCode, &'static str) {
    use axum::http::{Method, StatusCode};
    let mut restored = restored.lock().unwrap();
    match (method, operation.as_str()) {
        (Method::POST, "restart") => {
            assert!(restored.insert(service), "duplicate repair attempt");
            (StatusCode::ACCEPTED, "started")
        }
        (Method::GET, "health") if restored.contains(&service) => (StatusCode::OK, "healthy"),
        (Method::GET, "catalogue-item")
            if restored.contains(&service)
                && service != "failed"
                && (service != "manual" || restored.contains("manual-item")) =>
        {
            (StatusCode::OK, "fixture-item-42")
        }
        _ => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}
fn mode(root: &Path, url: &str, id: &str, major: bool) -> Value {
    let command = |op: &str| json!(["/usr/bin/python3", root.join("fixture.py"), root, id, op]);
    let http = |op: &str, method: &str| {
        json!([
            "/usr/bin/curl",
            "--silent",
            "--fail",
            "--max-time",
            "2",
            "--request",
            method,
            format!("{url}/{id}/{op}")
        ])
    };
    json!({"inspect":command("inspect"),"health":http("health","GET"),"user_request":http("catalogue-item","GET"),"runbook":root.join("runbook.md"),"postmortem_directory":root.join("reports"),
       "repairs":{"single":{"condition":command("condition"),"command":http("restart","POST"),"autonomous":!major,"action":"Restart fixture service","risks":"Service interruption","recovery_options":"Operator can inspect state"}}})
}
#[tokio::test]
async fn alert_and_peer_reports_share_verified_incident_and_linear_record() {
    const NAME: &str =
        "incidents::alert_and_peer_reports_share_verified_incident_and_linear_record";
    if std::env::var_os("CODIFIX_INCIDENT_TEST_CHILD").is_none() {
        let path = std::env::temp_dir().join(format!("codifix-incident-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("CODIFIX_INCIDENT_TEST_CHILD", "1")
            .env("CODIFIX_INCIDENT_PROFILE", path.join("profile.json"))
            .env("INCIDENT_TEST_LINEAR_TOKEN", TOKEN)
            .output()
            .unwrap();
        fs::remove_dir_all(path).unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        return;
    }
    let path = PathBuf::from(std::env::var_os("CODIFIX_INCIDENT_PROFILE").unwrap());
    let root = path.parent().unwrap();
    let Some(db) = codifix_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let remote = codifix_test_support::IsolatedDatabase::new().await.unwrap();
    let home = Store {
        pool: db.pool.clone(),
    };
    let linear = Store {
        pool: remote.pool.clone(),
    };
    let h = Uuid::new_v4();
    let l = Uuid::new_v4();
    let hm = Uuid::new_v4();
    let lm = Uuid::new_v4();
    let project = Uuid::new_v4();
    let team = Uuid::new_v4();
    let owner = peer("operator", h, true, hm);
    let alert = peer("alerts", Uuid::new_v4(), false, hm);
    let agent = peer("peer-agent", Uuid::new_v4(), false, hm);
    let origin = peer("homelab", h, false, lm);
    let mut hc = base(h, "homelab", "incident_report");
    let mut lc = base(l, "linear", "linear_track");
    hc.inbox_enabled = true;
    let signer = Secp256k1Keypair::import(&[1; 32]).unwrap();
    hc.social = Some(SocialConfig {
        did: HOME_DID.into(),
        pds: "https://fixture.example.invalid".into(),
        access_token_env: "UNUSED".into(),
        access_token_file: None,
        peers: [(
            agent.id.clone(),
            SocialPeer {
                did: REPORTER_DID.into(),
                signing_key: signer.did(),
                notice_inbox: None,
                incident_conversation: None,
            },
        )]
        .into(),
    });
    let principal = Uuid::new_v4();
    let chat = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'human_oidc','Outage operator')").bind(principal).execute(&db.pool).await.unwrap();
    sqlx::query("INSERT INTO inbox_conversations(id,home_instance_id,principal_id,request_digest,title) VALUES($1,$2,$3,'fixture','Service updates')").bind(chat).bind(h).bind(principal).execute(&db.pool).await.unwrap();
    grant(
        &home,
        h,
        &owner,
        "homelab",
        "incident_report",
        ["auto", "major", "failed", "manual"]
            .into_iter()
            .map(|s| ResourceScope {
                kind: "service".into(),
                id: s.into(),
            })
            .collect(),
        hm,
    )
    .await;
    grant(
        &linear,
        l,
        &peer("operator", l, true, lm),
        "linear",
        "linear_track",
        BTreeSet::from([ResourceScope {
            kind: "linear_project".into(),
            id: project.to_string(),
        }]),
        lm,
    )
    .await;
    let graphql_state = Arc::new(Mutex::new(Graphql::default()));
    let app = Router::new()
        .route("/graphql", post(graphql))
        .with_state(graphql_state.clone());
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/graphql", tcp.local_addr().unwrap());
    let provider = tokio::spawn(async move { axum::serve(tcp, app).await.unwrap() });
    lc.linear = Some(
        json!({"endpoint":endpoint,"token_env":"INCIDENT_TEST_LINEAR_TOKEN","tracking":{lm.to_string():{"project_id":project,"team_id":team}}}),
    );
    lc.peers = vec![origin.clone()];
    let (url, linear_server) =
        serve(linear.clone(), lc.clone(), vec![(origin, TOKEN.into())]).await;
    let mut linear_peer = peer("linear", l, false, lm);
    linear_peer.endpoint = Some(url);
    linear_peer.outbound_token_env = Some("INCIDENT_TEST_LINEAR_TOKEN".into());
    hc.peers = vec![owner.clone(), alert.clone(), agent.clone(), linear_peer];
    let (url, home_server) = serve(
        home.clone(),
        hc.clone(),
        vec![
            (alert.clone(), TOKEN.into()),
            (owner.clone(), OPERATOR.into()),
        ],
    )
    .await;
    fs::write(root.join("runbook.md"), "# Fixture service\n").unwrap();
    fs::write(root.join("fixture.py"),r#"import json, pathlib, sys
root,service,op=pathlib.Path(sys.argv[1]),sys.argv[2],sys.argv[3]
if op=='inspect':
 print(json.dumps(dict(impact='Synthetic user cannot open service',logs='Application exited',metrics='Up is zero',recent_changes='No recent deployment',cause='Single process stopped',repair='single',runbook_note='Check the actual user operation',followups=['Add a representative alert'] if service=='major' else [])))
elif op=='notify':
 with (root/'notices').open('a') as f: f.write(json.dumps(json.load(sys.stdin))+'\n')
"#).unwrap();
    let fixture_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let fixture_url = format!("http://{}", fixture_listener.local_addr().unwrap());
    let restored = Arc::new(Mutex::new(BTreeSet::new()));
    let app = Router::new()
        .route(
            "/{service}/{op}",
            axum::routing::get(fixture_service).post(fixture_service),
        )
        .with_state(restored.clone());
    let fixture = tokio::spawn(async move { axum::serve(fixture_listener, app).await.unwrap() });
    let mut manual_service = mode(root, &fixture_url, "manual", false);
    manual_service["manual_recovery"] = json!(true);
    manual_service.as_object_mut().unwrap().remove("repairs");
    fs::write(&path,serde_json::to_vec(&json!({"alert_peer":"alerts","specialist":"homelab","mandate_id":hm,"inbox":{"operator_conversation":chat},"notify":["/usr/bin/python3",root.join("fixture.py"),root,"any","notify"],
        "linear":{"peer":"linear","specialist":"linear","mandate_id":lm,"project_id":project},
        "services":{"auto":mode(root,&fixture_url,"auto",false),"major":mode(root,&fixture_url,"major",true),"failed":mode(root,&fixture_url,"failed",false),"manual":manual_service}})).unwrap()).unwrap();
    let starts = Utc::now();
    let client = reqwest::Client::new();
    let alerts = json!({"alerts":[{"status":"firing","labels":{"service":"auto","alertname":"Down"},"annotations":{"summary":"Service unreachable"},"startsAt":starts,"fingerprint":"abc123"}]});
    let route = format!("{url}/v1/incidents/alertmanager");
    assert_eq!(
        client
            .post(&route)
            .json(&alerts)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let send = || client.post(&route).bearer_auth(TOKEN).json(&alerts);
    let receipt = send()
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        receipt,
        send().send().await.unwrap().json::<Value>().await.unwrap()
    );
    let first: Uuid = serde_json::from_value(receipt["tasks"][0].clone()).unwrap();
    let request = |name: &str| Submit {
        request_id: Uuid::new_v4().to_string(),
        mandate_id: hm,
        specialist_id: "homelab".into(),
        objective: "Investigate unreachable service".into(),
        acceptance_criteria: vec!["incident_report_processed".into()],
        capability: "incident_report".into(),
        resource: ResourceScope {
            kind: "service".into(),
            id: name.into(),
        },
        deadline: starts + Duration::hours(1),
        budget: Budget {
            calls: 10,
            turns: 8,
            micro_usd: 0,
            delegation_depth: 0,
        },
        payload: json!({"kind":"incident_report","starts_at":starts,"summary":"Peer encountered outage"}),
        parent_id: None,
    };
    let report = async |name: &str| {
        let request = request(name);
        let envelope = json!({"message":{"messageId":request.request_id,"role":"ROLE_USER","parts":[{"data":request}]},"configuration":{"returnImmediately":true}});
        let response = client
            .post(format!("{url}/xrpc/dev.codifix.agency.sendMessage"))
            .bearer_auth(reporter_token(&signer))
            .json(&envelope)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap();
        serde_json::from_value::<Uuid>(response["task"]["id"].clone()).unwrap()
    };
    let probe = async |service: &str, op: &str| {
        client
            .get(format!("{fixture_url}/{service}/{op}"))
            .send()
            .await
            .unwrap()
    };
    assert_eq!(probe("auto", "catalogue-item").await.status(), 503);
    let second = report("auto").await;
    let major = report("major").await;
    let failed = report("failed").await;
    for _ in 0..4 {
        assert!(worker::run_once(&home, &hc).await.unwrap());
    }
    assert_eq!(
        home.get_internal(first).await.unwrap().contract.status,
        TaskStatus::Completed
    );
    let pair: Vec<Uuid> =
        sqlx::query_scalar("SELECT incident_id FROM agency_incident_reports WHERE task_id=ANY($1)")
            .bind(vec![first, second])
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(pair.len(), 2);
    assert_eq!(pair[0], pair[1]);
    let running = home.get_internal(major).await.unwrap();
    let q = running.contract.clarification.unwrap().question;
    assert!(q.contains("Recovery options"));
    let approve = q.split('`').nth(1).unwrap();
    home.respond(major, "peer-approve", approve, None, &agent)
        .await
        .unwrap();
    worker::run_once(&home, &hc).await.unwrap();
    assert_eq!(probe("major", "health").await.status(), 503);
    home.respond(major, "operator-approved", approve, None, &owner)
        .await
        .unwrap();
    worker::run_once(&home, &hc).await.unwrap();
    assert_eq!(
        probe("auto", "catalogue-item").await.text().await.unwrap(),
        "fixture-item-42"
    );
    assert_eq!(
        probe("major", "catalogue-item").await.text().await.unwrap(),
        "fixture-item-42"
    );
    assert_eq!(probe("failed", "health").await.status(), 200);
    assert_eq!(probe("failed", "catalogue-item").await.status(), 503);
    assert_eq!(
        home.get_internal(failed).await.unwrap().contract.status,
        TaskStatus::InputRequired
    );
    assert!(root.join("reports").join(format!("{}.md", major)).is_file());
    assert!(
        fs::read_to_string(root.join("runbook.md"))
            .unwrap()
            .contains("Check the actual user operation")
    );
    // Advance the test clock for durable status delivery. The actual Linear worker
    // sends its confirmation challenge and uses the disposable GraphQL provider.
    let drain_outbox = async || {
        for _ in 0..160 {
            let n = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM agency_incident_events WHERE NOT (tracked AND notified)",
            )
            .fetch_one(&db.pool)
            .await
            .unwrap();
            if n == 0 {
                break;
            }
            sqlx::query("UPDATE agency_incident_events SET retry_at=statement_timestamp() WHERE NOT (tracked AND notified)").execute(&db.pool).await.unwrap();
            worker::run_once(&home, &hc).await.unwrap();
            worker::run_once(&linear, &lc).await.unwrap();
        }
        let pending: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT event_id,problem FROM agency_incident_events WHERE NOT (tracked AND notified)",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert!(pending.is_empty(), "{pending:?}");
    };
    drain_outbox().await;
    let status = client
        .get(format!("{url}/v1/incidents/{}", pair[0]))
        .bearer_auth(OPERATOR)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        status["verified"],
        json!({"health":true,"user_request":true})
    );
    assert!(status["ticket"]["url"].is_string());
    let stored = graphql_state.lock().unwrap().objects.clone();
    assert_eq!(stored.len(), 4, "three incidents plus one linked follow-up");
    let followup = stored
        .values()
        .find(|issue| {
            issue["description"]
                .as_str()
                .unwrap_or_default()
                .contains(&format!("Follow-up to incident {major}"))
        })
        .unwrap();
    let postmortem = fs::read_to_string(root.join("reports").join(format!("{major}.md"))).unwrap();
    let followup_url = followup["url"].as_str().unwrap();
    assert!(postmortem.contains("## Timeline"));
    assert_eq!(
        postmortem.matches(&format!("](<{followup_url}>)")).count(),
        1
    );
    let inbox = codifix_inbox::store::Store {
        pool: db.pool.clone(),
        home_instance_id: h,
    };
    let messages = inbox.detail(principal, chat).await.unwrap().messages;
    assert!(
        messages
            .iter()
            .any(|m| m.body.contains("waiting for operator") && m.body.contains("Proposed action"))
    );
    assert!(messages.iter().any(|m| m.body.contains("recovered")));
    let notices = fs::read_to_string(root.join("notices")).unwrap();
    assert!(
        notices.contains("acknowledged")
            && notices.contains("waiting_for_operator")
            && notices.contains("recovered")
    );
    let manual = report("manual").await;
    worker::run_once(&home, &hc).await.unwrap();
    assert_eq!(
        home.get_internal(manual).await.unwrap().contract.status,
        TaskStatus::Completed
    );
    let incident_status = async |id: Uuid| {
        client
            .get(format!("{url}/v1/incidents/{id}"))
            .bearer_auth(OPERATOR)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    };
    assert_eq!(probe("manual", "health").await.status(), 503);
    let cursor = worker::verify_incident_recovery(&home, &hc, None)
        .await
        .unwrap();
    assert_eq!(cursor, Some(manual));
    restored.lock().unwrap().insert("manual".into());
    assert_eq!(probe("manual", "health").await.status(), 200);
    assert_eq!(probe("manual", "catalogue-item").await.status(), 503);
    worker::verify_incident_recovery(&home, &hc, cursor)
        .await
        .unwrap();
    drain_outbox().await;
    let waiting = incident_status(manual).await;
    assert_eq!(waiting["state"], "watching_manual_recovery");
    assert!(waiting["verified"].is_null());
    let first_ticket = waiting["ticket"].clone();

    home.control_mandate(hm, "pause", &owner).await.unwrap();
    restored.lock().unwrap().insert("manual-item".into());
    worker::verify_incident_recovery(&home, &hc, cursor)
        .await
        .unwrap();
    assert_eq!(
        incident_status(manual).await["state"],
        "watching_manual_recovery"
    );
    home.control_mandate(hm, "resume", &owner).await.unwrap();
    worker::verify_incident_recovery(&home, &hc, cursor)
        .await
        .unwrap();
    assert_eq!(
        worker::verify_incident_recovery(&home, &hc, cursor)
            .await
            .unwrap(),
        None
    );
    drain_outbox().await;
    let recovered = incident_status(manual).await;
    assert_eq!(recovered["state"], "recovered");
    assert_eq!(
        recovered["verified"],
        json!({"health":true,"user_request":true})
    );
    assert_eq!(recovered["ticket"]["issue_id"], first_ticket["issue_id"]);
    assert_eq!(recovered["ticket"]["url"], first_ticket["url"]);
    let recovery_events: Vec<&Value> = recovered["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| (event["event"]["state"] == "recovered").then_some(&event["event"]))
        .collect();
    assert_eq!(recovery_events.len(), 1);
    assert_eq!(recovery_events[0]["automated_repair"], false);
    let repairs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM external_effects WHERE resource='service:manual'")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(repairs, 0);
    assert!(!root.join("reports").join(format!("{manual}.md")).exists());
    let stored = graphql_state.lock().unwrap().objects.clone();
    assert_eq!(
        stored.len(),
        5,
        "manual recovery updates one additional issue"
    );
    assert!(stored.values().any(|issue| {
        issue["id"] == first_ticket["issue_id"]
            && issue["description"]
                .as_str()
                .unwrap_or_default()
                .contains("The companion only ran read-only probes.")
    }));
    fixture.abort();
    provider.abort();
    home_server.abort();
    linear_server.abort();
    remote.cleanup().await.unwrap();
    db.cleanup().await.unwrap();
}
