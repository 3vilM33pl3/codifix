// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{Response, StatusCode},
    routing::any,
};
use mem_atproto_verify::fixtures::{FixtureKey, signed_repo};
use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, RecordLocator, SpaceCapability, publication::*,
};
use mem_federation_runtime::*;
use mem_federation_store::*;
use mem_record::{Permission, PermissionSet};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn id() -> RecordId {
    RecordId::new(Uuid::new_v4()).unwrap()
}
fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn intent(space: SpaceId, publisher: InstanceId, record: PublicationRecord) -> PublicationIntent {
    let record_id = id();
    PublicationIntent {
        locator: RecordLocator::new(format!("publication:{}", record_id.as_uuid())).unwrap(),
        envelope: PublicationEnvelope {
            schema_version: 2,
            space,
            publisher,
            record_id,
            author: AuthorIdentity::new("expert-attribution").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record,
        },
    }
}
async fn database(admin: &PgPool) -> (PgPool, String) {
    let name = format!("federation_c_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(admin)
        .await
        .unwrap();
    let mut url =
        reqwest::Url::parse(&std::env::var("MEMORY_LAYER_TEST_DATABASE_URL").unwrap()).unwrap();
    url.set_path(&name);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(url.as_str())
        .await
        .unwrap();
    mem_test_support::run_migrations(&pool).await.unwrap();
    (pool, name)
}
async fn local_project(pool: &PgPool) -> LocalProjectAccess {
    let project = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'federation test','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_role) VALUES($1,'internal','fixture reviewer','admin')").bind(principal).execute(pool).await.unwrap();
    LocalProjectAccess {
        principal,
        project,
        permissions: PermissionSet::from_permissions(&Permission::ALL),
        read_only: false,
    }
}
async fn grants(pool: &PgPool, access: &LocalProjectAccess, space: SpaceId, instance: InstanceId) {
    let mut tx = pool.begin().await.unwrap();
    set_local_grant(
        &mut tx,
        access,
        &LocalSpaceGrant {
            principal: access.principal,
            project: access.project,
            space,
            instance,
            capabilities: vec![SpaceCapability::Pull, SpaceCapability::Publish],
        },
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[derive(Clone)]
struct PdsFixture {
    records: Arc<Mutex<BTreeMap<String, Value>>>,
    lost_reply: Arc<AtomicBool>,
    upstream: Option<String>,
}
async fn pds_rpc(State(f): State<PdsFixture>, request: Request) -> Response<Body> {
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    let method = request.method().clone();
    let auth = request.headers().get("authorization").cloned();
    let body = to_bytes(request.into_body(), 128 * 1024).await.unwrap();
    if let Some(base) = &f.upstream {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let mut req = client
            .request(method, format!("{base}{path}?{query}"))
            .body(body.to_vec())
            .header("content-type", "application/json");
        if let Some(auth) = auth {
            req = req.header("authorization", auth);
        }
        let response = req.send().await.unwrap();
        let status = response.status();
        let bytes = response.bytes().await.unwrap();
        if path.ends_with("createRecord")
            && status.is_success()
            && f.lost_reply.swap(false, Ordering::SeqCst)
        {
            return Response::builder().status(503).body(Body::empty()).unwrap();
        }
        return Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .unwrap();
    }
    if path.ends_with("getRepo") {
        let records = f
            .records
            .lock()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                (
                    format!("{}/{key}", mem_atproto::publication::COLLECTION),
                    serde_json::from_value(value["value"].clone()).unwrap(),
                )
            })
            .collect();
        let bytes = signed_repo("did:web:fixture.example.com", &records, FixtureKey::K256);
        return Response::builder()
            .status(200)
            .header("content-type", "application/vnd.ipld.car")
            .body(Body::from(bytes))
            .unwrap();
    }
    let (status, body) = if path.ends_with("getRecord") {
        let url = reqwest::Url::parse(&format!("http://fixture/?{query}")).unwrap();
        let params: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        f.records
            .lock()
            .unwrap()
            .get(&params["rkey"])
            .cloned()
            .map_or(
                (StatusCode::BAD_REQUEST, json!({"error":"RecordNotFound"})),
                |body| (StatusCode::OK, body),
            )
    } else {
        let body: Value = serde_json::from_slice(&body).unwrap();
        let wire = mem_atproto::publication::PublicationWire::decode(
            serde_json::from_value(body["record"].clone()).unwrap(),
        )
        .unwrap();
        let record = json!({"uri":format!("at://{}/{}/{}",body["repo"].as_str().unwrap(),mem_atproto::publication::COLLECTION,body["rkey"].as_str().unwrap()),"cid":wire.cid().unwrap(),"value":body["record"]});
        let mut records = f.records.lock().unwrap();
        let key = body["rkey"].as_str().unwrap().to_owned();
        if let std::collections::btree_map::Entry::Vacant(entry) = records.entry(key) {
            entry.insert(record.clone());
            if f.lost_reply.swap(false, Ordering::SeqCst) {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"error":"lost_reply_after_write"}),
                )
            } else {
                (StatusCode::OK, record)
            }
        } else {
            (
                StatusCode::BAD_REQUEST,
                json!({"error":"RecordAlreadyExists"}),
            )
        }
    };
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}
#[derive(Deserialize)]
struct LivePds {
    base: String,
    repository: String,
    token: String,
    signing_key: String,
}
async fn publisher(space: SpaceId) -> (PdsPublisher, tokio::task::JoinHandle<()>, Arc<AtomicBool>) {
    let fixture = std::env::var_os("MEMORY_FEDERATION_PDS_FIXTURE")
        .map(|path| serde_json::from_slice::<LivePds>(&std::fs::read(path).unwrap()).unwrap());
    let lost_reply = Arc::new(AtomicBool::new(true));
    let state = PdsFixture {
        records: Arc::default(),
        lost_reply: lost_reply.clone(),
        upstream: fixture.as_ref().map(|f| f.base.clone()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/xrpc/com.atproto.repo.getRecord", any(pds_rpc))
        .route("/xrpc/com.atproto.repo.createRecord", any(pds_rpc))
        .route("/xrpc/com.atproto.sync.getRepo", any(pds_rpc))
        .with_state(state);
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let repo = fixture
        .as_ref()
        .map_or("did:web:fixture.example.com", |f| f.repository.as_str())
        .to_owned();
    let signing_key = fixture
        .as_ref()
        .map_or_else(|| FixtureKey::K256.did_key(), |f| f.signing_key.clone());
    let auth = fixture.map_or_else(token, |f| f.token);
    (
        PdsPublisher::new(
            Endpoint::new(&format!("http://{addr}"), true).unwrap(),
            repo,
            auth,
            signing_key,
            space,
        )
        .unwrap(),
        task,
        lost_reply,
    )
}
async fn publish_all(server: &GatewayServer, pds: &PdsPublisher) {
    for _ in 0..256 {
        match server.publish_one(pds).await.unwrap() {
            WorkerOutcome::Published(_) => {}
            WorkerOutcome::Idle => return,
            other => panic!("publication stalled: {other:?}"),
        }
    }
    panic!("unbounded publication");
}

/// With MEMORY_FEDERATION_PDS_FIXTURE this exact Rust pipeline runs against the
/// pinned upstream PDS. Without it, the same test uses an isolated fake XRPC edge.
#[tokio::test]
async fn c_source_gateway_pds_receiver_and_recovery_across_three_databases() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_test_writer()
        .try_init();
    let Some(admin) = mem_test_support::migrated_pool().await else {
        return;
    };
    let (source, source_name) = database(&admin).await;
    let (receiver, receiver_name) = database(&admin).await;
    let (gateway, gateway_name) = database(&admin).await;
    assert_ne!(source_name, receiver_name);
    assert_ne!(source_name, gateway_name);
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = instance();
    let a = instance();
    let b = instance();
    let root_token = token();
    let a_token = token();
    let b_token = token();
    let credentials = vec![
        (root_token.clone(), authority),
        (a_token.clone(), a),
        (b_token.clone(), b),
    ];
    let store = GatewayStore::new(gateway.clone());
    let server = GatewayServer::new(store.clone(), space, authority, credentials.clone())
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = server.router();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let endpoint = Endpoint::new(&format!("http://{addr}"), true).unwrap();
    let root = GatewayClient::new(endpoint.clone(), root_token.clone(), space, authority).unwrap();
    let a_client = GatewayClient::new(endpoint.clone(), a_token.clone(), space, authority).unwrap();
    let b_client = GatewayClient::new(endpoint.clone(), b_token, space, authority).unwrap();
    let invalid = GatewayClient::new(endpoint.clone(), token(), space, authority).unwrap();
    assert!(matches!(
        invalid.pull(0, 32).await,
        Err(RuntimeError::Unauthorized)
    ));
    // Authenticated token identity, never claimed publisher, owns authority.
    let forged = intent(
        space,
        authority,
        PublicationRecord::Membership {
            member: a,
            capability: SpaceCapability::Publish,
        },
    );
    assert_eq!(
        a_client.accept(&forged).await.unwrap().outcome,
        PublicationOutcome::Rejected(PublicationReject::ForgedPublisher)
    );
    let foreign = intent(
        SpaceId::new(Uuid::new_v4()).unwrap(),
        authority,
        PublicationRecord::Membership {
            member: a,
            capability: SpaceCapability::Publish,
        },
    );
    let foreign: Acceptance = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("http://{addr}/v1/accept"))
        .bearer_auth(&root_token)
        .json(&foreign)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        foreign.outcome,
        PublicationOutcome::Rejected(PublicationReject::WrongSpace)
    );
    let accepted: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_accepted")
        .fetch_one(&gateway)
        .await
        .unwrap();
    assert_eq!(
        accepted, 0,
        "rejected identities must not change acceptance state"
    );
    for member in [a, b] {
        for capability in [SpaceCapability::Pull, SpaceCapability::Publish] {
            assert_eq!(
                root.accept(&intent(
                    space,
                    authority,
                    PublicationRecord::Membership { member, capability }
                ))
                .await
                .unwrap()
                .outcome,
                PublicationOutcome::Applied
            );
        }
    }
    let (pds, pds_task, lost_reply) = publisher(space).await;
    assert!(a_client.pull(0, 32).await.unwrap().records.is_empty());
    publish_all(&server, &pds).await;
    assert!(!lost_reply.load(Ordering::SeqCst)); // real/fake create succeeded, response was discarded
    let a_access = local_project(&source).await;
    let b_access = local_project(&receiver).await;
    grants(&source, &a_access, space, a).await;
    grants(&receiver, &b_access, space, b).await;
    let a_sync = InstanceSync::new(source.clone(), a_client.clone(), a)
        .await
        .unwrap();
    let b_sync = InstanceSync::new(receiver.clone(), b_client.clone(), b)
        .await
        .unwrap();
    a_sync.catch_up().await.unwrap();
    b_sync.catch_up().await.unwrap();
    let canonical = id();
    let source_id = id();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,27,FALSE,'original diagnosis','original diagnosis','decision','project',3,0.8,now(),now(),to_tsvector('original diagnosis'))")
        .bind(source_id.as_uuid()).bind(a_access.project).bind(canonical.as_uuid()).execute(&source).await.unwrap();
    sqlx::query("INSERT INTO memory_tags(memory_entry_id,tag) VALUES($1,'selected-tag')")
        .bind(source_id.as_uuid())
        .execute(&source)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_sources(id,memory_entry_id,file_path,source_kind,excerpt,created_at) VALUES(gen_random_uuid(),$1,'/private/unselected.txt','file','NEVER_SHARE_RAW_CAPTURE',now())")
        .bind(source_id.as_uuid()).execute(&source).await.unwrap();
    let no_intent: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_source_changes")
        .fetch_one(&source)
        .await
        .unwrap();
    assert_eq!(no_intent, 0);
    let expected = preview_source(&source, a_access.project, canonical)
        .await
        .unwrap();
    let stream = StreamId {
        origin: a,
        canonical,
    };
    let evidence = EvidenceSnapshot {
        space,
        stream,
        public_source_label: "reviewed incident extract".into(),
        selected_text: "selected evidence bytes".into(),
        observed_at: chrono::Utc::now(),
    };
    let reviewed = ReviewedEvidence::new(
        evidence.clone(),
        ReviewReceipt {
            snapshot_digest: CanonicalJsonCodec::digest(&evidence).unwrap(),
            reviewer: AuthorIdentity::new(a_access.principal.to_string()).unwrap(),
            reviewed_at: chrono::Utc::now(),
        },
    )
    .unwrap();
    let known = a_sync.replica().snapshot().await.unwrap();
    let mut tx = source.begin().await.unwrap();
    review_and_share(
        &mut tx,
        &known,
        &a_access,
        a,
        canonical,
        &expected,
        std::slice::from_ref(&reviewed),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_source_changes")
        .fetch_one(&source)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let mut tx = source.begin().await.unwrap();
    review_and_share(
        &mut tx,
        &known,
        &a_access,
        a,
        canonical,
        &expected,
        std::slice::from_ref(&reviewed),
    )
    .await
    .unwrap();
    let before_commit: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_source_changes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(before_commit, 0);
    // A mutation after the review call in that same transaction must be captured
    // at commit, rather than freezing an intermediate state at review time.
    sqlx::query("UPDATE memory_canonical_state SET confidence=0.83 WHERE canonical_id=$1")
        .bind(canonical.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(a_sync.cycle(&a_access).await.unwrap(), 3); // evidence, content, state
    assert_eq!(b_sync.catch_up().await.unwrap(), 4); // accepted is NOT published
    let queued = pending_intents(&source, space, a, 32).await.unwrap();
    assert!(queued.is_empty());
    publish_all(&server, &pds).await;
    assert_eq!(b_sync.cycle(&b_access).await.unwrap(), 0);
    let first = b_sync.replica().snapshot().await.unwrap();
    assert_eq!(first.content_version(stream), 1);
    assert_eq!(
        first.current_state(stream).unwrap().confidence.get(),
        830_000
    );
    assert_eq!(
        first.history(stream)[0].publication.source.version.get(),
        27
    );
    assert_eq!(
        first.history(stream)[0].publication.content.tags,
        vec!["selected-tag"]
    );
    let evidence_key = first.history(stream)[0].publication.evidence[0];
    assert_eq!(
        first
            .evidence(evidence_key)
            .unwrap()
            .published_snapshot_bytes()
            .unwrap(),
        reviewed.published_snapshot_bytes().unwrap()
    );
    let serialized = serde_json::to_string(first.accepted()).unwrap();
    assert!(!serialized.contains("NEVER_SHARE_RAW_CAPTURE"));
    assert!(!serialized.contains("/private/"));

    // Disconnect the gateway listener. The actual curation mutation function
    // still commits, and its deferred hook freezes publication intent atomically.
    task.abort();
    let _ = task.await;
    let next_source = mem_curate::apply_validation_revision(
        &source,
        source_id.as_uuid(),
        "corrected diagnosis",
        "corrected diagnosis",
    )
    .await
    .unwrap();
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_source_changes WHERE NOT prepared")
            .fetch_one(&source)
            .await
            .unwrap();
    assert_eq!(pending, 1);
    assert!(a_sync.cycle(&a_access).await.is_err());
    let mut tx = source.begin().await.unwrap();
    sqlx::query("UPDATE memory_canonical_state SET status='archived',updated_at=now() WHERE canonical_id=$1").bind(canonical.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
    let pending_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM federation_source_changes WHERE NOT prepared")
            .fetch_one(&source)
            .await
            .unwrap();
    assert_eq!(pending_after, pending);
    let server = GatewayServer::new(store.clone(), space, authority, credentials)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let router = server.router();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    retry_source_work(&source, &a_access, space, a)
        .await
        .unwrap();
    assert_eq!(a_sync.cycle(&a_access).await.unwrap(), 2);
    publish_all(&server, &pds).await;
    let changes = b_client.pull(first.sequence(), 32).await.unwrap().records;
    let reversed = changes.iter().rev().cloned().collect::<Vec<_>>();
    assert!(
        b_sync
            .replica()
            .receive(authority, &reversed)
            .await
            .is_err()
    );
    assert_eq!(b_sync.replica().snapshot().await.unwrap(), first);
    let mut tampered = changes.clone();
    tampered[0].envelope.author = AuthorIdentity::new("tampered").unwrap();
    assert!(
        b_sync
            .replica()
            .receive(authority, &tampered)
            .await
            .is_err()
    );
    b_sync.catch_up().await.unwrap();
    let current = b_sync.replica().snapshot().await.unwrap();
    assert_eq!(current.content_version(stream), 2);
    assert_eq!(
        current.history(stream)[1]
            .publication
            .source
            .record_id
            .as_uuid(),
        next_source
    );
    assert_eq!(
        current.history(stream)[1].publication.evidence,
        vec![evidence_key]
    );
    let accepted = current.accepted().last().unwrap();
    let duplicate = PublicationIntent {
        locator: accepted.locator.clone(),
        envelope: accepted.envelope.clone(),
    };
    assert_eq!(
        a_client.accept(&duplicate).await.unwrap().outcome,
        PublicationOutcome::Duplicate
    );

    // Cross a real network page boundary using automatic mutable-state capture.
    for i in 0..33 {
        sqlx::query("UPDATE memory_canonical_state SET confidence=$2,updated_at=clock_timestamp() WHERE canonical_id=$1")
            .bind(canonical.as_uuid()).bind(0.2f32+i as f32/100.).execute(&source).await.unwrap();
    }
    a_sync.cycle(&a_access).await.unwrap();
    publish_all(&server, &pds).await;
    a_sync.cycle(&a_access).await.unwrap();
    publish_all(&server, &pds).await;
    let target = b_client.pull(0, 32).await.unwrap().published_through;
    assert!(target > 32);
    assert_eq!(b_sync.resynchronize().await.unwrap(), target);
    assert_eq!(
        b_sync
            .replica()
            .snapshot()
            .await
            .unwrap()
            .content_version(stream),
        2
    );
    // Pull remains allowed after publication permission removal. Already-frozen,
    // prepared work still requires the current local publishing permission.
    sqlx::query("UPDATE memory_canonical_state SET confidence=0.91 WHERE canonical_id=$1")
        .bind(canonical.as_uuid())
        .execute(&source)
        .await
        .unwrap();
    prepare_source_changes(
        &source,
        &a_sync.replica().snapshot().await.unwrap(),
        &a_access,
        a,
        32,
    )
    .await
    .unwrap();
    let before_denial = root.pull(0, 32).await.unwrap().published_through;
    let mut denied = a_access.clone();
    denied.permissions = PermissionSet::from_permissions(&[Permission::MemoryRead]);
    assert!(a_sync.cycle(&denied).await.is_err());
    assert_eq!(
        root.pull(0, 32).await.unwrap().published_through,
        before_denial
    );
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM federation_source_outbox WHERE acceptance IS NULL",
    )
    .fetch_one(&source)
    .await
    .unwrap();
    assert!(pending > 0);
    retry_source_work(&source, &a_access, space, a)
        .await
        .unwrap();
    assert!(a_sync.cycle(&a_access).await.unwrap() > 0);
    publish_all(&server, &pds).await;

    // Tombstone is state, not a new blank content version.
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES(gen_random_uuid(),$1,$2,29,TRUE,'','deleted','decision','project',3,0.8,now(),now(),to_tsvector(''))")
        .bind(a_access.project).bind(canonical.as_uuid()).execute(&source).await.unwrap();
    a_sync.cycle(&a_access).await.unwrap();
    publish_all(&server, &pds).await;
    b_sync.catch_up().await.unwrap();
    assert!(
        b_sync
            .replica()
            .snapshot()
            .await
            .unwrap()
            .current_state(stream)
            .unwrap()
            .tombstone
    );
    assert_eq!(
        b_sync
            .replica()
            .snapshot()
            .await
            .unwrap()
            .content_version(stream),
        2
    );
    let revoke = intent(
        space,
        authority,
        PublicationRecord::Revocation {
            member: a,
            capability: None,
        },
    );
    root.accept(&revoke).await.unwrap(); // still unpublished
    assert!(matches!(
        a_client.pull(0, 32).await,
        Err(RuntimeError::Unauthorized)
    ));
    let forged = intent(
        space,
        a,
        PublicationRecord::Membership {
            member: b,
            capability: SpaceCapability::Pull,
        },
    );
    assert_eq!(
        a_client.accept(&forged).await.unwrap().outcome,
        PublicationOutcome::Rejected(PublicationReject::Unauthorized)
    );
    assert_eq!(
        a_sync
            .replica()
            .snapshot()
            .await
            .unwrap()
            .content_version(stream),
        2
    );
    publish_all(&server, &pds).await;
    b_sync.catch_up().await.unwrap();
    let final_state = b_sync.replica().snapshot().await.unwrap();
    assert!(!final_state.has_capability(a, SpaceCapability::Publish));
    for record in final_state.accepted() {
        let wire = mem_atproto::publication::PublicationWire::encode(record.clone()).unwrap();
        let mut value = serde_json::to_value(wire.raw()).unwrap();
        value["future_extension"] = json!({"opaque":[1,2,3]});
        value["delivery"]["envelope"]["extra_attribution"] = json!("not authority");
        let restored = mem_atproto::publication::PublicationWire::decode(
            serde_json::from_value(value.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.delivery(), record);
        assert_eq!(serde_json::to_value(restored.raw()).unwrap(), value);
    }
    task.abort();
    pds_task.abort();
    let _ = task.await;
    let _ = pds_task.await;
    drop(a_sync);
    drop(b_sync);
    drop(server);
    drop(store);
    source.close().await;
    receiver.close().await;
    gateway.close().await;
    for name in [source_name, receiver_name, gateway_name] {
        // Only these UUID-named databases were created by this test. Force also
        // closes a backend whose final rollback/socket close is still in flight.
        sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&admin)
            .await
            .unwrap();
    }
}
