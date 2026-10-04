// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fresh authorization is useful only for this live, nonce-bound result read.
//! Restarts/cancelled readers cannot turn old approvals into a reusable cache.
use crate::{
    ExpertiseOperation, ExpertiseReply, GatewayClient, GatewayServer, RuntimeError,
    gateway::expertise_error,
};
use axum::http::StatusCode;
use mem_federation::publication::InstanceId;
use mem_federation_store::{
    AddressedDelivery, AddressedReceipt, AnswerDeliveryAuthorization, AnswerDeliveryLease,
    DELIVERY_WAIT_SECONDS, QuestionResult,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::oneshot;
use uuid::Uuid;

type Delivery = std::result::Result<QuestionResult, StatusCode>;
struct Waiter {
    requester: InstanceId,
    responder: InstanceId,
    request_id: Uuid,
    sender: oneshot::Sender<Delivery>,
}
#[derive(Clone, Default)]
pub(crate) struct DeliveryWaiters(Arc<Mutex<BTreeMap<Uuid, Waiter>>>);
struct Waiting {
    waiters: DeliveryWaiters,
    nonce: Uuid,
}
impl Drop for Waiting {
    fn drop(&mut self) {
        if let Ok(mut waiters) = self.waiters.0.lock() {
            waiters.remove(&self.nonce);
        }
    }
}
impl GatewayServer {
    pub(crate) async fn receive_answer(
        &self,
        requester: InstanceId,
        request_id: Uuid,
        nonce: Uuid,
    ) -> Delivery {
        let metadata = self
            .store
            .question_result(self.fence, requester, request_id)
            .await
            .map_err(expertise_error)?;
        let (sender, receiver) = oneshot::channel();
        {
            let mut waiters = self
                .delivery_waiters
                .0
                .lock()
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            if waiters.contains_key(&nonce) {
                return Err(StatusCode::CONFLICT);
            }
            if waiters
                .values()
                .filter(|w| w.requester == requester)
                .count()
                >= 32
            {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            waiters.insert(
                nonce,
                Waiter {
                    requester,
                    responder: metadata.binding.capability.publisher,
                    request_id,
                    sender,
                },
            );
        }
        let _guard = Waiting {
            waiters: self.delivery_waiters.clone(),
            nonce,
        };
        if !self
            .store
            .request_answer_delivery(self.fence, requester, request_id, nonce)
            .await
            .map_err(expertise_error)?
        {
            return Ok(metadata);
        }
        // No background task owns the private reply. On timeout/cancellation,
        // both the receiver and its registration are dropped, discarding bytes.
        tokio::time::timeout(Duration::from_secs(DELIVERY_WAIT_SECONDS), receiver)
            .await
            .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    }
    pub(crate) async fn release_answer(
        &self,
        responder: InstanceId,
        lease: &AnswerDeliveryAuthorization,
    ) -> std::result::Result<(), StatusCode> {
        let waiter = {
            let mut waiters = self
                .delivery_waiters
                .0
                .lock()
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            let waiter = waiters.get(&lease.nonce).ok_or(StatusCode::CONFLICT)?;
            if responder != waiter.responder
                || lease.responder != responder
                || lease.requester != waiter.requester
                || lease.request_id != waiter.request_id
                || waiter.sender.is_closed()
            {
                return Err(StatusCode::FORBIDDEN);
            }
            waiters.remove(&lease.nonce).ok_or(StatusCode::CONFLICT)?
        };
        let delivery = self
            .store
            .finish_answer_delivery(self.fence, responder, lease)
            .await
            .map_err(expertise_error)
            .and_then(|answer| answer.ok_or(StatusCode::FORBIDDEN));
        let status = delivery.as_ref().err().copied();
        // The commit is the single release-authorization point. An authorized
        // in-flight transfer cannot be recalled. Failed sends are not retried;
        // even an exact question retry needs a new live origin check.
        let _ = waiter.sender.send(delivery);
        if let Some(status) = status {
            return Err(status);
        }
        Ok(())
    }
}

impl GatewayClient {
    /// Caller resolves and locks CURRENT service permissions in `tx` before
    /// entering here. Keep that same transaction alive until this returns.
    /// No model call, local search, source mutation or extra payload is sent.
    pub async fn authorize_held_delivery(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        access: &mem_federation::publication::LocalProjectAccess,
        responder: InstanceId,
        lease: &AnswerDeliveryLease,
    ) -> crate::Result<()> {
        if responder != lease.question.capability.publisher || lease.question.space != self.space {
            return Err(RuntimeError::Unauthorized);
        }
        let selection = mem_federation_store::eligible_answer_collection(
            tx,
            access,
            responder,
            &lease.question,
        )
        .await;
        let allowed = match selection {
            Ok((snapshot, ad)) => {
                crate::validate_expertise_delivery(&lease.question, &ad, &snapshot, &lease.answer)
                    .is_ok()
            }
            Err(_) => false,
        };
        match self
            .expertise(
                responder,
                ExpertiseOperation::AuthorizeDelivery {
                    authorization: lease.authorization(allowed)?,
                },
            )
            .await?
        {
            ExpertiseReply::Completed if allowed => Ok(()),
            _ => Err(RuntimeError::Unauthorized),
        }
    }
}

impl crate::InstanceSync {
    pub async fn submit_addressed_expertise(
        &self,
        record: mem_federation::expertise::AddressedExpertiseRecord,
    ) -> crate::Result<AddressedReceipt> {
        if record.sender != self.instance {
            return Err(RuntimeError::Unauthorized);
        }
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::AddressedSubmit { record },
            )
            .await?
        {
            ExpertiseReply::AddressedReceipt(receipt) => Ok(receipt),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }

    pub async fn addressed_expertise_result(
        &self,
        request_id: Uuid,
        kind: &str,
    ) -> crate::Result<AddressedDelivery> {
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::AddressedRead {
                    request_id,
                    kind: kind.to_owned(),
                },
            )
            .await?
        {
            ExpertiseReply::AddressedDelivery(delivery) => Ok(*delivery),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }

    pub async fn addressed_expertise_inbox(
        &self,
        limit: u32,
    ) -> crate::Result<Vec<AddressedDelivery>> {
        match self
            .client
            .expertise(self.instance, ExpertiseOperation::AddressedInbox { limit })
            .await?
        {
            ExpertiseReply::AddressedInbox(items) => Ok(items),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn expertise_capabilities(
        &self,
        after: Option<mem_federation::publication::RecordKey>,
        limit: u32,
    ) -> crate::Result<Vec<mem_federation::expertise::CapabilityAdvertisement>> {
        match self
            .client
            .expertise(self.instance, ExpertiseOperation::Discover { after, limit })
            .await?
        {
            ExpertiseReply::Capabilities(ads) => Ok(ads),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    /// Result bytes still require a fresh live origin delivery authorization.
    /// Caller holds current requester policy/consent locks through this read.
    pub async fn expertise_result(
        &self,
        q: &mem_federation::expertise::ExpertiseQuestion,
    ) -> crate::Result<mem_federation_store::QuestionResult> {
        if q.requester != self.instance || q.space != self.client.space {
            return Err(RuntimeError::Unauthorized);
        }
        let digest = q.digest().map_err(|_| RuntimeError::InvalidResponse)?;
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::Result {
                    request_id: q.request_id,
                },
            )
            .await?
        {
            ExpertiseReply::Question(result)
                if result.request_digest == digest
                    && result.binding == mem_federation_store::QuestionBinding::from(q) =>
            {
                Ok(*result)
            }
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    /// Trusted local workflow only: commit exact consent before calling.
    pub async fn submit_expertise_question(
        &self,
        question: mem_federation::expertise::ExpertiseQuestion,
    ) -> crate::Result<mem_federation_store::QuestionResult> {
        if question.requester != self.instance {
            return Err(RuntimeError::Unauthorized);
        }
        match self
            .client
            .expertise(self.instance, ExpertiseOperation::Submit { question })
            .await?
        {
            ExpertiseReply::Question(result) if result.answer.is_none() => Ok(*result),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn renew_expertise_question(
        &self,
        lease: &mem_federation_store::QuestionLease,
    ) -> crate::Result<()> {
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::Renew {
                    lease: lease.clone(),
                },
            )
            .await?
        {
            ExpertiseReply::Renewed(_) => Ok(()),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn claim_expertise_question(
        &self,
    ) -> crate::Result<Option<mem_federation_store::QuestionLease>> {
        match self
            .client
            .expertise(self.instance, ExpertiseOperation::Claim)
            .await?
        {
            ExpertiseReply::Lease(lease) => Ok(lease.map(|l| *l)),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    /// Recheck exact evidence under current caller-held authority locks.
    /// The private result MUST commit before this transfer is attempted.
    pub async fn complete_expertise_question(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        access: &mem_federation::publication::LocalProjectAccess,
        lease: &mem_federation_store::QuestionLease,
        answer: &mem_federation::expertise::ExpertiseAnswer,
    ) -> crate::Result<()> {
        let (snapshot, ad) = mem_federation_store::eligible_question_collection(
            tx,
            access,
            self.instance,
            &lease.question,
        )
        .await?;
        crate::validate_expertise_delivery(&lease.question, &ad, &snapshot, answer)
            .map_err(|_| RuntimeError::Unauthorized)?;
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::Complete {
                    lease: lease.clone(),
                    answer: answer.clone(),
                },
            )
            .await?
        {
            ExpertiseReply::Completed => Ok(()),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn reserve_expertise_model(
        &self,
        lease: &mem_federation_store::QuestionLease,
        daily_calls: u32,
        daily_tokens: u32,
        tokens: u32,
    ) -> crate::Result<()> {
        match self
            .client
            .expertise(
                self.instance,
                ExpertiseOperation::ReserveModel {
                    lease: lease.clone(),
                    daily_calls,
                    daily_tokens,
                    tokens,
                },
            )
            .await?
        {
            ExpertiseReply::Completed => Ok(()),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn claim_expertise_delivery(&self) -> crate::Result<Option<AnswerDeliveryLease>> {
        match self
            .client
            .expertise(self.instance, ExpertiseOperation::ClaimDelivery)
            .await?
        {
            ExpertiseReply::DeliveryLease(lease) => Ok(lease.map(|l| *l)),
            _ => Err(RuntimeError::InvalidResponse),
        }
    }
    pub async fn authorize_expertise_delivery(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        access: &mem_federation::publication::LocalProjectAccess,
        lease: &AnswerDeliveryLease,
    ) -> crate::Result<()> {
        self.client
            .authorize_held_delivery(tx, access, self.instance, lease)
            .await
    }
}
