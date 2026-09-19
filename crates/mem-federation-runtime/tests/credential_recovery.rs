// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg(unix)]
#![allow(clippy::unwrap_used)]
//! Synthetic PDS and isolated PostgreSQL only; no deployed credentials.
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use mem_atproto::publication::PublicationWire;
use mem_atproto_verify::fixtures::{FixtureKey, signed_repo};
use mem_federation::{AuthorIdentity, RecordLocator, SpaceCapability, publication::*};
use mem_federation_runtime::*;
use mem_federation_store::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

const REPOSITORY: &str = "did:web:synthetic-pds.example.test";
fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn intent(space: SpaceId, authority: InstanceId, record: PublicationRecord) -> PublicationIntent {
    let id = Uuid::new_v4();
    PublicationIntent {
        locator: RecordLocator::new(format!("synthetic:{id}")).unwrap(),
        envelope: PublicationEnvelope {
            schema_version: 2,
            space,
            publisher: authority,
            record_id: RecordId::new(id).unwrap(),
            author: AuthorIdentity::new("synthetic-expert").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record,
        },
    }
}
fn bundle(pds: &str, generation: u64, expires: u64) -> Value {
    let now = chrono::Utc::now().timestamp() as u64;
    let token = format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#),
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"sub":REPOSITORY,"aud":REPOSITORY,
            "scope":"com.atproto.access","exp":expires,"jti":generation}))
            .unwrap()
        )
    );
    json!({"schema_version":1,"repository":REPOSITORY,"pds":pds,"generation":generation,
        "access_token":token,"issued_at":now-3600,"expires_at":expires})
}
fn replace(path: &Path, bundle: &Value) {
    let pending = path.with_extension("next");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&pending)
        .unwrap();
    file.write_all(&serde_json::to_vec(bundle).unwrap())
        .unwrap();
    fs::rename(pending, path).unwrap();
}
fn private_directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn proof(delivery: &PublicationDelivery) -> (Value, Vec<u8>) {
    let wire = PublicationWire::encode(delivery.clone()).unwrap();
    let body = json!({"uri":wire.uri(&REPOSITORY.parse().unwrap()).unwrap(),"cid":wire.cid().unwrap(),"value":wire.raw()});
    let records = BTreeMap::from([(
        format!(
            "{}/{}",
            mem_atproto::publication::COLLECTION,
            wire.record().unwrap().rkey.as_ref()
        ),
        wire.raw().clone(),
    )]);
    (body, signed_repo(REPOSITORY, &records, FixtureKey::K256))
}
fn capture(headers: &HeaderMap, seen: &Mutex<Vec<String>>) {
    seen.lock().unwrap().push(
        headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
    );
}

#[tokio::test]
async fn replacement_during_create_keeps_one_credential_for_readback_and_signed_car() {
    let directory = private_directory();
    let path = directory.path().join("session.json");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let expires = chrono::Utc::now().timestamp() as u64 + 3600;
    let first = bundle(endpoint.origin(), 1, expires);
    let next = bundle(endpoint.origin(), 2, expires + 1);
    replace(&path, &first);
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = instance();
    let intent = intent(
        space,
        authority,
        PublicationRecord::Membership {
            member: instance(),
            capability: SpaceCapability::Pull,
        },
    );
    let delivery = PublicationDelivery::new(
        AuthoritySequence::new(1).unwrap(),
        intent.locator,
        intent.envelope,
    )
    .unwrap();
    let (body, car) = proof(&delivery);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let reads = Arc::new(AtomicUsize::new(0));
    let writes = Arc::new(AtomicUsize::new(0));
    let route_seen = seen.clone();
    let route_reads = reads.clone();
    let route_path = path.clone();
    let next_bundle = next.clone();
    let get_record = get(move |headers: HeaderMap| {
        let seen = route_seen.clone();
        let reads = route_reads.clone();
        let path = route_path.clone();
        let next = next_bundle.clone();
        let body = body.clone();
        async move {
            capture(&headers, &seen);
            if reads.fetch_add(1, Ordering::SeqCst) == 0 {
                replace(&path, &next);
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error":"RecordNotFound"})),
                )
            } else {
                (StatusCode::OK, Json(body))
            }
        }
    });
    let route_seen = seen.clone();
    let route_writes = writes.clone();
    let create_record = post(move |headers: HeaderMap| {
        let seen = route_seen.clone();
        let writes = route_writes.clone();
        async move {
            capture(&headers, &seen);
            writes.fetch_add(1, Ordering::SeqCst);
            StatusCode::OK
        }
    });
    let route_seen = seen.clone();
    let get_repo = get(move |headers: HeaderMap| {
        let seen = route_seen.clone();
        let car = car.clone();
        async move {
            capture(&headers, &seen);
            car
        }
    });
    let router = Router::new()
        .route("/xrpc/com.atproto.repo.getRecord", get_record)
        .route("/xrpc/com.atproto.repo.createRecord", create_record)
        .route("/xrpc/com.atproto.sync.getRepo", get_repo);
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let publisher = PdsPublisher::with_rotating_credentials(
        endpoint,
        REPOSITORY.into(),
        path,
        None,
        FixtureKey::K256.did_key(),
        space,
    )
    .unwrap();
    publisher.publish_exact(&delivery).await.unwrap();
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        vec![format!("Bearer {}", first["access_token"].as_str().unwrap()); 4]
    );
    seen.lock().unwrap().clear();
    publisher.publish_exact(&delivery).await.unwrap();
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        vec![format!("Bearer {}", next["access_token"].as_str().unwrap()); 2]
    );
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn gateway_restarts_recover_only_new_credentials_and_status_rechecks_membership() {
    let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let directory = private_directory();
    let path = directory.path().join("session.json");
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = instance();
    let member = instance();
    let outsider = instance();
    let token = "synthetic-member-token-".repeat(3);
    let outsider_token = "synthetic-outsider-token-".repeat(3);
    let security = GatewaySecurity::initial(&[23; 32], SpacePolicy::default()).unwrap();
    let credentials = vec![(token.clone(), member), (outsider_token.clone(), outsider)];
    let store = GatewayStore::new(db.pool.clone());
    let server = GatewayServer::new(
        store.clone(),
        space,
        authority,
        credentials.clone(),
        security.clone(),
    )
    .await
    .unwrap();
    let fence = store.initialize(space, authority).await.unwrap();
    let grant = intent(
        space,
        authority,
        PublicationRecord::Membership {
            member,
            capability: SpaceCapability::Pull,
        },
    );
    let accepted = store
        .accept(fence, authority, &grant)
        .await
        .unwrap()
        .delivery
        .unwrap();
    let (body, car) = proof(&accepted);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let expires = chrono::Utc::now().timestamp() as u64 + 3600;
    let valid = bundle(endpoint.origin(), 2, expires);
    let expected_header = format!("Bearer {}", valid["access_token"].as_str().unwrap());
    let reads = Arc::new(AtomicUsize::new(0));
    let route_reads = reads.clone();
    let get_record = get(move |headers: HeaderMap| {
        let expected = expected_header.clone();
        let body = body.clone();
        let reads = route_reads.clone();
        async move {
            reads.fetch_add(1, Ordering::SeqCst);
            if headers.get("authorization").and_then(|h| h.to_str().ok()) == Some(&expected) {
                (StatusCode::OK, Json(body))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error":"ExpiredToken"})),
                )
            }
        }
    });
    let router = Router::new()
        .route("/xrpc/com.atproto.repo.getRecord", get_record)
        .route(
            "/xrpc/com.atproto.sync.getRepo",
            get(move || {
                let car = car.clone();
                async move { car }
            }),
        );
    let pds_task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let publisher = || {
        PdsPublisher::with_rotating_credentials(
            endpoint.clone(),
            REPOSITORY.into(),
            path.clone(),
            None,
            FixtureKey::K256.did_key(),
            space,
        )
        .unwrap()
    };
    let pds = publisher();
    assert_eq!(
        server.publish_one(&pds).await.unwrap(),
        WorkerOutcome::Blocked
    );
    let pending = store.pending_publication(fence).await.unwrap().unwrap();
    assert_eq!(pending.attempts, 0);
    assert!(!pending.leased);
    replace(
        &path,
        &bundle(endpoint.origin(), 1, chrono::Utc::now().timestamp() as u64),
    );
    assert_eq!(
        server.publish_one(&pds).await.unwrap(),
        WorkerOutcome::Blocked
    );
    assert_eq!(
        store
            .pending_publication(fence)
            .await
            .unwrap()
            .unwrap()
            .attempts,
        0
    );
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    // A syntactically valid credential can still be rejected by the PDS.
    replace(&path, &bundle(endpoint.origin(), 2, expires + 1));
    assert_eq!(
        server.publish_one(&pds).await.unwrap(),
        WorkerOutcome::Blocked
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .pending_publication(fence)
            .await
            .unwrap()
            .unwrap()
            .last_error
            .as_deref(),
        Some("authentication:2")
    );
    let restarted = GatewayServer::new(
        store.clone(),
        space,
        authority,
        credentials,
        security.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        restarted.publish_one(&publisher()).await.unwrap(),
        WorkerOutcome::Blocked
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1); // persisted generation prevented retry
    // The exact rejected generation cannot be changed into another token.
    replace(&path, &valid);
    assert_eq!(
        restarted.publish_one(&publisher()).await.unwrap(),
        WorkerOutcome::Blocked
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    // Newer generation with the same usable token is a synthetic stand-in for
    // a successfully verified same-account refresh from the operator.
    let mut newer = valid.clone();
    newer["generation"] = json!(3);
    replace(&path, &newer);
    let pds = publisher();
    assert_eq!(
        restarted.publish_one(&pds).await.unwrap(),
        WorkerOutcome::Published(1)
    );
    let server = restarted.with_publication_status(pds);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let router = server.router();
    let gateway_task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = GatewayClient::new(
        gateway_endpoint.clone(),
        token,
        space,
        authority,
        security.trust(),
    )
    .unwrap();
    let status = client.status(member).await.unwrap();
    assert_eq!(status.credentials.generation, Some(3));
    assert!(status.credentials.publication_usable);
    assert!(status.pending.is_none());
    assert!(matches!(
        client.status(outsider).await,
        Err(RuntimeError::InvalidResponse)
    ));
    let denied = GatewayClient::new(
        gateway_endpoint,
        outsider_token,
        space,
        authority,
        security.trust(),
    )
    .unwrap();
    assert!(matches!(
        denied.status(outsider).await,
        Err(RuntimeError::Unauthorized)
    ));
    let revoke = intent(
        space,
        authority,
        PublicationRecord::Revocation {
            member,
            capability: Some(SpaceCapability::Pull),
        },
    );
    let fence = store.initialize(space, authority).await.unwrap();
    store.accept(fence, authority, &revoke).await.unwrap();
    assert!(matches!(
        client.status(member).await,
        Err(RuntimeError::Unauthorized)
    ));
    gateway_task.abort();
    pds_task.abort();
    db.cleanup().await.unwrap();
}
