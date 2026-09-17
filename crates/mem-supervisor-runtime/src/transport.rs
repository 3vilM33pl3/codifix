//! One-shot credential-bearing adapter outside Pi. No background scheduler or
//! automatic model retry exists here. The caller supplies only currently eligible,
//! reviewed input and must enforce destination consent before invoking this API.

use crate::model::{CountedRequest, ModelError, PreparedRequest, Proposal};
use chrono::Utc;
use mem_supervisor::{DEFAULT_MODEL, StableId, UnixSeconds, budget::ProviderRequest};
use mem_supervisor_store::{LeaseClaim, Reservation, SendPermit, StoreError, SupervisorStore};
use reqwest::{
    Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

const BASE: &str = "https://api.openai.com/v1";
const RESPONSE_LIMIT: usize = crate::model::MAX_RESPONSE_BYTES;

#[cfg(test)]
mod tests;

/// Neither this type nor its secret-bearing headers implement Debug/Serialize.
/// Construction reads only the explicitly approved normal variable. It never
/// falls back to an R3 key, credential files, another provider or a caller URL.
pub struct OpenAiTransport {
    client: Client,
    base: String,
    #[cfg(test)]
    semantic_model_time: Option<UnixSeconds>,
}

/// Trusted caller's stable execution binding, not a disclosure capability.
pub struct ProposalExecution {
    pub policy_revision: Uuid,
    pub attempt_id: Uuid,
    pub project_id: Uuid,
}

impl OpenAiTransport {
    /// Execute once, then persist the private suggestion and its completed
    /// action together. The caller must authorize this exact project and retain
    /// reviewed source/version bindings independently of the metadata journal.
    /// A crash after billing but before persistence leaves Running/Uncertain,
    /// never Verified. Recover completed bodies via `private_proposal`; an
    /// existing attempt never authorizes another model call.
    pub async fn execute_and_persist(
        &self,
        store: &SupervisorStore,
        lease: &LeaseClaim,
        execution: &ProposalExecution,
        request: &PreparedRequest,
    ) -> Result<mem_supervisor_store::PrivateProposal, TransportError> {
        if execution.project_id.is_nil() {
            return Err(TransportError::Authority);
        }
        let proposal = self
            .execute(
                store,
                lease,
                execution.policy_revision,
                execution.attempt_id,
                request,
            )
            .await?;
        store
            .publish_proposal(
                lease,
                &execution.attempt_id,
                execution.project_id,
                &proposal,
                request.evidence_digests(),
            )
            .await
            .map_err(|_| TransportError::Journal)
    }

    pub fn from_environment() -> Result<Self, TransportError> {
        let secret =
            std::env::var("OPENAI_API_KEY").map_err(|_| TransportError::CredentialUnavailable)?;
        Self::build(&secret, BASE)
    }

    fn build(secret: &str, base: &str) -> Result<Self, TransportError> {
        if secret.trim().is_empty() || secret.len() > 1024 {
            return Err(TransportError::CredentialUnavailable);
        }
        let mut value = HeaderValue::from_str(&format!("Bearer {secret}"))
            .map_err(|_| TransportError::CredentialUnavailable)?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        let client = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| TransportError::Unavailable)?;
        Ok(Self {
            client,
            base: base.into(),
            #[cfg(test)]
            semantic_model_time: None,
        })
    }

    fn model_time(&self, real_seconds: i64) -> Result<UnixSeconds, TransportError> {
        // Offline fixtures must not expire when reviewed production prices do.
        // There is no configurable clock override in a production binary.
        #[cfg(test)]
        if let Some(now) = self.semantic_model_time {
            return Ok(now);
        }
        epoch(real_seconds)
    }

    async fn post(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
        limit: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>, TransportError> {
        let mut response = self
            .client
            .post(format!("{}{endpoint}", self.base))
            .json(body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|_| TransportError::Uncertain)?;
        if !response.status().is_success() {
            // Do not inspect or leak error bodies. Even non-2xx status alone is
            // not proof of zero billing: retain admitted reservations.
            return Err(TransportError::ProviderRejected);
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|v| v.split(';').next().map(str::trim) != Some("application/json"))
        {
            return Err(TransportError::Uncertain);
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(TransportError::Uncertain);
        }
        let mut result = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| TransportError::Uncertain)?
        {
            if result.len().saturating_add(chunk.len()) > limit {
                return Err(TransportError::Uncertain);
            }
            result.extend_from_slice(&chunk);
        }
        Ok(result)
    }

    async fn count<'a>(
        &self,
        store: &SupervisorStore,
        lease: &LeaseClaim,
        request: &'a PreparedRequest,
    ) -> Result<CountedRequest<'a>, TransportError> {
        store
            .check_current(lease)
            .await
            .map_err(|_| TransportError::Authority)?;
        let body = self
            .post(
                "/responses/input_tokens",
                &request.count_body(),
                4096,
                Duration::from_secs(20),
            )
            .await?;
        let now = store
            .check_current(lease)
            .await
            .map_err(|_| TransportError::Authority)?;
        request
            .accept_count(&body, self.model_time(now.timestamp())?)
            .map_err(TransportError::Model)
    }

    async fn send_once(
        &self,
        permit: SendPermit,
        counted: &CountedRequest<'_>,
    ) -> Result<Vec<u8>, TransportError> {
        if permit.model() != DEFAULT_MODEL
            || permit.request_digest() != digest_hex(counted.digest())?
            || permit.worst_case_cost() != counted.reservation()
        {
            return Err(TransportError::PermitMismatch);
        }
        let now = Utc::now();
        if now < permit.admitted_at() || now >= permit.expires_at() {
            return Err(TransportError::Authority);
        }
        let remaining = (permit.expires_at() - now)
            .to_std()
            .map_err(|_| TransportError::Authority)?;
        let body = counted
            .generation_body(self.model_time(now.timestamp())?)
            .map_err(TransportError::Model)?;
        self.post(
            "/responses",
            body,
            RESPONSE_LIMIT,
            remaining.min(Duration::from_secs(60)),
        )
        .await
    }

    /// Caller must already have a durably authorized/claimed action and current
    /// collection/destination consent. This function never manufactures consent.
    /// `attempt_id` must be stable across recovery: an existing attempt is NOT a
    /// send permit. No model retry is performed on any error.
    pub async fn execute(
        &self,
        store: &SupervisorStore,
        lease: &LeaseClaim,
        policy_revision: Uuid,
        attempt_id: Uuid,
        request: &PreparedRequest,
    ) -> Result<Proposal, TransportError> {
        if attempt_id.is_nil() || policy_revision.is_nil() {
            return Err(TransportError::PermitMismatch);
        }
        store
            .check_current(lease)
            .await
            .map_err(|_| TransportError::Authority)?;
        let action = store
            .action(lease.area(), lease.action_id())
            .await
            .map_err(|_| TransportError::Authority)?;
        if action.policy_revision != policy_revision
            || matches!(
                action.kind,
                mem_supervisor::action::ActionKind::HealthCheck
                    | mem_supervisor::action::ActionKind::InstallMaintenance
            )
        {
            return Err(TransportError::Authority);
        }
        let digest = digest_hex(request.digest())?;
        // Recovery of a known attempt must not even redisclose its input to the
        // token counter. Absence must be authoritative; DB errors fail closed.
        match store.attempt(attempt_id).await {
            Ok(existing) => {
                if existing.area != lease.area()
                    || existing.action_id != lease.action_id()
                    || existing.policy_revision != policy_revision
                    || existing.request_digest != digest
                {
                    return Err(TransportError::PermitMismatch);
                }
                return Err(TransportError::ExistingAttempt);
            }
            Err(StoreError::NotFound) => {}
            Err(_) => return Err(TransportError::Authority),
        }
        let counted = self.count(store, lease, request).await?;
        let provider_request = ProviderRequest {
            id: stable(attempt_id)?,
            action_id: stable(lease.action_id())?,
            policy_revision: stable(policy_revision)?,
            model: StableId::new(DEFAULT_MODEL).map_err(|_| TransportError::PermitMismatch)?,
            worst_case_cost: counted.reservation(),
        };
        let reservation = store
            .reserve(lease, &provider_request, digest)
            .await
            .map_err(|_| TransportError::Authority)?;
        if !matches!(reservation, Reservation::Reserved(_)) {
            return Err(TransportError::ExistingAttempt);
        }
        let permit = store
            .begin(lease, attempt_id)
            .await
            .map_err(|_| TransportError::Authority)?;
        // From this point onward, losing control flow leaves a durable in-flight
        // attempt. Recovery must classify it uncertain, never invoke it again.
        let response = match self.send_once(permit, &counted).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = store.mark_uncertain(attempt_id).await;
                return Err(error);
            }
        };
        let measured = match counted.inspect_response(&response) {
            Ok(value) => value,
            Err(error) => {
                let _ = store.mark_uncertain(attempt_id).await;
                return Err(TransportError::Model(error));
            }
        };
        let evidence: [u8; 32] = Sha256::digest(&response).into();
        store
            .settle(attempt_id, measured.actual_cost, evidence)
            .await
            .map_err(|_| TransportError::Journal)?;
        if measured.exceeded_reservation {
            store
                .revoke_policy(lease.area(), policy_revision)
                .await
                .map_err(|_| TransportError::Journal)?;
            return Err(TransportError::Model(ModelError::ExceededReservation));
        }
        // A billed reply is not disclosure authorization. Expired/revoked leases
        // cannot accept results even though their usage was settled above. The
        // caller must persist the private structured proposal and then publish
        // its digest atomically. This adapter does NOT mark the action Verified:
        // doing so here would turn a returned-only proposal into lost history.
        store
            .check_current(lease)
            .await
            .map_err(|_| TransportError::Authority)?;
        measured.proposal.map_err(TransportError::Model)
    }
}

fn epoch(seconds: i64) -> Result<UnixSeconds, TransportError> {
    u64::try_from(seconds)
        .map(UnixSeconds)
        .map_err(|_| TransportError::Authority)
}
fn stable(id: Uuid) -> Result<StableId, TransportError> {
    StableId::new(id.to_string()).map_err(|_| TransportError::PermitMismatch)
}
fn digest_hex(s: &str) -> Result<[u8; 32], TransportError> {
    if s.len() != 64 {
        return Err(TransportError::PermitMismatch);
    }
    let mut digest = [0; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .map_err(|_| TransportError::PermitMismatch)?;
    }
    Ok(digest)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("normal OpenAI credential is unavailable or invalid")]
    CredentialUnavailable,
    #[error("approved model transport is unavailable")]
    Unavailable,
    #[error("provider completion or usage is uncertain; automatic retry is forbidden")]
    Uncertain,
    #[error("provider rejected the request; no response body was recorded")]
    ProviderRejected,
    #[error("current supervisor authority or lease is unavailable")]
    Authority,
    #[error("attempt already exists and cannot authorize another physical send")]
    ExistingAttempt,
    #[error("provider request does not match the durable send permit")]
    PermitMismatch,
    #[error("provider outcome could not be committed; reconcile before proceeding")]
    Journal,
    #[error(transparent)]
    Model(#[from] ModelError),
}
