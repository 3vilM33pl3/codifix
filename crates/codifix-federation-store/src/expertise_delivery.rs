// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded per-read authorization checks. The runtime additionally requires a
//! live addressed waiter; the origin must revalidate and lock its local sources.
use crate::{
    GatewayStore, QuestionResult, Result, StoreError, WriterFence,
    expertise_mailbox::{journal, now, result},
};
use chrono::{DateTime, Utc};
use codifix_federation::{CanonicalJsonCodec, ContentDigest, expertise::*, publication::*};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

pub const DELIVERY_WAIT_SECONDS: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerDeliveryLease {
    pub nonce: Uuid,
    pub question: ExpertiseQuestion,
    pub answer: ExpertiseAnswer,
    pub answer_digest: ContentDigest,
    pub policy_digest: ContentDigest,
    pub lease_token: Uuid,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerDeliveryAuthorization {
    pub nonce: Uuid,
    pub space: SpaceId,
    pub requester: InstanceId,
    pub responder: InstanceId,
    pub request_id: Uuid,
    pub request_digest: ContentDigest,
    pub answer_digest: ContentDigest,
    pub policy_digest: ContentDigest,
    pub lease_token: Uuid,
    pub expires_at: DateTime<Utc>,
    pub allow: bool,
}
impl AnswerDeliveryLease {
    /// Approval contains no question text, source content, or answer payload.
    pub fn authorization(&self, allow: bool) -> Result<AnswerDeliveryAuthorization> {
        Ok(AnswerDeliveryAuthorization {
            nonce: self.nonce,
            space: self.question.space,
            requester: self.question.requester,
            responder: self.question.capability.publisher,
            request_id: self.question.request_id,
            request_digest: self.question.digest()?,
            answer_digest: self.answer_digest.clone(),
            policy_digest: self.policy_digest.clone(),
            lease_token: self.lease_token,
            expires_at: self.expires_at,
            allow,
        })
    }
}
fn digest(answer: &ExpertiseAnswer) -> Result<ContentDigest> {
    CanonicalJsonCodec::digest(answer).map_err(|_| StoreError::Binding)
}
impl GatewayStore {
    /// Returns false for metadata-only status/expired payload. A nonce is used
    /// once even after a failure; a new read requires a new authorization check.
    pub async fn request_answer_delivery(
        &self,
        fence: WriterFence,
        requester: InstanceId,
        request_id: Uuid,
        nonce: Uuid,
    ) -> Result<bool> {
        if nonce.is_nil() {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row = sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3 AND payload_until>clock_timestamp() AND question IS NOT NULL AND answer IS NOT NULL FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(requester.as_uuid()).bind(request_id).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
        let answer = row.try_get::<Json<ExpertiseAnswer>, _>("answer")?.0;
        let policy = self
            .question_policy(&mut tx, fence, &q, q.created_at)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_deliveries WHERE space=$1 AND requester=$2 AND status IN ('pending','claimed') AND expires_at>clock_timestamp()")
            .bind(fence.space.as_uuid()).bind(requester.as_uuid()).fetch_one(&mut *tx).await?;
        if count >= 32 {
            return Err(StoreError::Limits);
        }
        let expires = (now(&mut tx).await?
            + chrono::Duration::seconds(DELIVERY_WAIT_SECONDS as i64))
        .min(row.try_get("payload_until")?);
        let n = sqlx::query("INSERT INTO federation_expertise_deliveries(space,nonce,requester,responder,request_id,request_digest,answer_digest,policy_digest,status,expires_at,metadata_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9,$10) ON CONFLICT DO NOTHING")
            .bind(fence.space.as_uuid()).bind(nonce).bind(requester.as_uuid()).bind(q.capability.publisher.as_uuid()).bind(request_id)
            .bind(Json(q.digest()?)).bind(Json(digest(&answer)?)).bind(Json(policy)).bind(expires).bind(row.try_get::<DateTime<Utc>,_>("metadata_until")?)
            .execute(&mut *tx).await?.rows_affected();
        if n != 1 {
            return Err(StoreError::Binding);
        }
        journal(&mut tx, &q, "delivery_requested").await?;
        tx.commit().await?;
        Ok(true)
    }
    pub async fn claim_answer_delivery(
        &self,
        fence: WriterFence,
        responder: InstanceId,
    ) -> Result<Option<AnswerDeliveryLease>> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(
            responder,
            codifix_federation::SpaceCapability::AnswerExpertise,
        ) {
            return Err(StoreError::Binding);
        }
        sqlx::query("UPDATE federation_expertise_deliveries SET status='expired',lease_token=NULL WHERE space=$1 AND responder=$2 AND status IN ('pending','claimed') AND expires_at<=clock_timestamp()")
            .bind(fence.space.as_uuid()).bind(responder.as_uuid()).execute(&mut *tx).await?;
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_expertise_deliveries WHERE space=$1 AND responder=$2 AND status='claimed')")
            .bind(fence.space.as_uuid()).bind(responder.as_uuid()).fetch_one(&mut *tx).await?;
        if active {
            tx.commit().await?;
            return Ok(None);
        }
        let rows = sqlx::query("SELECT * FROM federation_expertise_deliveries WHERE space=$1 AND responder=$2 AND status='pending' ORDER BY created_at,nonce LIMIT 32 FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(responder.as_uuid()).fetch_all(&mut *tx).await?;
        for row in rows {
            let nonce: Uuid = row.try_get("nonce")?;
            let payload = self.delivery_payload(&mut tx, fence, &row).await;
            let (q, answer) = match payload {
                Ok(v) => v,
                Err(StoreError::Binding | StoreError::Expertise(_)) => {
                    sqlx::query("UPDATE federation_expertise_deliveries SET status='denied' WHERE space=$1 AND nonce=$2")
                        .bind(fence.space.as_uuid()).bind(nonce).execute(&mut *tx).await?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let lease = AnswerDeliveryLease {
                nonce,
                question: q,
                answer,
                answer_digest: row.try_get::<Json<ContentDigest>, _>("answer_digest")?.0,
                policy_digest: row.try_get::<Json<ContentDigest>, _>("policy_digest")?.0,
                lease_token: Uuid::new_v4(),
                expires_at: row.try_get("expires_at")?,
            };
            sqlx::query("UPDATE federation_expertise_deliveries SET status='claimed',lease_token=$3 WHERE space=$1 AND nonce=$2")
                .bind(fence.space.as_uuid()).bind(nonce).bind(lease.lease_token).execute(&mut *tx).await?;
            journal(&mut tx, &lease.question, "delivery_claimed").await?;
            tx.commit().await?;
            return Ok(Some(lease));
        }
        tx.commit().await?;
        Ok(None)
    }
    async fn delivery_payload(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
        challenge: &sqlx::postgres::PgRow,
    ) -> Result<(ExpertiseQuestion, ExpertiseAnswer)> {
        let row = sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3 AND responder=$4 AND question IS NOT NULL AND answer IS NOT NULL AND payload_until>clock_timestamp() FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(challenge.try_get::<Uuid,_>("requester")?).bind(challenge.try_get::<Uuid,_>("request_id")?).bind(challenge.try_get::<Uuid,_>("responder")?)
            .fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
        let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
        let answer = row.try_get::<Json<ExpertiseAnswer>, _>("answer")?.0;
        if q.digest()?
            != challenge
                .try_get::<Json<ContentDigest>, _>("request_digest")?
                .0
            || digest(&answer)?
                != challenge
                    .try_get::<Json<ContentDigest>, _>("answer_digest")?
                    .0
            || self.question_policy(tx, fence, &q, q.created_at).await?
                != challenge
                    .try_get::<Json<ContentDigest>, _>("policy_digest")?
                    .0
        {
            return Err(StoreError::Binding);
        }
        Ok((q, answer))
    }
    /// Single-use release. Runtime calls this ONLY for a still-live addressed
    /// waiter and delivers directly, never stores a reusable approval.
    pub async fn finish_answer_delivery(
        &self,
        fence: WriterFence,
        responder: InstanceId,
        lease: &AnswerDeliveryAuthorization,
    ) -> Result<Option<QuestionResult>> {
        let allow = lease.allow;
        if lease.space != fence.space || lease.responder != responder {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row = sqlx::query("SELECT * FROM federation_expertise_deliveries WHERE space=$1 AND nonce=$2 AND responder=$3 AND requester=$4 AND request_id=$5 AND status='claimed' AND lease_token=$6 AND expires_at>clock_timestamp() FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(lease.nonce).bind(responder.as_uuid()).bind(lease.requester.as_uuid()).bind(lease.request_id).bind(lease.lease_token)
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::LostLease)?;
        if row.try_get::<Json<ContentDigest>, _>("answer_digest")?.0 != lease.answer_digest
            || row.try_get::<Json<ContentDigest>, _>("policy_digest")?.0 != lease.policy_digest
            || row.try_get::<DateTime<Utc>, _>("expires_at")? != lease.expires_at
        {
            return Err(StoreError::Binding);
        }
        let (q, _) = self.delivery_payload(&mut tx, fence, &row).await?;
        if q.digest()? != lease.request_digest {
            return Err(StoreError::Binding);
        }
        let payload = if allow {
            let row = sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3")
                .bind(fence.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).fetch_one(&mut *tx).await?;
            Some(result(&row)?)
        } else {
            None
        };
        sqlx::query("UPDATE federation_expertise_deliveries SET status=$3,lease_token=NULL WHERE space=$1 AND nonce=$2")
            .bind(fence.space.as_uuid()).bind(lease.nonce).bind(if allow {"released"} else {"denied"}).execute(&mut *tx).await?;
        journal(
            &mut tx,
            &q,
            if allow {
                "delivery_released"
            } else {
                "delivery_denied"
            },
        )
        .await?;
        tx.commit().await?;
        Ok(payload)
    }
}
