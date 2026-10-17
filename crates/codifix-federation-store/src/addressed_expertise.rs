// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gateway-only durable intents for addressed ATProto expertise records. This
//! table is separate from the public-to-space publication prefix and mailbox.
use crate::{GatewayStore, PublishFailure, Result, StoreError, WriterFence};
use chrono::{DateTime, Utc};
use codifix_federation::{
    CanonicalJsonCodec, ContentDigest, SpaceCapability,
    expertise::{AddressedExpertisePayload, AddressedExpertiseRecord},
    publication::{InstanceId, SpaceId},
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressedStatus {
    Queued,
    Published,
    Failed,
    AuthBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressedReceipt {
    pub space: SpaceId,
    pub request_id: Uuid,
    pub kind: String,
    pub status: AddressedStatus,
    pub record_digest: ContentDigest,
    pub proof: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressedDelivery {
    pub receipt: AddressedReceipt,
    /// Absent while queued, after expiry, or after payload retention ends.
    pub record: Option<AddressedExpertiseRecord>,
}

#[derive(Debug, Clone)]
pub struct AddressedLease {
    pub record: AddressedExpertiseRecord,
    pub lease_token: Uuid,
    pub attempts: u16,
}
#[derive(Debug, Clone)]
pub struct AddressedDeleteLease {
    pub record: AddressedExpertiseRecord,
    pub prior_proof: Option<serde_json::Value>,
    pub lease_token: Uuid,
    pub attempts: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressedQuestionLease {
    pub record: AddressedExpertiseRecord,
    pub lease_token: Uuid,
    pub lease_until: DateTime<Utc>,
}

fn kind(payload: &AddressedExpertisePayload) -> &'static str {
    match payload {
        AddressedExpertisePayload::Question(_) => "question",
        AddressedExpertisePayload::Answer(_) => "answer",
    }
}

fn receipt(row: &sqlx::postgres::PgRow) -> Result<AddressedReceipt> {
    let status: String = row.try_get("status")?;
    Ok(AddressedReceipt {
        space: SpaceId::new(row.try_get("space")?)?,
        request_id: row.try_get("request_id")?,
        kind: row.try_get("kind")?,
        status: serde_json::from_value(serde_json::Value::String(status))?,
        record_digest: row.try_get::<Json<ContentDigest>, _>("record_digest")?.0,
        proof: row
            .try_get::<Option<Json<serde_json::Value>>, _>("proof")?
            .map(|p| p.0),
    })
}

impl GatewayStore {
    /// Exact retry is harmless; changed bytes or metadata under the same
    /// request/kind are equivocation. Current space policy is checked before
    /// creating an intent, not inferred from an AT repository record.
    pub async fn enqueue_addressed(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        record: &AddressedExpertiseRecord,
        answer_claim: Option<Uuid>,
    ) -> Result<AddressedReceipt> {
        record.validate()?;
        if record.space != fence.space || authenticated != record.sender {
            return Err(StoreError::Binding);
        }
        let digest = CanonicalJsonCodec::digest(record).map_err(|_| StoreError::Binding)?;
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        let at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let prior = sqlx::query("SELECT * FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind=$3 FOR UPDATE")
            .bind(record.space.as_uuid()).bind(record.request_id).bind(kind(&record.payload))
            .fetch_optional(&mut *tx).await?;
        if let Some(row) = prior {
            if row.try_get::<Json<ContentDigest>, _>("record_digest")?.0 != digest {
                return Err(StoreError::Binding);
            }
            let result = receipt(&row)?;
            tx.commit().await?;
            return Ok(result);
        }
        if at >= record.expires_at {
            return Err(StoreError::Binding);
        }
        match &record.payload {
            AddressedExpertisePayload::Question(question) => {
                if answer_claim.is_some() {
                    return Err(StoreError::Binding);
                }
                state.authorize_expertise(authenticated, question, at)?;
                self.question_policy(&mut tx, fence, question, at).await?;
                let outstanding: i64 = sqlx::query_scalar("SELECT count(*) FROM federation_expertise_addressed q WHERE q.space=$1 AND q.sender=$2 AND q.kind='question' AND q.expires_at>clock_timestamp() AND NOT EXISTS (SELECT 1 FROM federation_expertise_addressed a WHERE a.space=q.space AND a.request_id=q.request_id AND a.kind='answer')")
                    .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).fetch_one(&mut *tx).await?;
                if outstanding >= 32 {
                    return Err(StoreError::Limits);
                }
            }
            AddressedExpertisePayload::Answer(answer) => {
                let claim = answer_claim.ok_or(StoreError::Binding)?;
                if !state.has_capability(authenticated, SpaceCapability::AnswerExpertise)
                    || !state.has_capability(record.recipient, SpaceCapability::AskExpertise)
                {
                    return Err(StoreError::Binding);
                }
                let question_row = sqlx::query("SELECT record,status,answer_lease_token,answer_lease_until FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question' FOR UPDATE")
                    .bind(record.space.as_uuid()).bind(record.request_id)
                    .fetch_optional(&mut *tx).await?.ok_or(StoreError::Binding)?;
                if question_row.try_get::<String, _>("status")? != "published"
                    || question_row.try_get::<Option<Uuid>, _>("answer_lease_token")? != Some(claim)
                    || question_row
                        .try_get::<Option<DateTime<Utc>>, _>("answer_lease_until")?
                        .is_none_or(|until| until <= at)
                {
                    return Err(StoreError::Binding);
                }
                let question_record = question_row
                    .try_get::<Option<Json<AddressedExpertiseRecord>>, _>("record")?
                    .ok_or(StoreError::Binding)?
                    .0;
                let AddressedExpertisePayload::Question(question) = question_record.payload else {
                    return Err(StoreError::Corrupt);
                };
                if question.requester != record.recipient
                    || question.capability.publisher != authenticated
                    || question.digest()? != answer.request_digest
                    || question_record.policy_digest != record.policy_digest
                    || question_record.policy_revision != record.policy_revision
                {
                    return Err(StoreError::Binding);
                }
                self.question_policy(&mut tx, fence, &question, at).await?;
            }
        }
        let row = sqlx::query("INSERT INTO federation_expertise_addressed(space,request_id,kind,sender,recipient,record_digest,record,status,created_at,expires_at,payload_until,metadata_until) VALUES($1,$2,$3,$4,$5,$6,$7,'queued',$8,$9,$10,$11) RETURNING *")
            .bind(record.space.as_uuid()).bind(record.request_id).bind(kind(&record.payload))
            .bind(record.sender.as_uuid()).bind(record.recipient.as_uuid())
            .bind(Json(&digest)).bind(Json(record)).bind(record.created_at).bind(record.expires_at)
            .bind(record.created_at+chrono::Duration::hours(24))
            .bind(record.created_at+chrono::Duration::days(30))
            .fetch_one(&mut *tx).await?;
        let result = receipt(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    /// One responder at a time may prepare the deterministic answer. A crashed
    /// responder relinquishes the lease on expiry; accepted answers are never
    /// overwritten and disappear from claim eligibility immediately.
    pub async fn claim_addressed_question(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
    ) -> Result<Option<AddressedQuestionLease>> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(authenticated, SpaceCapability::AnswerExpertise) {
            return Err(StoreError::Binding);
        }
        let rows = sqlx::query("SELECT q.* FROM federation_expertise_addressed q WHERE q.space=$1 AND q.recipient=$2 AND q.kind='question' AND q.status='published' AND q.expires_at>clock_timestamp() AND q.payload_until>clock_timestamp() AND (q.answer_lease_until IS NULL OR q.answer_lease_until<=clock_timestamp()) AND NOT EXISTS (SELECT 1 FROM federation_expertise_addressed a WHERE a.space=q.space AND a.request_id=q.request_id AND a.kind='answer') ORDER BY q.created_at,q.request_id LIMIT 32 FOR UPDATE OF q SKIP LOCKED")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).fetch_all(&mut *tx).await?;
        for row in rows {
            let Some(record) = row
                .try_get::<Option<Json<AddressedExpertiseRecord>>, _>("record")?
                .map(|r| r.0)
            else {
                continue;
            };
            let AddressedExpertisePayload::Question(q) = &record.payload else {
                return Err(StoreError::Corrupt);
            };
            match self.question_policy(&mut tx, fence, q, q.created_at).await {
                Ok(_) => {}
                Err(StoreError::Binding | StoreError::Expertise(_) | StoreError::Outcome(_)) => {
                    continue;
                }
                Err(other) => return Err(other),
            }
            let token = Uuid::new_v4();
            let until = Utc::now() + chrono::Duration::seconds(30);
            sqlx::query("UPDATE federation_expertise_addressed SET answer_lease_token=$3,answer_lease_until=$4 WHERE space=$1 AND request_id=$2 AND kind='question'")
                .bind(fence.space.as_uuid()).bind(record.request_id).bind(token).bind(until)
                .execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Some(AddressedQuestionLease {
                record,
                lease_token: token,
                lease_until: until,
            }));
        }
        tx.commit().await?;
        Ok(None)
    }

    pub async fn claim_addressed(&self, fence: WriterFence) -> Result<Option<AddressedLease>> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        sqlx::query("UPDATE federation_expertise_addressed SET lease_token=NULL,lease_until=NULL,status=CASE WHEN attempts>=5 THEN 'failed' ELSE status END WHERE space=$1 AND status='queued' AND lease_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("UPDATE federation_expertise_addressed SET status='failed',last_error='expired' WHERE space=$1 AND status IN ('queued','auth_blocked') AND expires_at<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT * FROM federation_expertise_addressed WHERE space=$1 AND status='queued' AND lease_token IS NULL AND next_attempt_at<=clock_timestamp() AND expires_at>clock_timestamp() ORDER BY created_at,request_id,kind LIMIT 1 FOR UPDATE")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let token = Uuid::new_v4();
        let attempts: i16 = row.try_get("attempts")?;
        let record = row
            .try_get::<Option<Json<AddressedExpertiseRecord>>, _>("record")?
            .ok_or(StoreError::Corrupt)?
            .0;
        let question = match &record.payload {
            AddressedExpertisePayload::Question(q) => q.clone(),
            AddressedExpertisePayload::Answer(_) => {
                let q: Option<Json<AddressedExpertiseRecord>> = sqlx::query_scalar("SELECT record FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question'")
                    .bind(fence.space.as_uuid()).bind(record.request_id).fetch_optional(&mut *tx).await?.flatten();
                let q = q.ok_or(StoreError::Corrupt)?.0;
                let AddressedExpertisePayload::Question(q) = q.payload else {
                    return Err(StoreError::Corrupt);
                };
                q
            }
        };
        let membership = state.has_capability(
            record.sender,
            if kind(&record.payload) == "question" {
                SpaceCapability::AskExpertise
            } else {
                SpaceCapability::AnswerExpertise
            },
        ) && state.has_capability(
            record.recipient,
            if kind(&record.payload) == "question" {
                SpaceCapability::AnswerExpertise
            } else {
                SpaceCapability::AskExpertise
            },
        );
        let authorized = if membership {
            match self
                .question_policy(&mut tx, fence, &question, question.created_at)
                .await
            {
                Ok(_) => true,
                Err(StoreError::Binding | StoreError::Expertise(_) | StoreError::Outcome(_)) => {
                    false
                }
                Err(other) => return Err(other),
            }
        } else {
            false
        };
        if !authorized {
            sqlx::query("UPDATE federation_expertise_addressed SET status='failed',last_error='policy_denied' WHERE space=$1 AND request_id=$2 AND kind=$3")
                .bind(fence.space.as_uuid()).bind(record.request_id).bind(kind(&record.payload))
                .execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(None);
        }
        sqlx::query("UPDATE federation_expertise_addressed SET lease_token=$4,lease_until=clock_timestamp()+interval '120 seconds',attempts=attempts+1 WHERE space=$1 AND request_id=$2 AND kind=$3")
            .bind(fence.space.as_uuid()).bind(record.request_id).bind(kind(&record.payload))
            .bind(token).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(AddressedLease {
            record,
            lease_token: token,
            attempts: u16::try_from(attempts + 1).map_err(|_| StoreError::Corrupt)?,
        }))
    }

    /// Current membership and collection policy are checked for every body
    /// read. The other space members cannot use the ordinary publication pull
    /// or this API to obtain the addressed payload.
    pub async fn read_addressed(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        request_id: Uuid,
        kind: &str,
    ) -> Result<Option<AddressedDelivery>> {
        if !matches!(kind, "question" | "answer") || request_id.is_nil() {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        let row = sqlx::query("SELECT * FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind=$3 AND metadata_until>clock_timestamp() FOR SHARE")
            .bind(fence.space.as_uuid()).bind(request_id).bind(kind)
            .fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let sender = InstanceId::new(row.try_get("sender")?)?;
        let recipient = InstanceId::new(row.try_get("recipient")?)?;
        if authenticated != sender && authenticated != recipient {
            return Err(StoreError::Binding);
        }
        if !state.has_capability(
            sender,
            if kind == "question" {
                SpaceCapability::AskExpertise
            } else {
                SpaceCapability::AnswerExpertise
            },
        ) || !state.has_capability(
            recipient,
            if kind == "question" {
                SpaceCapability::AnswerExpertise
            } else {
                SpaceCapability::AskExpertise
            },
        ) {
            return Err(StoreError::Binding);
        }
        let receipt = receipt(&row)?;
        let retained: bool = row.try_get::<DateTime<Utc>, _>("payload_until")? > Utc::now();
        let record = if receipt.status == AddressedStatus::Published && retained {
            row.try_get::<Option<Json<AddressedExpertiseRecord>>, _>("record")?
                .map(|r| r.0)
        } else {
            None
        };
        if let Some(record) = &record {
            let question = match &record.payload {
                AddressedExpertisePayload::Question(q) => q.clone(),
                AddressedExpertisePayload::Answer(_) => {
                    let q: Option<Json<AddressedExpertiseRecord>> = sqlx::query_scalar("SELECT record FROM federation_expertise_addressed WHERE space=$1 AND request_id=$2 AND kind='question'")
                        .bind(fence.space.as_uuid()).bind(request_id).fetch_optional(&mut *tx).await?.flatten();
                    let q = q.ok_or(StoreError::Corrupt)?.0;
                    let AddressedExpertisePayload::Question(q) = q.payload else {
                        return Err(StoreError::Corrupt);
                    };
                    q
                }
            };
            // The original deadline is not re-imposed for a completed record,
            // but all current policy and membership state is rechecked.
            self.question_policy(&mut tx, fence, &question, question.created_at)
                .await?;
        }
        tx.commit().await?;
        Ok(Some(AddressedDelivery { receipt, record }))
    }

    pub async fn addressed_inbox(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        limit: u32,
    ) -> Result<Vec<AddressedDelivery>> {
        if !(1..=32).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(authenticated, SpaceCapability::AnswerExpertise) {
            return Err(StoreError::Binding);
        }
        let rows = sqlx::query("SELECT q.* FROM federation_expertise_addressed q WHERE q.space=$1 AND q.recipient=$2 AND q.kind='question' AND q.status='published' AND q.expires_at>clock_timestamp() AND q.payload_until>clock_timestamp() AND NOT EXISTS (SELECT 1 FROM federation_expertise_addressed a WHERE a.space=q.space AND a.request_id=q.request_id AND a.kind='answer') ORDER BY q.created_at,q.request_id LIMIT $3")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).bind(i64::from(limit))
            .fetch_all(&mut *tx).await?;
        let mut deliveries = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(record) = row
                .try_get::<Option<Json<AddressedExpertiseRecord>>, _>("record")?
                .map(|r| r.0)
            else {
                continue;
            };
            let AddressedExpertisePayload::Question(q) = &record.payload else {
                return Err(StoreError::Corrupt);
            };
            match self.question_policy(&mut tx, fence, q, q.created_at).await {
                Ok(_) => deliveries.push(AddressedDelivery {
                    receipt: receipt(&row)?,
                    record: Some(record),
                }),
                Err(StoreError::Binding | StoreError::Expertise(_) | StoreError::Outcome(_)) => {}
                Err(other) => return Err(other),
            }
        }
        tx.commit().await?;
        Ok(deliveries)
    }

    pub async fn cleanup_addressed(&self, fence: WriterFence) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        if self.addressed_retention_hold(&mut tx, fence).await? {
            tx.commit().await?;
            return Ok(());
        }
        // A PDS failure keeps the payload available for an exact, fenced retry.
        sqlx::query("DELETE FROM federation_expertise_addressed WHERE space=$1 AND metadata_until<=clock_timestamp() AND record IS NULL")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn addressed_retention_hold(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        fence: WriterFence,
    ) -> Result<bool> {
        let latest: Option<String> = sqlx::query_scalar("SELECT action FROM federation_retention_decisions WHERE space=$1 AND action IN ('hold','release_hold') ORDER BY decision DESC LIMIT 1")
            .bind(fence.space.as_uuid()).fetch_optional(&mut **tx).await?;
        Ok(latest.as_deref() == Some("hold"))
    }

    pub async fn claim_addressed_delete(
        &self,
        fence: WriterFence,
    ) -> Result<Option<AddressedDeleteLease>> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        if self.addressed_retention_hold(&mut tx, fence).await? {
            tx.commit().await?;
            return Ok(None);
        }
        sqlx::query("UPDATE federation_expertise_addressed SET delete_lease_token=NULL,delete_lease_until=NULL,delete_status=CASE WHEN delete_attempts>=5 THEN 'failed' ELSE 'queued' END WHERE space=$1 AND delete_lease_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT record,proof,delete_attempts,request_id,kind FROM federation_expertise_addressed WHERE space=$1 AND record IS NOT NULL AND payload_until<=clock_timestamp() AND (delete_status IS NULL OR delete_status='queued') AND delete_lease_token IS NULL AND delete_next_attempt_at<=clock_timestamp() ORDER BY payload_until,request_id,kind LIMIT 1 FOR UPDATE SKIP LOCKED")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let token = Uuid::new_v4();
        sqlx::query("UPDATE federation_expertise_addressed SET delete_status='queued',delete_lease_token=$4,delete_lease_until=clock_timestamp()+interval '120 seconds',delete_attempts=delete_attempts+1 WHERE space=$1 AND request_id=$2 AND kind=$3")
            .bind(fence.space.as_uuid()).bind(row.try_get::<Uuid,_>("request_id")?)
            .bind(row.try_get::<String,_>("kind")?).bind(token).execute(&mut *tx).await?;
        let lease = AddressedDeleteLease {
            record: row
                .try_get::<Json<AddressedExpertiseRecord>, _>("record")?
                .0,
            prior_proof: row
                .try_get::<Option<Json<serde_json::Value>>, _>("proof")?
                .map(|p| p.0),
            lease_token: token,
            attempts: u16::try_from(row.try_get::<i16, _>("delete_attempts")? + 1)
                .map_err(|_| StoreError::Corrupt)?,
        };
        tx.commit().await?;
        Ok(Some(lease))
    }

    pub async fn acknowledge_addressed_delete(
        &self,
        fence: WriterFence,
        lease: &AddressedDeleteLease,
        deletion: serde_json::Value,
    ) -> Result<()> {
        if deletion
            .get("prior_record_cid")
            .and_then(serde_json::Value::as_str)
            .is_none()
        {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        if self.addressed_retention_hold(&mut tx, fence).await? {
            return Err(StoreError::Binding);
        }
        let updated = sqlx::query("UPDATE federation_expertise_addressed SET record=NULL,delete_status='verified',delete_proof=$5,delete_lease_token=NULL,delete_lease_until=NULL,delete_error=NULL WHERE space=$1 AND request_id=$2 AND kind=$3 AND delete_lease_token=$4 AND record_digest=$6")
            .bind(fence.space.as_uuid()).bind(lease.record.request_id).bind(kind(&lease.record.payload))
            .bind(lease.lease_token).bind(Json(deletion))
            .bind(Json(CanonicalJsonCodec::digest(&lease.record).map_err(|_|StoreError::Binding)?))
            .execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::LostLease);
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn fail_addressed_delete(
        &self,
        fence: WriterFence,
        lease: &AddressedDeleteLease,
        failure: PublishFailure,
    ) -> Result<()> {
        let auth_generation = match failure {
            PublishFailure::Authentication { generation } => {
                Some(i64::try_from(generation).map_err(|_| StoreError::Binding)?)
            }
            _ => None,
        };
        let terminal = lease.attempts >= 5
            || matches!(
                failure,
                PublishFailure::Conflict
                    | PublishFailure::InvalidReceipt
                    | PublishFailure::AuthorizationDenied
            );
        let reason = match failure {
            PublishFailure::Unavailable => "unavailable",
            PublishFailure::Conflict => "conflict",
            PublishFailure::InvalidReceipt => "invalid_receipt",
            PublishFailure::AuthorizationDenied => "authorization_denied",
            PublishFailure::Authentication { .. } => "authentication",
        };
        let delay = (10_i32.saturating_mul(1_i32 << lease.attempts.saturating_sub(1))).min(600);
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let updated=sqlx::query("UPDATE federation_expertise_addressed SET delete_status=$5,delete_error=$6,delete_lease_token=NULL,delete_lease_until=NULL,delete_next_attempt_at=clock_timestamp()+($7 * interval '1 second'),delete_auth_generation=$8 WHERE space=$1 AND request_id=$2 AND kind=$3 AND delete_lease_token=$4")
            .bind(fence.space.as_uuid()).bind(lease.record.request_id).bind(kind(&lease.record.payload))
            .bind(lease.lease_token).bind(if auth_generation.is_some(){"auth_blocked"}else if terminal{"failed"}else{"queued"})
            .bind(reason).bind(delay).bind(auth_generation).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::LostLease);
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn recover_addressed_delete_auth(
        &self,
        fence: WriterFence,
        generation: u64,
    ) -> Result<u64> {
        let generation = i64::try_from(generation).map_err(|_| StoreError::Binding)?;
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let updated=sqlx::query("UPDATE federation_expertise_addressed SET delete_status='queued',delete_attempts=0,delete_auth_generation=NULL,delete_next_attempt_at=clock_timestamp(),delete_error=NULL WHERE space=$1 AND delete_status='auth_blocked' AND delete_auth_generation<$2 AND record IS NOT NULL")
            .bind(fence.space.as_uuid()).bind(generation).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(updated.rows_affected())
    }

    pub async fn acknowledge_addressed(
        &self,
        fence: WriterFence,
        lease: &AddressedLease,
        expected_cid: &str,
        proof: serde_json::Value,
    ) -> Result<AddressedReceipt> {
        if expected_cid.is_empty()
            || proof.get("record_cid").and_then(serde_json::Value::as_str) != Some(expected_cid)
        {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let digest = CanonicalJsonCodec::digest(&lease.record).map_err(|_| StoreError::Binding)?;
        let row = sqlx::query("UPDATE federation_expertise_addressed SET status='published',proof=$5,lease_token=NULL,lease_until=NULL WHERE space=$1 AND request_id=$2 AND kind=$3 AND lease_token=$4 AND status='queued' AND record_digest=$6 RETURNING *")
            .bind(fence.space.as_uuid()).bind(lease.record.request_id).bind(kind(&lease.record.payload))
            .bind(lease.lease_token).bind(Json(&proof)).bind(Json(&digest))
            .fetch_optional(&mut *tx).await?.ok_or(StoreError::LostLease)?;
        let result = receipt(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn fail_addressed(
        &self,
        fence: WriterFence,
        lease: &AddressedLease,
        failure: PublishFailure,
    ) -> Result<AddressedStatus> {
        let terminal = lease.attempts >= 5
            || matches!(
                failure,
                PublishFailure::Conflict
                    | PublishFailure::InvalidReceipt
                    | PublishFailure::AuthorizationDenied
            );
        let auth_generation = match failure {
            PublishFailure::Authentication { generation } => {
                Some(i64::try_from(generation).map_err(|_| StoreError::Binding)?)
            }
            _ => None,
        };
        let reason = match failure {
            PublishFailure::Unavailable => "unavailable",
            PublishFailure::Conflict => "conflict",
            PublishFailure::InvalidReceipt => "invalid_receipt",
            PublishFailure::AuthorizationDenied => "authorization_denied",
            PublishFailure::Authentication { .. } => "authentication",
        };
        let delay = (10_i32.saturating_mul(1_i32 << lease.attempts.saturating_sub(1))).min(600);
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row = sqlx::query("UPDATE federation_expertise_addressed SET status=$5,last_error=$6,lease_token=NULL,lease_until=NULL,next_attempt_at=clock_timestamp()+($7 * interval '1 second'),auth_generation=$8 WHERE space=$1 AND request_id=$2 AND kind=$3 AND lease_token=$4 AND status='queued' RETURNING *")
            .bind(fence.space.as_uuid()).bind(lease.record.request_id).bind(kind(&lease.record.payload))
            .bind(lease.lease_token).bind(if auth_generation.is_some() { "auth_blocked" } else if terminal { "failed" } else { "queued" }).bind(reason)
            .bind(delay).bind(auth_generation).fetch_optional(&mut *tx).await?.ok_or(StoreError::LostLease)?;
        let result = receipt(&row)?.status;
        tx.commit().await?;
        Ok(result)
    }

    /// Only a newly verified access credential generation may reopen a PDS
    /// authentication failure. Proof/conflict failures remain terminal.
    pub async fn recover_addressed_auth(&self, fence: WriterFence, generation: u64) -> Result<u64> {
        let generation = i64::try_from(generation).map_err(|_| StoreError::Binding)?;
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let result = sqlx::query("UPDATE federation_expertise_addressed SET status='queued',attempts=0,auth_generation=NULL,next_attempt_at=clock_timestamp(),last_error=NULL WHERE space=$1 AND status='auth_blocked' AND auth_generation<$2 AND expires_at>clock_timestamp()")
            .bind(fence.space.as_uuid()).bind(generation).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
}
