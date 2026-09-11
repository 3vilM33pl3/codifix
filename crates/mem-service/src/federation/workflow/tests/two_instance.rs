// SPDX-License-Identifier: AGPL-3.0-or-later
//! Controlled E gate: three databases, distinct non-superuser app credentials,
//! separate HTTP instances and gateway tokens. Real PDS is an explicit fixture.
use super::*;
use mem_federation_runtime::PdsPublisher;
use mem_federation_store::WorkerOutcome;
use sqlx::postgres::PgPoolOptions;
use std::{
    process::Stdio,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

pub(super) struct ServiceProcess {
    child: std::process::Child,
    directory: std::path::PathBuf,
}
impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(super) struct Database {
    pub(super) pool: PgPool,
    admin: PgPool,
    url: String,
    name: String,
    role: String,
}
impl Database {
    pub(super) async fn new(admin: &PgPool) -> Self {
        let name = format!("federation_e_{}", Uuid::new_v4().simple());
        let role = format!("{name}_app");
        let password = Uuid::new_v4().simple().to_string();
        sqlx::query(&format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE"
        ))
        .execute(admin)
        .await
        .unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(admin)
            .await
            .unwrap();
        sqlx::query(&format!("REVOKE CONNECT ON DATABASE {name} FROM PUBLIC"))
            .execute(admin)
            .await
            .unwrap();
        sqlx::query(&format!("GRANT CONNECT ON DATABASE {name} TO {role}"))
            .execute(admin)
            .await
            .unwrap();
        let mut url =
            reqwest::Url::parse(&std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap()).unwrap();
        url.set_path(&name);
        let owner = PgPoolOptions::new()
            .max_connections(3)
            .connect(url.as_str())
            .await
            .unwrap();
        mem_test_support::run_migrations(&owner).await.unwrap();
        for sql in [
            format!("GRANT USAGE,CREATE ON SCHEMA public TO {role}"),
            format!("GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO {role}"),
            format!("GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO {role}"),
        ] {
            sqlx::query(&sql).execute(&owner).await.unwrap();
        }
        url.set_username(&role).unwrap();
        url.set_password(Some(&password)).unwrap();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url.as_str())
            .await
            .unwrap();
        Self {
            pool,
            admin: owner,
            url: url.to_string(),
            name,
            role,
        }
    }
    pub(super) async fn cleanup(self, admin: &PgPool) {
        self.pool.close().await;
        self.admin.close().await;
        sqlx::query(&format!("DROP DATABASE {} WITH (FORCE)", self.name))
            .execute(admin)
            .await
            .unwrap();
        sqlx::query(&format!("DROP ROLE {}", self.role))
            .execute(admin)
            .await
            .unwrap();
    }
}
pub(super) struct Network {
    pub(super) space: SpaceId,
    pub(super) authority: InstanceId,
    pub(super) gateway: GatewayStore,
    pub(super) server: GatewayServer,
    pub(super) security: GatewaySecurity,
    pub(super) url: String,
}
impl Network {
    pub(super) async fn publish(&self, pds: Option<&PdsPublisher>) {
        let fence = self
            .gateway
            .initialize(self.space, self.authority)
            .await
            .unwrap();
        for _ in 0..256 {
            if let Some(pds) = pds {
                match self.server.publish_one(pds).await.unwrap() {
                    WorkerOutcome::Published(_) => {}
                    WorkerOutcome::Idle => return,
                    other => panic!("PDS publication failed: {other:?}"),
                }
            } else {
                match self
                    .gateway
                    .claim_publication(fence, RetryPolicy::default())
                    .await
                    .unwrap()
                {
                    Claim::Ready(lease) => self
                        .gateway
                        .acknowledge_with_receipt(
                            &lease,
                            json!({"fixture":"synthetic transport, not real PDS evidence"}),
                            &self.security,
                        )
                        .await
                        .unwrap(),
                    Claim::Idle => return,
                    other => panic!("fixture publication failed: {other:?}"),
                }
            }
        }
        panic!("unbounded publication");
    }
}
pub(super) async fn app(
    db: &Database,
    n: &Network,
    instance: InstanceId,
    token: String,
) -> Fixture {
    let project = Uuid::new_v4();
    let slug = project.to_string();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'Two-instance fixture','/DO_NOT_SHARE')").bind(project).bind(&slug).execute(&db.pool).await.unwrap();
    let (root_id, root) = actor(&db.pool, project, "admin").await;
    let (writer_id, writer) = actor(&db.pool, project, "operator").await;
    let (reader_id, reader) = actor(&db.pool, project, "reader").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config=serde_json::from_value(json!({"service":{"bind_addr":listener.local_addr().unwrap().to_string(),"api_token":"unused-disposable-only","web_root":concat!(env!("CARGO_MANIFEST_DIR"),"/../../web/dist")},
        "auth":{"mode":"multi_user","public_base_url":url},"database":{"url":db.url},
        "offline":{"enabled":false},"cluster":{"enabled":false},"reinforcement":{"enabled":false},"embeddings":{"create_enabled":false},"mcp":{"enabled":false}})).unwrap();
    let state = crate::runtime::build_state(config, Arc::new(Mutex::new(None)))
        .await
        .unwrap();
    let sync = InstanceSync::new(
        db.pool.clone(),
        GatewayClient::new(
            Endpoint::new(&n.url, true).unwrap(),
            token.clone(),
            n.space,
            n.authority,
            n.security.trust(),
        )
        .unwrap(),
        instance,
    )
    .await
    .unwrap();
    let runtime = Arc::new(ConfiguredInstance {
        expertise_browser: false,
        expertise_callers: vec![],
        expertise_output_destinations: crate::federation::local_output(),
        expertise_model_destinations: vec![],
        expertise_model: None,
        project,
        space: n.space,
        instance,
        authority: n.authority,
        sync,
    });
    *state.federation.write().unwrap() = Some(runtime.clone());
    let (tasks, process) = if std::env::var("MEMORY_FEDERATION_E_CLIENTS").as_deref() == Ok("1") {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!("memory-e-service-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config_path = directory.join("config.toml");
        let runtime_path = directory.join("federation.json");
        let token_path = directory.join("gateway-token");
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let config = format!(
            "[service]\nbind_addr={:?}\napi_token=\"unused-disposable-only\"\nweb_root={:?}\n[database]\nurl={:?}\n[auth]\nmode=\"multi_user\"\npublic_base_url={:?}\n[offline]\nenabled=false\n[cluster]\nenabled=false\n[reinforcement]\nenabled=false\n[embeddings]\ncreate_enabled=false\n[mcp]\nenabled=false\n",
            addr.to_string(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/dist"),
            db.url,
            url
        );
        // This gate authenticates fixture-issued sessions and tokens, not SSO.
        // No request is made to this intentionally unavailable local issuer.
        let config = format!(
            "{config}[auth.oidc]\nissuer_url=\"http://127.0.0.1:9/disposable-issuer\"\nclient_id=\"disposable-e-fixture\"\n"
        );
        std::fs::write(&config_path, config).unwrap();
        std::fs::write(&token_path, token).unwrap();
        std::fs::write(&runtime_path,serde_json::to_vec(&json!({"role":"instance","space":n.space,"authority":n.authority,"instance":instance,"project":project,"principal":writer_id,"gateway":n.url,"token_file":token_path,"receipt_keys":n.security.trust().keys(),"disposable_test_mode":true})).unwrap()).unwrap();
        for path in [&config_path, &token_path, &runtime_path] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let child = std::process::Command::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/debug/memory"
        ))
        .args(["--config", config_path.to_str().unwrap(), "service", "run"])
        .env("MEMORY_LAYER_PROFILE", "prod")
        .env("MEMORY_LAYER_FEDERATION_CONFIG", runtime_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
        let process = ServiceProcess { child, directory };
        let client = reqwest::Client::new();
        let mut ready = false;
        for _ in 0..200 {
            if client
                .get(format!("{url}/healthz"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(ready, "disposable Memory service process did not start");
        (vec![], Some(process))
    } else {
        let router = crate::routes::build_http_app(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (vec![task], None)
    };
    Fixture {
        process,
        state,
        pool: db.pool.clone(),
        runtime,
        slug,
        root,
        root_id,
        reader,
        reader_id,
        writer,
        writer_id,
        url,
        tasks,
        gateway: n.gateway.clone(),
        security: n.security.clone(),
    }
}
pub(super) async fn pump(n: &Network, pds: Option<&PdsPublisher>, apps: &[&Fixture]) {
    for f in apps {
        for principal in [f.root_id, f.writer_id] {
            let access =
                crate::auth::federation_local_access(&f.state, principal, f.runtime.project)
                    .await
                    .unwrap();
            f.runtime.sync.publish_pending(&access).await.unwrap();
        }
    }
    n.publish(pds).await;
    for f in apps {
        f.runtime.sync.catch_up().await.unwrap();
    }
}
async fn queue(f: &Fixture, token: &str, action: Value) -> Value {
    let body = json!({"request_id":Uuid::new_v4(),"action":action});
    let response = f
        .request(token, reqwest::Method::POST, "/actions", body.clone())
        .await;
    assert_eq!(response.0, StatusCode::ACCEPTED, "{}", response.1);
    let retry = f
        .request(token, reqwest::Method::POST, "/actions", body)
        .await;
    assert_eq!(response, retry);
    response.1
}
async fn browser_session(f: &Fixture) -> Value {
    let session = Uuid::new_v4().simple().to_string();
    let csrf = Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(f.writer_id).bind(crate::auth::hash_secret(&session)).bind(crate::auth::hash_secret(&csrf)).execute(&f.pool).await.unwrap();
    json!({"url":f.url,"project":f.slug,"instance":f.runtime.instance,"space":f.runtime.space,"token":f.writer,"session":session,"csrf":csrf})
}

#[tokio::test]
async fn e_two_instance_http_workflow() {
    let Some(admin) = mem_test_support::migrated_pool().await else {
        return;
    };
    let left_db = Database::new(&admin).await;
    let right_db = Database::new(&admin).await;
    let gateway_db = Database::new(&admin).await;
    // Database and local credentials are not shared. Neither app can connect to
    // the other's database or authenticate to its HTTP application.
    let mut wrong = reqwest::Url::parse(&left_db.url).unwrap();
    wrong.set_path(&right_db.name);
    assert!(PgPoolOptions::new().connect(wrong.as_str()).await.is_err());
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = InstanceId::new(Uuid::new_v4()).unwrap();
    let remote = InstanceId::new(Uuid::new_v4()).unwrap();
    let security = GatewaySecurity::initial(&[5; 32], Default::default()).unwrap();
    let gateway = GatewayStore::new(gateway_db.pool.clone());
    let left_token = Uuid::new_v4().simple().to_string();
    let right_token = Uuid::new_v4().simple().to_string();
    let server = GatewayServer::new(
        gateway.clone(),
        space,
        authority,
        vec![
            (left_token.clone(), authority),
            (right_token.clone(), remote),
        ],
        security.clone(),
    )
    .await
    .unwrap();
    let (url, mut gateway_task) = serve(server.router()).await;
    let n = Arc::new(Network {
        space,
        authority,
        gateway,
        server,
        security,
        url,
    });
    let pds = std::env::var("MEMORY_FEDERATION_PDS_FIXTURE")
        .ok()
        .map(|path| {
            let f: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            PdsPublisher::new(
                Endpoint::new(f["base"].as_str().unwrap(), true).unwrap(),
                f["repository"].as_str().unwrap().into(),
                f["token"].as_str().unwrap().into(),
                f["signing_key"].as_str().unwrap().into(),
                space,
            )
            .unwrap()
        });
    let pds = Arc::new(pds);
    let left = app(&left_db, &n, authority, left_token).await;
    let right = app(&right_db, &n, remote, right_token).await;
    for f in [&left, &right] {
        f.grant(f.writer_id, true).await;
        f.grant(f.reader_id, true).await;
        let (code,value)=f.request(&f.root,reqwest::Method::POST,"/grants",json!({"principal":f.root_id,"capabilities":["pull","publish","manage_membership"],"enabled":true})).await;
        assert_eq!(code, StatusCode::OK, "{value}");
    }
    assert_eq!(
        right
            .request(&left.writer, reqwest::Method::GET, "", Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    for capability in ["pull", "publish"] {
        queue(
            &left,
            &left.root,
            json!({"kind":"membership","member":remote,"capability":capability,"enabled":true}),
        )
        .await;
        // Right cannot bootstrap network access before authority accepts Pull.
        pump(&n, pds.as_ref().as_ref(), &[&left]).await;
    }
    right.runtime.sync.catch_up().await.unwrap();
    let canonical = left.source().await;
    let stream = StreamId {
        origin: authority,
        canonical,
    };
    if std::env::var("MEMORY_FEDERATION_E_CLIENTS").as_deref() == Ok("1") {
        assert!(pds.is_some(), "client E gate must use the real pinned PDS");
        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::new();
        for f in [&left, &right] {
            let state = f.state.clone();
            let runtime = f.runtime.clone();
            let principal = f.writer_id;
            let stop = stop.clone();
            workers.push(tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    let access =
                        crate::auth::federation_local_access(&state, principal, runtime.project)
                            .await
                            .unwrap();
                    let _ = runtime.sync.cycle(&access).await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }));
        }
        let network = n.clone();
        let publisher = pds.clone();
        let publish_stop = stop.clone();
        workers.push(tokio::spawn(async move {
            while !publish_stop.load(Ordering::Relaxed) {
                network.publish(publisher.as_ref().as_ref()).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }));
        let data = json!({"left":browser_session(&left).await,"right":browser_session(&right).await,"canonical":canonical});
        let mut child = tokio::process::Command::new("node")
            .arg("web/tests/federation-live.mjs")
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&data).unwrap().as_bytes())
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(180), child.wait())
            .await
            .unwrap()
            .unwrap();
        stop.store(true, Ordering::Relaxed);
        for worker in workers {
            worker.await.unwrap();
        }
        assert!(
            result.success(),
            "browser/CLI/TUI two-instance journey failed"
        );
    } else {
        let review = left.review_request(canonical).await;
        let (code, response) = left
            .request(&left.writer, reqwest::Method::POST, "/reviews", review)
            .await;
        assert_eq!(code, StatusCode::ACCEPTED, "{response}");
        pump(&n, pds.as_ref().as_ref(), &[&left, &right]).await;
        let known = right.runtime.sync.verified_snapshot().await.unwrap();
        let publication = &known.history(stream).last().unwrap().publication;
        let mut content = publication.content.clone();
        content.text = "Reviewed two-organization correction".into();
        let response=queue(&right,&right.writer,json!({"kind":"propose","proposal":{"target":stream,"base_version":1,"content":content,"evidence":publication.evidence}})).await;
        pump(&n, pds.as_ref().as_ref(), &[&left, &right]).await;
        let bad = json!({"request_id":Uuid::new_v4(),"action":{"kind":"resolve","proposal":response["record"],"approve":true}});
        assert_eq!(
            right
                .request(&right.writer, reqwest::Method::POST, "/actions", bad)
                .await
                .0,
            StatusCode::CONFLICT
        );
        queue(
            &left,
            &left.writer,
            json!({"kind":"resolve","proposal":response["record"],"approve":true}),
        )
        .await;
        pump(&n, pds.as_ref().as_ref(), &[&left, &right]).await;
    }
    let before = right.runtime.sync.verified_snapshot().await.unwrap();
    assert_eq!(before.content_version(stream), 2);
    assert_eq!(before.history(stream).len(), 2);
    let text: String = sqlx::query_scalar("SELECT canonical_text FROM memory_entries WHERE id=$1")
        .bind(canonical.as_uuid())
        .fetch_one(&left.pool)
        .await
        .unwrap();
    assert_eq!(text, "Reviewed incident diagnosis");
    let enabled: bool = sqlx::query_scalar(
        "SELECT enabled FROM federation_source_subscriptions WHERE space=$1 AND canonical_id=$2",
    )
    .bind(space.as_uuid())
    .bind(canonical.as_uuid())
    .fetch_one(&left.pool)
    .await
    .unwrap();
    assert!(!enabled);
    // Full catch-up twice is duplicate-safe. No imported rows enter ordinary DB.
    right.runtime.sync.resynchronize().await.unwrap();
    right.runtime.sync.resynchronize().await.unwrap();
    assert_eq!(
        before,
        right.runtime.sync.verified_snapshot().await.unwrap()
    );
    let local_count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_entries")
        .fetch_one(&right.pool)
        .await
        .unwrap();
    assert_eq!(local_count, 0);
    // Disconnect the configured gateway, continue local work, then reconnect
    // at the same trusted address. No fallback peer or stale acceptance claim.
    gateway_task.abort();
    let _ = gateway_task.await;
    assert!(right.runtime.sync.catch_up().await.is_err());
    assert_eq!(
        right
            .request(
                &right.writer,
                reqwest::Method::POST,
                "/context",
                json!({"streams":[stream]})
            )
            .await
            .0,
        StatusCode::OK
    );
    let offline_canonical = left.source().await;
    let review = left.review_request(offline_canonical).await;
    assert_eq!(
        left.request(&left.writer, reqwest::Method::POST, "/reviews", review)
            .await
            .0,
        StatusCode::ACCEPTED
    );
    let (_, offline_status) = left
        .request(&left.writer, reqwest::Method::GET, "", Value::Null)
        .await;
    assert!(
        offline_status["local"]["queued_intents"].as_i64().unwrap() > 0
            || offline_status["local"]["pending_source_changes"]
                .as_i64()
                .unwrap()
                > 0
    );
    let address = n.url.strip_prefix("http://").unwrap();
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let router = n.server.router();
    gateway_task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let access =
        crate::auth::federation_local_access(&left.state, left.writer_id, left.runtime.project)
            .await
            .unwrap();
    mem_federation_store::retry_source_work(&left.pool, &access, space, authority)
        .await
        .unwrap();
    pump(&n, pds.as_ref().as_ref(), &[&left, &right]).await;
    assert_eq!(
        right
            .runtime
            .sync
            .verified_snapshot()
            .await
            .unwrap()
            .content_version(StreamId {
                origin: authority,
                canonical: offline_canonical
            }),
        1
    );
    queue(&left, &left.root, json!({"kind":"revoke","member":remote})).await;
    pump(&n, pds.as_ref().as_ref(), &[&left]).await;
    assert!(right.runtime.sync.catch_up().await.is_err());
    assert!(right.runtime.sync.resynchronize().await.is_err());
    let accepted: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_accepted WHERE space=$1")
            .bind(space.as_uuid())
            .fetch_one(&gateway_db.pool)
            .await
            .unwrap();
    let mut content = before
        .history(stream)
        .last()
        .unwrap()
        .publication
        .content
        .clone();
    content.text = "Must remain queued/blocked after revocation".into();
    queue(&right,&right.writer,json!({"kind":"propose","proposal":{"target":stream,"base_version":2,"content":content,"evidence":before.history(stream).last().unwrap().publication.evidence}})).await;
    let access =
        crate::auth::federation_local_access(&right.state, right.writer_id, right.runtime.project)
            .await
            .unwrap();
    let _ = right.runtime.sync.publish_pending(&access).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM federation_accepted WHERE space=$1")
            .bind(space.as_uuid())
            .fetch_one(&gateway_db.pool)
            .await
            .unwrap(),
        accepted
    );
    let selected = json!({"streams":[stream]});
    assert_eq!(
        right
            .request(
                &right.reader,
                reqwest::Method::POST,
                "/context",
                selected.clone()
            )
            .await
            .0,
        StatusCode::OK
    );
    let (_, context) = right
        .request(
            &right.writer,
            reqwest::Method::POST,
            "/context",
            selected.clone(),
        )
        .await;
    assert_eq!(context["citations"][0]["shared_version"], 2);
    assert_eq!(
        context["citations"][0]["author"],
        right.writer_id.to_string()
    );
    // Revocation cannot recall bytes, but current local grant removal denies them.
    right.grant(right.reader_id, false).await;
    assert_eq!(
        right
            .request(&right.reader, reqwest::Method::POST, "/context", selected)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    println!(
        "E: isolated DB credentials, HTTP enrollment/review/proposal/approval, provenance, full replay, revocation and retained-copy policy passed; real PDS={}",
        pds.is_some()
    );
    for f in [&left, &right] {
        for task in &f.tasks {
            task.abort();
        }
    }
    gateway_task.abort();
    drop(left);
    drop(right);
    drop(n);
    drop(pds);
    left_db.cleanup(&admin).await;
    right_db.cleanup(&admin).await;
    gateway_db.cleanup(&admin).await;
}
