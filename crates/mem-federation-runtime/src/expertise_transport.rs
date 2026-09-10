// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private gateway protocol. Separate signature purposes and fresh nonces bind
//! each reply to its addressed operation; these receipts are not publications.
use crate::{GatewayClient, Result, RuntimeError};
use chrono::{DateTime, Utc};
use mem_federation::{CanonicalJsonCodec, ContentDigest, expertise::*, publication::*};
use mem_federation_store::{QuestionLease, QuestionResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExpertiseOperation {
    Discover {
        after: Option<RecordKey>,
        limit: u32,
    },
    Submit {
        question: ExpertiseQuestion,
    },
    Result {
        request_id: Uuid,
    },
    Claim,
    Renew {
        lease: QuestionLease,
    },
    Complete {
        lease: QuestionLease,
        answer: ExpertiseAnswer,
    },
}
impl ExpertiseOperation {
    pub(crate) fn purpose(&self) -> &'static str {
        match self {
            Self::Discover { .. } => "memory-expertise-discover-v1",
            Self::Submit { .. } => "memory-expertise-submit-v1",
            Self::Result { .. } => "memory-expertise-result-v1",
            Self::Claim => "memory-expertise-claim-v1",
            Self::Renew { .. } => "memory-expertise-renew-v1",
            Self::Complete { .. } => "memory-expertise-complete-v1",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseRpc {
    pub nonce: Uuid,
    pub operation: ExpertiseOperation,
}
impl ExpertiseRpc {
    pub(crate) fn digest(&self) -> Result<ContentDigest> {
        if self.nonce.is_nil() {
            return Err(RuntimeError::InvalidResponse);
        }
        CanonicalJsonCodec::digest(self).map_err(|_| RuntimeError::InvalidResponse)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ExpertiseReply {
    Capabilities(Vec<CapabilityAdvertisement>),
    Question(Box<QuestionResult>),
    Lease(Option<Box<QuestionLease>>),
    Renewed(DateTime<Utc>),
    Completed,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseClaims {
    pub purpose: String,
    pub nonce: Uuid,
    pub operation_digest: ContentDigest,
    pub audience: InstanceId,
    pub space: SpaceId,
    pub authority: InstanceId,
    pub key_generation: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub reply: ExpertiseReply,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExpertiseReply {
    pub claims: ExpertiseClaims,
    pub signature: Vec<u8>,
}

impl GatewayClient {
    /// Explicit RPC only. The calling service must authorize the local caller,
    /// record consent, and bind the onward destination BEFORE using this client.
    pub async fn expertise(
        &self,
        instance: InstanceId,
        operation: ExpertiseOperation,
    ) -> Result<ExpertiseReply> {
        let request = ExpertiseRpc {
            nonce: Uuid::new_v4(),
            operation,
        };
        let response = self
            .endpoint
            .client
            .post(self.endpoint.url("/v1/expertise")?)
            .bearer_auth(&self.token)
            .json(&request)
            .send()
            .await
            .map_err(|_| RuntimeError::Unavailable)?;
        crate::sync::check_status(response.status())?;
        let signed = crate::http::json(response, 256 * 1024).await?;
        let reply =
            self.trust
                .verify_expertise(signed, &request, instance, self.space, self.authority)?;
        match (&request.operation, &reply) {
            (ExpertiseOperation::Discover { after, limit }, ExpertiseReply::Capabilities(ads)) => {
                if !(1..=32).contains(limit)
                    || ads.len() > *limit as usize
                    || ads.windows(2).any(|pair| pair[0].key() >= pair[1].key())
                    || ads.iter().any(|ad| {
                        ad.validate().is_err()
                            || !ad.active
                            || !ad.requesters.contains(&instance)
                            || after.is_some_and(|key| ad.key() <= key)
                    })
                {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (ExpertiseOperation::Submit { question }, ExpertiseReply::Question(result)) => {
                if result.request_digest
                    != question
                        .digest()
                        .map_err(|_| RuntimeError::InvalidResponse)?
                    || result.binding.request_id != question.request_id
                {
                    return Err(RuntimeError::InvalidResponse);
                }
                validate_result(result, instance, self.space)?;
            }
            (ExpertiseOperation::Result { request_id }, ExpertiseReply::Question(result)) => {
                if *request_id != result.binding.request_id {
                    return Err(RuntimeError::InvalidResponse);
                }
                validate_result(result, instance, self.space)?;
            }
            (ExpertiseOperation::Claim, ExpertiseReply::Lease(lease)) => {
                if let Some(lease) = lease
                    && (lease.question.capability.publisher != instance
                        || lease.question.space != self.space
                        || lease.lease_token.is_nil()
                        || lease.lease_until <= Utc::now()
                        || lease.lease_until > lease.question.expires_at
                        || lease.question.validate().is_err())
                {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (ExpertiseOperation::Renew { lease }, ExpertiseReply::Renewed(until))
                if *until > Utc::now() && *until <= lease.question.expires_at => {}
            (ExpertiseOperation::Complete { .. }, ExpertiseReply::Completed) => {}
            _ => return Err(RuntimeError::InvalidResponse),
        }
        Ok(reply)
    }
}
fn validate_result(result: &QuestionResult, requester: InstanceId, space: SpaceId) -> Result<()> {
    if result.binding.requester != requester
        || result.binding.space != space
        || result.answer.as_ref().is_some_and(|a| {
            a.request_digest != result.request_digest
                || a.status != result.status
                || !a.untrusted
                || result.payload_until <= Utc::now()
        })
    {
        return Err(RuntimeError::InvalidResponse);
    }
    Ok(())
}
