// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private gateway protocol. Separate signature purposes and fresh nonces bind
//! each reply to its addressed operation; these receipts are not publications.
use crate::{GatewayClient, Result, RuntimeError};
use chrono::{DateTime, Utc};
use codifix_federation::{CanonicalJsonCodec, ContentDigest, expertise::*, publication::*};
use codifix_federation_store::{
    AddressedDelivery, AddressedQuestionLease, AddressedReceipt, AddressedStatus,
    AnswerDeliveryAuthorization, AnswerDeliveryLease, QuestionLease, QuestionResult,
};
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
    AddressedSubmit {
        record: AddressedExpertiseRecord,
        answer_claim: Option<Uuid>,
    },
    AddressedClaim,
    AddressedRead {
        request_id: Uuid,
        kind: String,
    },
    AddressedInbox {
        limit: u32,
    },
    Result {
        request_id: Uuid,
    },
    Claim,
    ClaimDelivery,
    AuthorizeDelivery {
        authorization: AnswerDeliveryAuthorization,
    },
    Renew {
        lease: QuestionLease,
    },
    ReserveModel {
        lease: QuestionLease,
        daily_calls: u32,
        daily_tokens: u32,
        tokens: u32,
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
            Self::AddressedSubmit { .. } => "memory-addressed-submit-v1",
            Self::AddressedClaim => "memory-addressed-claim-v1",
            Self::AddressedRead { .. } => "memory-addressed-read-v1",
            Self::AddressedInbox { .. } => "memory-addressed-inbox-v1",
            Self::Result { .. } => "memory-expertise-result-v1",
            Self::Claim => "memory-expertise-claim-v1",
            Self::ClaimDelivery => "memory-expertise-delivery-claim-v1",
            Self::AuthorizeDelivery { .. } => "memory-expertise-delivery-release-v1",
            Self::Renew { .. } => "memory-expertise-renew-v1",
            Self::ReserveModel { .. } => "memory-expertise-model-reservation-v1",
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
    AddressedReceipt(AddressedReceipt),
    AddressedLease(Option<Box<AddressedQuestionLease>>),
    AddressedDelivery(Option<Box<AddressedDelivery>>),
    AddressedInbox(Vec<AddressedDelivery>),
    Lease(Option<Box<QuestionLease>>),
    DeliveryLease(Option<Box<AnswerDeliveryLease>>),
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
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(RuntimeError::Store(
                codifix_federation_store::StoreError::Limits,
            ));
        }
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
            (
                ExpertiseOperation::AddressedSubmit { record, .. },
                ExpertiseReply::AddressedReceipt(receipt),
            ) => {
                if record.space != self.space
                    || record.sender != instance
                    || record.request_id != receipt.request_id
                    || record.space != receipt.space
                    || CanonicalJsonCodec::digest(record)
                        .map_err(|_| RuntimeError::InvalidResponse)?
                        != receipt.record_digest
                {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (ExpertiseOperation::AddressedClaim, ExpertiseReply::AddressedLease(lease)) => {
                if lease.as_ref().is_some_and(|lease| {
                    lease.lease_token.is_nil()
                        || lease.lease_until <= Utc::now()
                        || lease.record.recipient != instance
                        || lease.record.space != self.space
                        || !matches!(lease.record.payload, AddressedExpertisePayload::Question(_))
                        || lease.record.validate().is_err()
                }) {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (
                ExpertiseOperation::AddressedRead { request_id, kind },
                ExpertiseReply::AddressedDelivery(delivery),
            ) => {
                if let Some(delivery) = delivery
                    && (delivery.receipt.request_id != *request_id
                        || delivery.receipt.kind != *kind
                        || delivery.receipt.space != self.space
                        || !valid_addressed_delivery(delivery, instance)?)
                {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (
                ExpertiseOperation::AddressedInbox { limit },
                ExpertiseReply::AddressedInbox(items),
            ) => {
                if !(1..=32).contains(limit)
                    || items.len() > *limit as usize
                    || items.iter().any(|item| {
                        item.receipt.kind != "question"
                            || !matches!(valid_addressed_delivery(item, instance), Ok(true))
                    })
                {
                    return Err(RuntimeError::InvalidResponse);
                }
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
            (ExpertiseOperation::ClaimDelivery, ExpertiseReply::DeliveryLease(lease)) => {
                if let Some(lease) = lease
                    && (lease.question.capability.publisher != instance
                        || lease.question.space != self.space
                        || lease.nonce.is_nil()
                        || lease.lease_token.is_nil()
                        || lease.expires_at <= Utc::now()
                        || lease.answer.request_digest
                            != lease
                                .question
                                .digest()
                                .map_err(|_| RuntimeError::InvalidResponse)?
                        || lease.answer_digest
                            != CanonicalJsonCodec::digest(&lease.answer)
                                .map_err(|_| RuntimeError::InvalidResponse)?)
                {
                    return Err(RuntimeError::InvalidResponse);
                }
            }
            (ExpertiseOperation::Renew { lease }, ExpertiseReply::Renewed(until))
                if *until > Utc::now() && *until <= lease.question.expires_at => {}
            (
                ExpertiseOperation::Complete { .. }
                | ExpertiseOperation::AuthorizeDelivery { .. }
                | ExpertiseOperation::ReserveModel { .. },
                ExpertiseReply::Completed,
            ) => {}
            _ => return Err(RuntimeError::InvalidResponse),
        }
        Ok(reply)
    }
}
fn valid_addressed_delivery(delivery: &AddressedDelivery, actor: InstanceId) -> Result<bool> {
    let Some(record) = &delivery.record else {
        return Ok(true);
    };
    let digest = CanonicalJsonCodec::digest(record).map_err(|_| RuntimeError::InvalidResponse)?;
    Ok(delivery.receipt.status == AddressedStatus::Published
        && delivery.receipt.proof.is_some()
        && delivery.receipt.record_digest == digest
        && record.space == delivery.receipt.space
        && record.request_id == delivery.receipt.request_id
        && (record.sender == actor || record.recipient == actor)
        && record.validate().is_ok())
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
