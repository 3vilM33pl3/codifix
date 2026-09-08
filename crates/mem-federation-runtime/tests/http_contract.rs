// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use mem_atproto::publication::PublicationWire;
use mem_federation::{AuthorIdentity, RecordLocator, SpaceCapability, publication::*};
use mem_federation_runtime::{Endpoint, GatewayClient, PdsPublisher};
use mem_federation_store::PublishFailure;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

fn delivery() -> PublicationDelivery {
    let publisher = InstanceId::new(Uuid::new_v4()).unwrap();
    PublicationDelivery::new(
        AuthoritySequence::new(1).unwrap(),
        RecordLocator::new("record:one").unwrap(),
        PublicationEnvelope {
            schema_version: 2,
            space: SpaceId::new(Uuid::new_v4()).unwrap(),
            publisher,
            record_id: RecordId::new(Uuid::new_v4()).unwrap(),
            author: AuthorIdentity::new("expert").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record: PublicationRecord::Membership {
                member: publisher,
                capability: SpaceCapability::Pull,
            },
        },
    )
    .unwrap()
}
async fn serve(router: Router) -> (Endpoint, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (endpoint, task)
}

#[test]
fn endpoint_is_an_explicit_origin_not_record_controlled_discovery() {
    for url in [
        "http://example.com",
        "https://user:password@example.com",
        "https://example.com/path",
        "https://example.com/?query=1",
        "https://example.com/#fragment",
        "file:///etc/passwd",
    ] {
        assert!(Endpoint::new(url, false).is_err());
    }
    assert!(Endpoint::new("https://example.com", false).is_ok());
}

#[tokio::test]
async fn conflicting_or_malformed_pds_readback_never_triggers_overwrite() {
    let delivery = delivery();
    let wire = PublicationWire::encode(delivery.clone()).unwrap();
    let repo = "did:web:space.federation.example.com";
    for body in [
        json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),"cid":"wrong-cid","value":wire.raw()}),
        json!({"unrecognized":"response"}),
    ] {
        let writes = Arc::new(AtomicUsize::new(0));
        let seen = writes.clone();
        let router = Router::new()
            .route(
                "/xrpc/com.atproto.repo.getRecord",
                get(move || async move { Json(body) }),
            )
            .route(
                "/xrpc/com.atproto.repo.createRecord",
                post(move || async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }),
            );
        let (endpoint, task) = serve(router).await;
        let publisher = PdsPublisher::new(endpoint, repo.into(), "fixture".repeat(8)).unwrap();
        assert!(matches!(
            publisher.publish_exact(&delivery).await,
            Err(PublishFailure::Conflict | PublishFailure::InvalidReceipt)
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        task.abort();
    }
}

#[tokio::test]
async fn oversized_gateway_pages_and_redirects_fail_closed() {
    let delivery = delivery();
    let oversized = "x".repeat(3 * 1024 * 1024 + 1);
    let (endpoint, task) =
        serve(Router::new().route("/v1/pull", get(move || async move { oversized }))).await;
    let client = GatewayClient::new(
        endpoint,
        "t".repeat(64),
        delivery.envelope.space,
        delivery.envelope.publisher,
    )
    .unwrap();
    assert!(client.pull(0, 32).await.is_err());
    task.abort();
    let seen = Arc::new(AtomicUsize::new(0));
    let hits = seen.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/redirect-target", listener.local_addr().unwrap());
    let target = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/redirect-target",
                get(move || async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }),
            ),
        )
        .await
        .unwrap();
    });
    let (endpoint, task) = serve(Router::new().route(
        "/v1/pull",
        get(move || async move { (StatusCode::TEMPORARY_REDIRECT, [("location", destination)]) }),
    ))
    .await;
    let client = GatewayClient::new(
        endpoint,
        "t".repeat(64),
        delivery.envelope.space,
        delivery.envelope.publisher,
    )
    .unwrap();
    assert!(client.pull(0, 32).await.is_err());
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    task.abort();
    target.abort();
}
