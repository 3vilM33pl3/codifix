// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]
//! Real loopback HTTP/signatures and disposable DB; not origin/service/PDS E2E.
use mem_federation::{expertise::*, publication::*, *};
use mem_federation_runtime::*;
use mem_federation_store::*;
use uuid::Uuid;
#[path = "support/f1_collections.rs"]
mod f1_collections;

fn instance() -> InstanceId {
    InstanceId::new(Uuid::new_v4()).unwrap()
}
fn token() -> String {
    format!("disposable-f1-{}", Uuid::new_v4())
}
fn intent(space: SpaceId, publisher: InstanceId, record: PublicationRecord) -> PublicationIntent {
    let id = Uuid::new_v4();
    PublicationIntent {
        locator: RecordLocator::new(format!("fixture:{id}")).unwrap(),
        envelope: PublicationEnvelope {
            schema_version: 3,
            space,
            publisher,
            record_id: RecordId::new(id).unwrap(),
            author: AuthorIdentity::new("fixture-author").unwrap(),
            authored_at: chrono::Utc::now(),
            causation: None,
            record,
        },
    }
}
async fn publish(
    store: &GatewayStore,
    fence: WriterFence,
    security: &GatewaySecurity,
    intent: &PublicationIntent,
) {
    assert_eq!(
        store
            .accept(fence, intent.envelope.publisher, intent)
            .await
            .unwrap()
            .outcome,
        PublicationOutcome::Applied
    );
    let Claim::Ready(lease) = store
        .claim_publication(fence, RetryPolicy::default())
        .await
        .unwrap()
    else {
        panic!("claim");
    };
    // Simulate completed publication only. No claim of PDS verification here.
    store
        .acknowledge_with_receipt(&lease, serde_json::json!({"fixture_only":true}), security)
        .await
        .unwrap();
}
async fn listen(server: &GatewayServer) -> (Endpoint, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint =
        Endpoint::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap();
    let router = server.router();
    (
        endpoint,
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}

#[tokio::test]
async fn private_http_exchange_is_explicit_addressed_signed_and_recoverable() {
    let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let store = GatewayStore::new(db.pool.clone());
    let space = SpaceId::new(Uuid::new_v4()).unwrap();
    let authority = instance();
    let memory = instance();
    let homelab = instance();
    let game = instance();
    let root_token = token();
    let memory_token = token();
    let homelab_token = token();
    let game_token = token();
    let credentials = vec![
        (root_token.clone(), authority),
        (memory_token.clone(), memory),
        (homelab_token.clone(), homelab),
        (game_token.clone(), game),
    ];
    let security = GatewaySecurity::initial(&[41; 32], SpacePolicy::default()).unwrap();
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
    let (disabled_endpoint, disabled_task) = listen(&server).await;
    let response = reqwest::Client::new()
        .post(format!("{}/v1/expertise", disabled_endpoint.origin()))
        .bearer_auth(&root_token)
        .json(&ExpertiseRpc {
            nonce: Uuid::new_v4(),
            operation: ExpertiseOperation::Claim,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    disabled_task.abort();
    let fleet = [(authority, 3), (memory, 3), (homelab, 3), (game, 3)];
    let mut old_fleet = fleet;
    old_fleet[1].1 = 2;
    assert!(
        server
            .clone()
            .enable_expertise_rehearsal(&old_fleet)
            .await
            .is_err()
    );
    assert!(
        server
            .clone()
            .enable_expertise_rehearsal(&fleet[..3])
            .await
            .is_err()
    );
    let server = server.enable_expertise_rehearsal(&fleet).await.unwrap();
    for member in [memory, homelab] {
        for capability in [
            SpaceCapability::Pull,
            SpaceCapability::Publish,
            SpaceCapability::AskExpertise,
            SpaceCapability::AnswerExpertise,
        ] {
            publish(
                &store,
                fence,
                &security,
                &intent(
                    space,
                    authority,
                    PublicationRecord::Membership { member, capability },
                ),
            )
            .await;
        }
    }
    let mut ad = CapabilityAdvertisement {
        owner: memory,
        collection: RecordId::new(Uuid::new_v4()).unwrap(),
        revision: StateRevision::new(1).unwrap(),
        collection_digest: CanonicalJsonCodec::digest(&"reviewed private memory snapshot").unwrap(),
        name: "Memory expertise".into(),
        description: "Reviewed metadata only".into(),
        topics: vec!["memory".into()],
        modes: vec![AnswerMode::Deterministic],
        requesters: vec![homelab],
        model_destinations: vec![],
        output_destinations: vec!["local_human".into()],
        active: true,
        limits: Default::default(),
    };
    publish(
        &store,
        fence,
        &security,
        &intent(space, memory, PublicationRecord::Expertise(ad.clone())),
    )
    .await;
    let (endpoint, task) = listen(&server).await;
    let client = |token: String| {
        GatewayClient::new(endpoint.clone(), token, space, authority, security.trust()).unwrap()
    };
    let requester = client(homelab_token.clone());
    let responder = client(memory_token);
    let game_client = client(game_token);
    let discover = || ExpertiseOperation::Discover {
        after: None,
        limit: 32,
    };
    let ExpertiseReply::Capabilities(ads) = requester.expertise(homelab, discover()).await.unwrap()
    else {
        panic!("discovery");
    };
    assert_eq!(ads, vec![ad.clone()]);
    assert!(game_client.expertise(game, discover()).await.is_err());
    assert!(requester.expertise(memory, discover()).await.is_err()); // wrong audience fails verification
    let created_at = chrono::Utc::now();
    let q = ExpertiseQuestion {
        request_id: Uuid::new_v4(),
        space,
        requester: homelab,
        principal: Uuid::new_v4(),
        capability: ad.key(),
        revision: ad.revision,
        collection_digest: ad.collection_digest.clone(),
        question: "Private Q&A, never a public timeline item".into(),
        mode: AnswerMode::Deterministic,
        model_destination: None,
        output_destination: "local_human".into(),
        created_at,
        expires_at: created_at + chrono::Duration::seconds(120),
    };
    let submit = || ExpertiseOperation::Submit {
        question: q.clone(),
    };
    for _ in 0..2 {
        requester.expertise(homelab, submit()).await.unwrap();
    }
    let mut changed = q.clone();
    changed.question.push('?');
    assert!(
        requester
            .expertise(homelab, ExpertiseOperation::Submit { question: changed })
            .await
            .is_err()
    );
    let result = || ExpertiseOperation::Result {
        request_id: q.request_id,
    };
    assert!(game_client.expertise(game, result()).await.is_err());
    assert!(responder.expertise(memory, result()).await.is_err());
    let (a, b) = tokio::join!(
        responder.expertise(memory, ExpertiseOperation::Claim),
        responder.expertise(memory, ExpertiseOperation::Claim)
    );
    let mut leases = [a.unwrap(), b.unwrap()]
        .into_iter()
        .filter_map(|r| match r {
            ExpertiseReply::Lease(l) => l,
            _ => panic!("lease"),
        });
    let lease = leases.next().unwrap();
    assert!(leases.next().is_none());
    assert_eq!(lease.question, q);
    responder
        .expertise(
            memory,
            ExpertiseOperation::Renew {
                lease: (*lease).clone(),
            },
        )
        .await
        .unwrap();
    // A refused result tests transport completion without pretending that a
    // gateway can validate the responder's undisclosed source database.
    let answer = ExpertiseAnswer {
        request_digest: q.digest().unwrap(),
        status: ExpertiseStatus::Refused,
        text: "Insufficient reviewed evidence".into(),
        citations: vec![],
        insufficient_evidence: true,
        reason: None,
        untrusted: true,
    };
    for _ in 0..2 {
        responder
            .expertise(
                memory,
                ExpertiseOperation::Complete {
                    lease: (*lease).clone(),
                    answer: answer.clone(),
                },
            )
            .await
            .unwrap();
    }
    let ExpertiseReply::Question(result) = requester.expertise(homelab, result()).await.unwrap()
    else {
        panic!("result");
    };
    assert_eq!(result.answer.unwrap(), answer);
    let leaked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_accepted WHERE delivery::text LIKE '%Private Q&A%')")
        .fetch_one(&db.pool).await.unwrap();
    assert!(!leaked);
    // Enrollment alone does not grant discovery or asking this collection.
    for capability in [SpaceCapability::Pull, SpaceCapability::AskExpertise] {
        publish(
            &store,
            fence,
            &security,
            &intent(
                space,
                authority,
                PublicationRecord::Membership {
                    member: game,
                    capability,
                },
            ),
        )
        .await;
    }
    let ExpertiseReply::Capabilities(empty) =
        game_client.expertise(game, discover()).await.unwrap()
    else {
        panic!("discovery");
    };
    assert!(empty.is_empty());
    ad.revision = StateRevision::new(2).unwrap();
    ad.requesters.push(game);
    publish(
        &store,
        fence,
        &security,
        &intent(space, memory, PublicationRecord::Expertise(ad.clone())),
    )
    .await;
    let ExpertiseReply::Capabilities(ads) = game_client.expertise(game, discover()).await.unwrap()
    else {
        panic!("discovery");
    };
    assert_eq!(ads, vec![ad]);
    assert!(
        requester
            .expertise(
                homelab,
                ExpertiseOperation::Submit {
                    question: q.clone()
                }
            )
            .await
            .is_err()
    ); // stale revision even on retry
    // Network restart preserves completed metadata; old revision remains denied.
    task.abort();
    let restarted = GatewayServer::new(
        store.clone(),
        space,
        authority,
        credentials,
        security.clone(),
    )
    .await
    .unwrap()
    .enable_expertise_rehearsal(&fleet)
    .await
    .unwrap();
    let (endpoint, task) = listen(&restarted).await;
    let requester =
        GatewayClient::new(endpoint, homelab_token, space, authority, security.trust()).unwrap();
    assert!(
        requester
            .expertise(
                homelab,
                ExpertiseOperation::Result {
                    request_id: q.request_id
                }
            )
            .await
            .is_err()
    );
    task.abort();
    db.cleanup().await.unwrap();
}
