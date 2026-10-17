// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit browser gate: real local HTTP/SQL, synthetic signed gateway history.
//! Not the full independent two-origin/PDS/automatic-answerer rehearsal.
use super::*;
use codifix_federation::expertise::*;
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn f1_browser_collection_grants_and_queued_question() {
    if std::env::var("MEMORY_FEDERATION_F1_BROWSER").as_deref() != Ok("1") {
        return;
    }
    let f = Fixture::with_expertise(true)
        .await
        .expect("browser gate requires disposable PostgreSQL");
    let canonical = f.source().await;
    let fence = f
        .gateway
        .initialize(f.runtime.space, f.runtime.authority)
        .await
        .unwrap();
    let ad = CapabilityAdvertisement {
        owner: f.runtime.instance,
        collection: RecordId::new(Uuid::new_v4()).unwrap(),
        revision: StateRevision::new(1).unwrap(),
        collection_digest: CanonicalJsonCodec::digest(&"synthetic queued-only capability").unwrap(),
        name: "Fixture recovery".into(),
        description: "Synthetic metadata; queued question test only".into(),
        topics: vec!["recovery".into()],
        modes: vec![AnswerMode::Deterministic],
        requesters: vec![f.runtime.instance],
        model_destinations: vec![],
        output_destinations: vec!["local_human".into()],
        active: true,
        limits: Default::default(),
    };
    for record in [
        PublicationRecord::Membership {
            member: f.runtime.instance,
            capability: SpaceCapability::AskExpertise,
        },
        PublicationRecord::Membership {
            member: f.runtime.instance,
            capability: SpaceCapability::AnswerExpertise,
        },
        PublicationRecord::Expertise(ad),
    ] {
        let publisher = if matches!(&record, PublicationRecord::Expertise(_)) {
            f.runtime.instance
        } else {
            f.runtime.authority
        };
        let mut p = intent(f.runtime.space, publisher, record);
        p.envelope.schema_version = 3;
        assert_eq!(
            f.gateway
                .accept(fence, publisher, &p)
                .await
                .unwrap()
                .outcome,
            PublicationOutcome::Applied
        );
    }
    f.publish_fixture_receipts().await;
    f.runtime.sync.catch_up().await.unwrap();
    assert!(f.request(&f.root,reqwest::Method::POST,"/grants",json!({"principal":f.root_id,"enabled":true,"capabilities":["pull","publish","ask_expertise","answer_expertise"]})).await.0.is_success());
    let service = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name) VALUES($1,'internal','synthetic browser answerer')").bind(service).execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role) VALUES($1,$2,$3,'operator')").bind(Uuid::new_v4()).bind(service).bind(f.runtime.project).execute(&f.pool).await.unwrap();
    let session = Uuid::new_v4().simple().to_string();
    let csrf = Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO auth_web_sessions(id,principal_id,session_hash,csrf_hash,expires_at) VALUES($1,$2,$3,$4,now()+interval '1 hour')")
        .bind(Uuid::new_v4()).bind(f.root_id).bind(crate::auth::hash_secret(&session)).bind(crate::auth::hash_secret(&csrf)).execute(&f.pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut state = f.state.clone();
    state.config.auth.public_base_url = Some(url.clone());
    state.web_root = Some(std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/dist"
    )));
    assert!(
        state.web_root.as_ref().unwrap().join("index.html").exists(),
        "build Web first"
    );
    let app = crate::routes::build_http_app(state);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut child = tokio::process::Command::new("node")
        .arg("web/tests/f1-browser-live.mjs")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .env_remove("CODIFIX_CLIENT_TOKEN")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = json!({"synthetic":true,"url":url,"project":f.slug,"space":f.runtime.space,"instance":f.runtime.instance,"canonical":canonical,"service":service,"session":session,"csrf":csrf});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .await
        .unwrap();
    let output =
        tokio::time::timeout(std::time::Duration::from_secs(90), child.wait_with_output()).await;
    server.abort();
    let output = output.expect("bounded browser gate timed out").unwrap();
    assert!(
        output.status.success(),
        "browser gate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
