// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use codifix_atproto::publication::PublicationWire;
use codifix_atproto_verify::fixtures::{FixtureKey, signed_repo};
use codifix_federation::{AuthorIdentity, RecordLocator, SpaceCapability, publication::*};
use codifix_federation_runtime::{Endpoint, GatewayClient, PdsPublisher};
use codifix_federation_store::PublishFailure;
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
async fn pds_authentication_is_not_reported_as_a_repository_integrity_failure() {
    let delivery = delivery();
    let wire = PublicationWire::encode(delivery.clone()).unwrap();
    let repo = "did:web:space.federation.example.com";
    let expected = json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),"cid":wire.cid().unwrap(),"value":wire.raw()});
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        for reject_car in [false, true] {
            let body = expected.clone();
            let writes = Arc::new(AtomicUsize::new(0));
            let seen = writes.clone();
            let router = Router::new()
                .route(
                    "/xrpc/com.atproto.repo.getRecord",
                    get(move || async move {
                        if reject_car {
                            (StatusCode::OK, Json(body))
                        } else {
                            (
                                status,
                                Json(json!({"private":"must not be parsed or logged"})),
                            )
                        }
                    }),
                )
                .route(
                    "/xrpc/com.atproto.sync.getRepo",
                    get(move || async move { status }),
                )
                .route(
                    "/xrpc/com.atproto.repo.createRecord",
                    post(move || async move {
                        seen.fetch_add(1, Ordering::SeqCst);
                        StatusCode::OK
                    }),
                );
            let (endpoint, task) = serve(router).await;
            let publisher = PdsPublisher::new(
                endpoint,
                repo.into(),
                "fixture".repeat(8),
                FixtureKey::K256.did_key(),
                delivery.envelope.space,
            )
            .unwrap();
            assert_eq!(
                publisher.publish_exact(&delivery).await,
                Err(if status == StatusCode::UNAUTHORIZED {
                    PublishFailure::Authentication { generation: 0 }
                } else {
                    PublishFailure::AuthorizationDenied
                })
            );
            assert_eq!(writes.load(Ordering::SeqCst), 0);
            task.abort();
        }
    }
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
        let publisher = PdsPublisher::new(
            endpoint,
            repo.into(),
            "fixture".repeat(8),
            FixtureKey::K256.did_key(),
            delivery.envelope.space,
        )
        .unwrap();
        assert!(matches!(
            publisher.publish_exact(&delivery).await,
            Err(PublishFailure::Conflict | PublishFailure::InvalidReceipt)
        ));
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        task.abort();
    }
}

#[tokio::test]
async fn exact_json_requires_signed_inclusion_and_cannot_cross_space() {
    let delivery = delivery();
    let wire = PublicationWire::encode(delivery.clone()).unwrap();
    let repo = "did:web:space.federation.example.com";
    let expected = json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),"cid":wire.cid().unwrap(),"value":wire.raw()});
    let records = std::collections::BTreeMap::from([(
        format!(
            "{}/{}",
            codifix_atproto::publication::COLLECTION,
            wire.record().unwrap().rkey.as_ref()
        ),
        wire.raw().clone(),
    )]);
    let signed = signed_repo(repo, &records, FixtureKey::K256);
    for (proof, valid) in [
        (b"unsigned repository".to_vec(), false),
        (signed_repo(repo, &records, FixtureKey::P256), false),
        (
            signed_repo(repo, &std::collections::BTreeMap::new(), FixtureKey::K256),
            false,
        ),
        (vec![0; codifix_atproto_verify::MAX_CAR_BYTES + 1], false),
        (signed, true),
    ] {
        let body = expected.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = hits.clone();
        let router = Router::new()
            .route(
                "/xrpc/com.atproto.repo.getRecord",
                get(move || async move { Json(body) }),
            )
            .route(
                "/xrpc/com.atproto.sync.getRepo",
                get(move || async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    proof
                }),
            );
        let (endpoint, task) = serve(router).await;
        let publisher = PdsPublisher::new(
            endpoint,
            repo.into(),
            "fixture".repeat(8),
            FixtureKey::K256.did_key(),
            delivery.envelope.space,
        )
        .unwrap();
        assert_eq!(publisher.publish_exact(&delivery).await.is_ok(), valid);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let mut wrong_space = delivery.clone();
        wrong_space.envelope.space = SpaceId::new(Uuid::new_v4()).unwrap();
        assert_eq!(
            publisher.publish_exact(&wrong_space).await,
            Err(PublishFailure::InvalidReceipt)
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        task.abort();
    }
}

#[tokio::test]
async fn addressed_question_requires_exact_signed_pds_readback() {
    use codifix_atproto::addressed_expertise::AddressedExpertiseWire;
    use codifix_federation::{
        CanonicalJsonCodec,
        expertise::{AddressedExpertiseRecord, ExpertiseQuestion},
    };
    let q: ExpertiseQuestion = serde_json::from_value(json!({
        "request_id":Uuid::from_u128(301),"space":Uuid::from_u128(302),
        "requester":Uuid::from_u128(303),"principal":Uuid::from_u128(304),
        "capability":{"publisher":Uuid::from_u128(305),"id":Uuid::from_u128(306)},
        "revision":1,"collection_digest":CanonicalJsonCodec::digest(&"reviewed").unwrap(),
        "question":"What changed?","mode":"deterministic","model_destination":null,
        "output_destination":"codex_openai",
        "created_at":"2026-10-04T10:00:00Z","expires_at":"2026-10-04T10:02:00Z"
    }))
    .unwrap();
    let addressed =
        AddressedExpertiseRecord::question(q, 1, CanonicalJsonCodec::digest(&"policy").unwrap())
            .unwrap();
    let wire = AddressedExpertiseWire::encode(addressed.clone()).unwrap();
    let repo = "did:web:space.federation.example.com";
    let record = wire.record().unwrap();
    let records = std::collections::BTreeMap::from([(
        format!("{}/{}", record.collection.as_ref(), record.rkey.as_ref()),
        wire.raw().clone(),
    )]);
    let exact = json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),
        "cid":wire.cid().unwrap(),"value":wire.raw()});
    for (body, valid) in [
        (exact.clone(), true),
        (
            json!({"uri":exact["uri"],"cid":"wrong","value":wire.raw()}),
            false,
        ),
    ] {
        let car = signed_repo(repo, &records, FixtureKey::K256);
        let writes = Arc::new(AtomicUsize::new(0));
        let seen = writes.clone();
        let router = Router::new()
            .route(
                "/xrpc/com.atproto.repo.getRecord",
                get(move || async move { Json(body) }),
            )
            .route(
                "/xrpc/com.atproto.sync.getRepo",
                get(move || async move { car }),
            )
            .route(
                "/xrpc/com.atproto.repo.createRecord",
                post(move || async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }),
            );
        let (endpoint, task) = serve(router).await;
        let publisher = PdsPublisher::new(
            endpoint,
            repo.into(),
            "fixture".repeat(8),
            FixtureKey::K256.did_key(),
            addressed.space,
        )
        .unwrap();
        assert_eq!(
            publisher
                .publish_addressed_verified(&addressed)
                .await
                .is_ok(),
            valid
        );
        assert_eq!(writes.load(Ordering::SeqCst), 0);
        task.abort();
    }
}

#[tokio::test]
async fn addressed_expiry_uses_compare_and_swap_and_newer_signed_absence() {
    use codifix_atproto::addressed_expertise::AddressedExpertiseWire;
    use codifix_federation::{
        CanonicalJsonCodec,
        expertise::{AddressedExpertiseRecord, ExpertiseQuestion},
    };
    let q: ExpertiseQuestion = serde_json::from_value(json!({
        "request_id":Uuid::from_u128(401),"space":Uuid::from_u128(402),
        "requester":Uuid::from_u128(403),"principal":Uuid::from_u128(404),
        "capability":{"publisher":Uuid::from_u128(405),"id":Uuid::from_u128(406)},
        "revision":1,"collection_digest":CanonicalJsonCodec::digest(&"reviewed").unwrap(),
        "question":"What changed?","mode":"deterministic","model_destination":null,
        "output_destination":"codex_openai",
        "created_at":"2026-10-04T10:00:00Z","expires_at":"2026-10-04T10:02:00Z"
    }))
    .unwrap();
    let addressed =
        AddressedExpertiseRecord::question(q, 1, CanonicalJsonCodec::digest(&"policy").unwrap())
            .unwrap();
    let wire = AddressedExpertiseWire::encode(addressed.clone()).unwrap();
    let repo = "did:web:space.federation.example.com";
    let encoded = wire.record().unwrap();
    let records = std::collections::BTreeMap::from([(
        format!("{}/{}", encoded.collection.as_ref(), encoded.rkey.as_ref()),
        wire.raw().clone(),
    )]);
    let included = signed_repo(repo, &records, FixtureKey::K256);
    let key = FixtureKey::K256.did_key();
    let prior = codifix_atproto_verify::PinnedRepository::new(repo.into(), key.clone())
        .unwrap()
        .verify_full_car_addressed(&included, &wire, u64::MAX)
        .unwrap();
    // The fixture's TID clock is microsecond-resolution; create the absent
    // snapshot after inclusion so its signed revision is strictly newer.
    let absent = loop {
        let candidate = signed_repo(repo, &std::collections::BTreeMap::new(), FixtureKey::K256);
        if codifix_atproto_verify::PinnedRepository::new(repo.into(), key.clone())
            .unwrap()
            .verify_full_car_absent_addressed(&candidate, &wire, &prior.revision, u64::MAX)
            .is_ok()
        {
            break candidate;
        }
    };
    let present = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let deleted = Arc::new(AtomicUsize::new(0));
    let get_present = present.clone();
    let exact = json!({"uri":wire.uri(&repo.parse().unwrap()).unwrap(),
        "cid":wire.cid().unwrap(),"value":wire.raw()});
    let get = Router::new()
        .route(
            "/xrpc/com.atproto.repo.getRecord",
            get(move || {
                let present = get_present.clone();
                let body = exact.clone();
                async move {
                    if present.load(Ordering::SeqCst) {
                        (StatusCode::OK, Json(body))
                    } else {
                        (
                            StatusCode::NOT_FOUND,
                            Json(json!({"error":"RecordNotFound"})),
                        )
                    }
                }
            }),
        )
        .route(
            "/xrpc/com.atproto.sync.getRepo",
            get({
                let present = present.clone();
                move || {
                    let present = present.clone();
                    let included = included.clone();
                    let absent = absent.clone();
                    async move {
                        if present.load(Ordering::SeqCst) {
                            included
                        } else {
                            absent
                        }
                    }
                }
            }),
        )
        .route(
            "/xrpc/com.atproto.repo.deleteRecord",
            post({
                let present = present.clone();
                let deleted = deleted.clone();
                let cid = wire.cid().unwrap();
                move |Json(body): Json<serde_json::Value>| {
                    let present = present.clone();
                    let deleted = deleted.clone();
                    let cid = cid.clone();
                    async move {
                        assert_eq!(body["swapRecord"], cid);
                        deleted.fetch_add(1, Ordering::SeqCst);
                        present.store(false, Ordering::SeqCst);
                        StatusCode::OK
                    }
                }
            }),
        );
    let (endpoint, task) = serve(get).await;
    let publisher = PdsPublisher::new(
        endpoint,
        repo.into(),
        "fixture".repeat(8),
        key,
        addressed.space,
    )
    .unwrap();
    let proof = publisher
        .delete_addressed_verified(&addressed, Some(&prior))
        .await
        .unwrap();
    assert_eq!(proof.prior_record_cid, prior.record_cid);
    assert_eq!(deleted.load(Ordering::SeqCst), 1);
    // A retry after an ambiguous acknowledgement is a verified no-op.
    publisher
        .delete_addressed_verified(&addressed, Some(&prior))
        .await
        .unwrap();
    assert_eq!(deleted.load(Ordering::SeqCst), 1);
    task.abort();
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
        codifix_federation_runtime::GatewaySecurity::initial(&[3; 32], Default::default())
            .unwrap()
            .trust(),
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
        codifix_federation_runtime::GatewaySecurity::initial(&[3; 32], Default::default())
            .unwrap()
            .trust(),
    )
    .unwrap();
    assert!(client.pull(0, 32).await.is_err());
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    task.abort();
    target.abort();
}
