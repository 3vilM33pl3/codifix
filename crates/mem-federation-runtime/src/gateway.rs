// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{PdsPublisher, Result, RuntimeError};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use mem_federation::publication::*;
use mem_federation_store::{
    Acceptance, GatewayStore, PublicationIntent, PublishedPage, RetryPolicy, WorkerOutcome,
    WriterFence,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Gateway tokens identify enrolled INSTANCES, never original authors/users.
/// They do not confer capabilities: current DB membership is rechecked on use.
#[derive(Clone)]
pub struct GatewayServer {
    store: GatewayStore,
    fence: WriterFence,
    space: SpaceId,
    credentials: Vec<([u8; 32], InstanceId)>,
}
impl GatewayServer {
    pub async fn new(
        store: GatewayStore,
        space: SpaceId,
        authority: InstanceId,
        credentials: Vec<(String, InstanceId)>,
    ) -> Result<Self> {
        if credentials.iter().any(|(token, _)| token.len() < 32) {
            return Err(RuntimeError::Configuration);
        }
        let credentials: Vec<([u8; 32], InstanceId)> = credentials
            .into_iter()
            .map(|(token, id)| (Sha256::digest(token.as_bytes()).into(), id))
            .collect();
        for (i, (hash, _)) in credentials.iter().enumerate() {
            if credentials[..i].iter().any(|(prior, _)| prior == hash) {
                return Err(RuntimeError::Configuration);
            }
        }
        let old = store.initialize(space, authority).await?;
        let fence = store.rotate_writer(old).await?;
        Ok(Self {
            store,
            fence,
            space,
            credentials,
        })
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/v1/accept", post(accept))
            .route("/v1/pull", get(pull))
            .layer(DefaultBodyLimit::max(96 * 1024))
            .with_state(Arc::new(self.clone()))
    }
    pub async fn publish_one(&self, pds: &PdsPublisher) -> Result<WorkerOutcome> {
        Ok(self
            .store
            .publish_one(self.fence, RetryPolicy::default(), pds)
            .await?)
    }
    fn authenticate(&self, headers: &HeaderMap) -> std::result::Result<InstanceId, StatusCode> {
        if headers.get_all("authorization").iter().count() != 1 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| v.len() <= 4096)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.credentials
            .iter()
            .find(|(stored, _)| {
                stored
                    .iter()
                    .zip(hash)
                    .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                    == 0
            })
            .map(|(_, id)| *id)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}
async fn accept(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(intent): Json<PublicationIntent>,
) -> std::result::Result<Json<Acceptance>, StatusCode> {
    let publisher = server.authenticate(&headers)?;
    let delivery = PublicationDelivery::new(
        AuthoritySequence::new(u32::MAX).map_err(|_| StatusCode::BAD_REQUEST)?,
        intent.locator.clone(),
        intent.envelope.clone(),
    )
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    mem_atproto::publication::PublicationWire::encode(delivery)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    server
        .store
        .accept(server.fence, publisher, &intent)
        .await
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
#[derive(Deserialize)]
struct PullQuery {
    after: u32,
    limit: u32,
}
async fn pull(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> std::result::Result<Json<PublishedPage>, StatusCode> {
    let publisher = server.authenticate(&headers)?;
    if !(1..=32).contains(&query.limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    server
        .store
        .pull_page(server.space, publisher, query.after, query.limit)
        .await
        .map(Json)
        .map_err(|e| match e {
            mem_federation_store::StoreError::Outcome(PublicationOutcome::Rejected(
                PublicationReject::Unauthorized,
            )) => StatusCode::FORBIDDEN,
            mem_federation_store::StoreError::Resync => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        })
}
