// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{GatewaySecurity, PdsPublisher, Result, RuntimeError, SignedPage};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use mem_federation::publication::*;
use mem_federation_store::{
    Acceptance, GatewayStore, PublicationIntent, RetryPolicy, WorkerOutcome, WriterFence,
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
    credentials: Vec<([u8; 32], InstanceId)>,
    security: GatewaySecurity,
    // New semantics stay unreachable through the existing deployment profile.
    // F1 activation must establish fleet compatibility before enabling them.
    expertise_enabled: bool,
}
impl GatewayServer {
    pub async fn new(
        store: GatewayStore,
        space: SpaceId,
        authority: InstanceId,
        credentials: Vec<(String, InstanceId)>,
        security: GatewaySecurity,
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
        let (records, receipts) = store.audited_history(old).await?;
        security
            .trust()
            .verify_history(authority, &records, &receipts)?;
        let fence = store
            .enroll_security(old, security.trust().keys(), security.policy())
            .await?;
        Ok(Self {
            store,
            fence,
            credentials,
            security,
            expertise_enabled: false,
        })
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/v1/accept", post(accept))
            .route("/v1/pull", get(pull))
            .route("/v1/retention", post(retention))
            .layer(DefaultBodyLimit::max(96 * 1024))
            .with_state(Arc::new(self.clone()))
    }
    pub async fn publish_one(&self, pds: &PdsPublisher) -> Result<WorkerOutcome> {
        if self
            .security
            .trust()
            .keys()
            .iter()
            .any(|key| key.public_key == pds.signing_key)
        {
            return Err(RuntimeError::Configuration);
        }
        let policy = RetryPolicy::default();
        let lease = match self.store.claim_publication(self.fence, policy).await? {
            mem_federation_store::Claim::Ready(lease) => lease,
            mem_federation_store::Claim::Idle => return Ok(WorkerOutcome::Idle),
            mem_federation_store::Claim::Waiting => return Ok(WorkerOutcome::Waiting),
            mem_federation_store::Claim::Blocked => return Ok(WorkerOutcome::Blocked),
        };
        match pds.publish_verified(lease.delivery()).await {
            Ok(proof) => {
                self.store
                    .acknowledge_with_receipt(
                        &lease,
                        serde_json::to_value(proof).map_err(|_| RuntimeError::InvalidResponse)?,
                        &self.security,
                    )
                    .await?;
                Ok(WorkerOutcome::Published(lease.delivery().sequence.get()))
            }
            Err(failure) => Ok(
                if self.store.fail_publication(&lease, policy, failure).await? {
                    WorkerOutcome::Blocked
                } else {
                    WorkerOutcome::Retrying
                },
            ),
        }
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
async fn retention(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(request): Json<mem_federation_store::RetentionRequest>,
) -> std::result::Result<Json<mem_federation_store::RetentionStatus>, StatusCode> {
    let actor = server.authenticate(&headers)?;
    server
        .store
        .retention_decision(server.fence, actor, &request)
        .await
        .map(Json)
        .map_err(|e| match e {
            mem_federation_store::StoreError::Binding => StatusCode::FORBIDDEN,
            mem_federation_store::StoreError::Limits => StatusCode::BAD_REQUEST,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        })
}
async fn accept(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(intent): Json<PublicationIntent>,
) -> std::result::Result<Json<Acceptance>, StatusCode> {
    let publisher = server.authenticate(&headers)?;
    if intent.envelope.schema_version == mem_federation::expertise::EXPERTISE_SCHEMA_VERSION
        && !server.expertise_enabled
    {
        return Err(StatusCode::FORBIDDEN);
    }
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
    nonce: uuid::Uuid,
}
async fn pull(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> std::result::Result<Json<SignedPage>, StatusCode> {
    let publisher = server.authenticate(&headers)?;
    if !(1..=32).contains(&query.limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let page = server
        .store
        .pull_audited(server.fence, publisher, query.after, query.limit)
        .await
        .map_err(|e| match e {
            mem_federation_store::StoreError::Outcome(PublicationOutcome::Rejected(
                PublicationReject::Unauthorized,
            )) => StatusCode::FORBIDDEN,
            mem_federation_store::StoreError::Resync => StatusCode::CONFLICT,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        })?;
    server
        .security
        .sign_page(page, query.nonce)
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
