// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    ExpertiseOperation, ExpertiseReply, ExpertiseRpc, GatewaySecurity, PdsPublisher, Result,
    RuntimeError, SignedExpertiseReply, SignedPage,
};
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
use std::collections::BTreeSet;
use std::sync::Arc;

/// Gateway tokens identify enrolled INSTANCES, never original authors/users.
/// They do not confer capabilities: current DB membership is rechecked on use.
#[derive(Clone)]
pub struct GatewayServer {
    pub(crate) store: GatewayStore,
    pub(crate) fence: WriterFence,
    pub(crate) delivery_waiters: crate::expertise_delivery::DeliveryWaiters,
    credentials: Vec<([u8; 32], InstanceId)>,
    security: GatewaySecurity,
    // New semantics stay unreachable through the existing deployment profile.
    // F1 activation must establish fleet compatibility before enabling them.
    expertise_enabled: bool,
    space: SpaceId,
    authority: InstanceId,
}
impl GatewayServer {
    /// Scheduled by the owning gateway runtime, with its normal writer fence.
    pub async fn maintain_expertise(&self) -> Result<()> {
        if self.expertise_enabled {
            self.store.cleanup_questions(self.fence).await?;
        }
        Ok(())
    }
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
            delivery_waiters: Default::default(),
            space,
            authority,
        })
    }
    pub fn router(&self) -> Router {
        let mut router = Router::new()
            .route("/v1/accept", post(accept))
            .route("/v1/pull", get(pull))
            .route("/v1/retention", post(retention));
        if self.expertise_enabled {
            router = router.route("/v1/expertise", post(expertise));
        }
        router
            .layer(DefaultBodyLimit::max(96 * 1024))
            .with_state(Arc::new(self.clone()))
    }
    /// Temporary rehearsal entrypoint while origin workers/UI are incomplete.
    /// Production configuration intentionally cannot enable these routes yet.
    /// Require an explicit schema inventory for ALL configured participants;
    /// no inferred capability, dynamic discovery or mixed-version downgrade.
    pub async fn enable_expertise_rehearsal(
        mut self,
        participants: &[(InstanceId, u32)],
    ) -> Result<Self> {
        let configured: BTreeSet<_> = self.credentials.iter().map(|(_, id)| *id).collect();
        let compatible: BTreeSet<_> = participants.iter().map(|(id, _)| *id).collect();
        if !cfg!(debug_assertions)
            || compatible.len() != participants.len()
            || participants
                .iter()
                .any(|(_, schema)| *schema != mem_federation::expertise::EXPERTISE_SCHEMA_VERSION)
            || configured != compatible
        {
            return Err(RuntimeError::Configuration);
        }
        self.store
            .require_expertise_fleet(self.fence, &compatible)
            .await?;
        self.expertise_enabled = true;
        Ok(self)
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
    if server.expertise_enabled
        && let PublicationRecord::Membership { member, .. } = &intent.envelope.record
        && !server.credentials.iter().any(|(_, id)| id == member)
    {
        return Err(StatusCode::FORBIDDEN);
    }
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

async fn expertise(
    State(server): State<Arc<GatewayServer>>,
    headers: HeaderMap,
    Json(request): Json<ExpertiseRpc>,
) -> std::result::Result<Json<SignedExpertiseReply>, StatusCode> {
    let actor = server.authenticate(&headers)?;
    request.digest().map_err(|_| StatusCode::BAD_REQUEST)?;
    let store = &server.store;
    let fence = server.fence;
    let reply = match &request.operation {
        ExpertiseOperation::Discover { after, limit } => ExpertiseReply::Capabilities(
            store
                .expertise_capabilities(fence, actor, *after, *limit)
                .await
                .map_err(expertise_error)?,
        ),
        ExpertiseOperation::Submit { question } => ExpertiseReply::Question(Box::new(
            store
                .submit_question(fence, actor, question)
                .await
                .map_err(expertise_error)?,
        )),
        ExpertiseOperation::Result { request_id } => ExpertiseReply::Question(Box::new(
            server
                .receive_answer(actor, *request_id, request.nonce)
                .await?,
        )),
        ExpertiseOperation::ClaimDelivery => ExpertiseReply::DeliveryLease(
            store
                .claim_answer_delivery(fence, actor)
                .await
                .map_err(expertise_error)?
                .map(Box::new),
        ),
        ExpertiseOperation::AuthorizeDelivery { authorization } => {
            server.release_answer(actor, authorization).await?;
            ExpertiseReply::Completed
        }
        ExpertiseOperation::Claim => ExpertiseReply::Lease(
            store
                .claim_question(fence, actor)
                .await
                .map_err(expertise_error)?
                .map(Box::new),
        ),
        ExpertiseOperation::Renew { lease } => ExpertiseReply::Renewed(
            store
                .renew_question(fence, actor, lease)
                .await
                .map_err(expertise_error)?,
        ),
        ExpertiseOperation::ReserveModel {
            lease,
            daily_calls,
            daily_tokens,
            tokens,
        } => {
            if *daily_calls > 20 || *daily_tokens > 800_000 || *tokens > 40_000 {
                return Err(StatusCode::BAD_REQUEST);
            }
            store
                .reserve_question_model(fence, actor, lease, *daily_calls, *daily_tokens, *tokens)
                .await
                .map_err(expertise_error)?;
            ExpertiseReply::Completed
        }
        ExpertiseOperation::Complete { lease, answer } => {
            store
                .complete_question(fence, actor, lease, answer)
                .await
                .map_err(expertise_error)?;
            ExpertiseReply::Completed
        }
    };
    server
        .security
        .sign_expertise(&request, actor, server.space, server.authority, reply)
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
pub(crate) fn expertise_error(error: mem_federation_store::StoreError) -> StatusCode {
    use mem_federation_store::StoreError;
    match error {
        StoreError::Expertise(_) | StoreError::Binding => StatusCode::FORBIDDEN,
        StoreError::Limits => StatusCode::TOO_MANY_REQUESTS,
        StoreError::LostLease => StatusCode::CONFLICT,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
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
