// SPDX-License-Identifier: AGPL-3.0-or-later
//! In-process addressed Q&A storage, not authentication or a public router.
use crate::{GatewayStore, Result, SpacePolicy, StoreError, WriterFence};
use chrono::{DateTime, Utc};
use mem_federation::{
    CanonicalJsonCodec, ContentDigest, SpaceCapability, expertise::*, publication::*,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow, types::Json};
use uuid::Uuid;

/// Metadata survives payload expiry so current authorization can still be
/// enforced. No question text, evidence, model prompt or answer is retained here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionBinding {
    pub request_id: Uuid,
    pub space: SpaceId,
    pub requester: InstanceId,
    pub principal: Uuid,
    pub capability: RecordKey,
    pub revision: StateRevision,
    pub collection_digest: ContentDigest,
    pub mode: AnswerMode,
    pub model_destination: Option<String>,
    pub output_destination: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
impl From<&ExpertiseQuestion> for QuestionBinding {
    fn from(q: &ExpertiseQuestion) -> Self {
        Self {
            request_id: q.request_id,
            space: q.space,
            requester: q.requester,
            principal: q.principal,
            capability: q.capability,
            revision: q.revision,
            collection_digest: q.collection_digest.clone(),
            mode: q.mode,
            model_destination: q.model_destination.clone(),
            output_destination: q.output_destination.clone(),
            created_at: q.created_at,
            expires_at: q.expires_at,
        }
    }
}
impl QuestionBinding {
    /// Authorization projection only. Never execute it or use its digest as
    /// the immutable request digest. Input bytes were validated on submission.
    fn authorization_projection(&self) -> ExpertiseQuestion {
        ExpertiseQuestion {
            request_id: self.request_id,
            space: self.space,
            requester: self.requester,
            principal: self.principal,
            capability: self.capability,
            revision: self.revision,
            collection_digest: self.collection_digest.clone(),
            mode: self.mode,
            model_destination: self.model_destination.clone(),
            output_destination: self.output_destination.clone(),
            created_at: self.created_at,
            expires_at: self.expires_at,
            question: "?".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionResult {
    pub binding: QuestionBinding,
    pub request_digest: ContentDigest,
    pub policy_digest: ContentDigest,
    pub status: ExpertiseStatus,
    pub payload_until: DateTime<Utc>,
    pub answer: Option<ExpertiseAnswer>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionLease {
    pub question: ExpertiseQuestion,
    pub lease_token: Uuid,
    pub lease_until: DateTime<Utc>,
    pub policy_digest: ContentDigest,
}
fn status_name(status: ExpertiseStatus) -> &'static str {
    match status {
        ExpertiseStatus::Queued => "queued",
        ExpertiseStatus::Running => "running",
        ExpertiseStatus::Answered => "answered",
        ExpertiseStatus::Refused => "refused",
        ExpertiseStatus::Denied => "denied",
        ExpertiseStatus::Expired => "expired",
        ExpertiseStatus::Failed => "failed",
        ExpertiseStatus::Uncertain => "uncertain",
    }
}
pub(super) fn result(row: &PgRow) -> Result<QuestionResult> {
    Ok(QuestionResult {
        binding: row.try_get::<Json<QuestionBinding>, _>("binding")?.0,
        request_digest: row.try_get::<Json<ContentDigest>, _>("request_digest")?.0,
        policy_digest: row.try_get::<Json<ContentDigest>, _>("policy_digest")?.0,
        status: serde_json::from_value(serde_json::Value::String(row.try_get("status")?))?,
        payload_until: row.try_get("payload_until")?,
        answer: row
            .try_get::<Option<Json<ExpertiseAnswer>>, _>("answer")?
            .map(|j| j.0),
    })
}
pub(super) async fn now(tx: &mut Transaction<'_, Postgres>) -> Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}
pub(super) async fn journal(
    tx: &mut Transaction<'_, Postgres>,
    q: &ExpertiseQuestion,
    action: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO federation_expertise_attempts(id,space,requester,responder,request_id,action,metadata_until) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::new_v4()).bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.capability.publisher.as_uuid())
        .bind(q.request_id).bind(action).bind(q.created_at+chrono::Duration::days(30)).execute(&mut **tx).await?;
    Ok(())
}
async fn set_status(
    tx: &mut Transaction<'_, Postgres>,
    q: &ExpertiseQuestion,
    status: &str,
) -> Result<()> {
    sqlx::query("UPDATE federation_expertise_mailbox SET status=$4,lease_token=NULL,lease_until=NULL WHERE space=$1 AND requester=$2 AND request_id=$3")
        .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(status).execute(&mut **tx).await?;
    journal(
        tx,
        q,
        if status == "queued" {
            "recovered"
        } else {
            status
        },
    )
    .await
}
impl GatewayStore {
    /// Only metadata authorized for this requester; current revocations and
    /// unpublished advertisement changes hide a capability immediately.
    pub async fn expertise_capabilities(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        after: Option<RecordKey>,
        limit: u32,
    ) -> Result<Vec<CapabilityAdvertisement>> {
        if !(1..=32).contains(&limit) {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(authenticated, SpaceCapability::Pull)
            || !state.has_capability(authenticated, SpaceCapability::AskExpertise)
        {
            return Err(StoreError::Expertise(ExpertiseReject::Unauthorized));
        }
        let published: i64 = sqlx::query_scalar(
            "SELECT published_sequence FROM federation_gateway_spaces WHERE space=$1",
        )
        .bind(fence.space.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let mut ads: Vec<_> = state.expertise_catalog().filter(|ad| {
            ad.active && ad.requesters.contains(&authenticated)
                && after.is_none_or(|after| ad.key() > after)
                && state.has_capability(ad.owner, SpaceCapability::Pull)
                && state.has_capability(ad.owner, SpaceCapability::AnswerExpertise)
                && state.accepted().iter().rev().find(|d| {
                    matches!(&d.envelope.record, PublicationRecord::Expertise(a) if a.key()==ad.key())
                }).is_some_and(|d| i64::from(d.sequence.get()) <= published)
        }).cloned().collect();
        ads.sort_by_key(CapabilityAdvertisement::key);
        ads.truncate(limit as usize);
        tx.commit().await?;
        Ok(ads)
    }

    /// Administrator supplied compatibility inventory, never remote discovery.
    /// Every current publisher/member (including the authority) must be pinned.
    pub async fn require_expertise_fleet(
        &self,
        fence: WriterFence,
        compatible: &std::collections::BTreeSet<InstanceId>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !compatible.contains(&authority)
            || state.accepted().iter().any(|d| {
                !compatible.contains(&d.envelope.publisher)
                    || matches!(&d.envelope.record, PublicationRecord::Membership { member, .. }
                    if !compatible.contains(member))
            })
        {
            return Err(StoreError::Binding);
        }
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn question_policy(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
        q: &ExpertiseQuestion,
        at: DateTime<Utc>,
    ) -> Result<ContentDigest> {
        let authority = self.lock(tx, fence).await?;
        let state = self.load(tx, fence.space, authority).await?;
        state.authorize_expertise(q.requester, q, at)?;
        let sequence = state
            .accepted()
            .iter()
            .rev()
            .find_map(|d| match &d.envelope.record {
                PublicationRecord::Expertise(ad) if ad.key() == q.capability => {
                    Some(d.sequence.get())
                }
                _ => None,
            })
            .ok_or(StoreError::Binding)?;
        let published: i64 = sqlx::query_scalar(
            "SELECT published_sequence FROM federation_gateway_spaces WHERE space=$1",
        )
        .bind(fence.space.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
        if published < i64::from(sequence) {
            return Err(StoreError::Binding);
        }
        let policy: Json<SpacePolicy> =
            sqlx::query_scalar("SELECT policy FROM federation_security WHERE space=$1")
                .bind(fence.space.as_uuid())
                .fetch_one(&mut **tx)
                .await?;
        policy.validate()?;
        if q.model_destination
            .as_ref()
            .is_some_and(|id| !policy.external_models.contains(id))
            || (q.output_destination != "local_human"
                && !policy.external_models.contains(&q.output_destination))
        {
            return Err(StoreError::Expertise(ExpertiseReject::DestinationDenied));
        }
        Ok(CanonicalJsonCodec::digest(&policy.0).map_err(PublicationError::from)?)
    }
    pub async fn submit_question(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        q: &ExpertiseQuestion,
    ) -> Result<QuestionResult> {
        q.validate()?;
        if authenticated != q.requester || q.space != fence.space {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let digest = q.digest()?;
        let old:Option<Json<ContentDigest>>=sqlx::query_scalar("SELECT request_digest FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3")
            .bind(q.space.as_uuid()).bind(authenticated.as_uuid()).bind(q.request_id).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            if old.0 != digest {
                return Err(StoreError::Binding);
            }
            tx.rollback().await?;
            return self
                .question_result(fence, authenticated, q.request_id)
                .await;
        }
        let at = now(&mut tx).await?;
        let policy = self.question_policy(&mut tx, fence, q, at).await?;
        let depth:i64=sqlx::query_scalar("SELECT count(*) FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND status IN ('queued','running') AND expires_at>clock_timestamp()")
            .bind(q.space.as_uuid()).bind(authenticated.as_uuid()).fetch_one(&mut *tx).await?;
        if depth >= 32 {
            return Err(StoreError::Limits);
        }
        let row=sqlx::query("INSERT INTO federation_expertise_mailbox(space,requester,responder,request_id,request_digest,binding,question,policy_digest,status,created_at,expires_at,payload_until,metadata_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'queued',$9,$10,$11,$12) RETURNING *")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.capability.publisher.as_uuid()).bind(q.request_id)
            .bind(Json(&digest)).bind(Json(QuestionBinding::from(q))).bind(Json(q)).bind(Json(policy))
            .bind(q.created_at).bind(q.expires_at).bind(q.created_at+chrono::Duration::hours(24)).bind(q.created_at+chrono::Duration::days(30))
            .fetch_one(&mut *tx).await?;
        journal(&mut tx, q, "queued").await?;
        tx.commit().await?;
        result(&row)
    }
    pub async fn question_result(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        id: Uuid,
    ) -> Result<QuestionResult> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let row=sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3 AND metadata_until>clock_timestamp() FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).bind(id).fetch_optional(&mut *tx).await?.ok_or(StoreError::Binding)?;
        let mut value = result(&row)?;
        let q = value.binding.authorization_projection();
        // Time only substitutes for the request deadline, never historical
        // memberships/policy. Current revocation also denies retained metadata.
        self.question_policy(&mut tx, fence, &q, q.created_at)
            .await?;
        let at = now(&mut tx).await?;
        if at >= value.payload_until {
            value.answer = None;
            value.status = ExpertiseStatus::Expired;
        } else if !value.status.terminal() && at >= q.expires_at {
            value.status = if row.try_get("provider_started")? {
                ExpertiseStatus::Uncertain
            } else {
                ExpertiseStatus::Expired
            };
            set_status(&mut tx, &q, status_name(value.status)).await?;
        }
        tx.commit().await?;
        // A status/read or submit retry is never authorization to disclose a
        // retained payload. Only the fresh origin delivery path may do that.
        value.answer = None;
        Ok(value)
    }
    pub async fn claim_question(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
    ) -> Result<Option<QuestionLease>> {
        let mut tx = self.pool.begin().await?;
        let authority = self.lock(&mut tx, fence).await?;
        let state = self.load(&mut tx, fence.space, authority).await?;
        if !state.has_capability(
            authenticated,
            mem_federation::SpaceCapability::AnswerExpertise,
        ) {
            return Err(StoreError::Binding);
        }
        let at = now(&mut tx).await?;
        let lost=sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND responder=$2 AND status='running' AND (lease_until<=clock_timestamp() OR expires_at<=clock_timestamp()) FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).fetch_all(&mut *tx).await?;
        for row in lost {
            let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
            let status = if row.try_get::<bool, _>("provider_started")? {
                "uncertain"
            } else if q.expires_at <= at {
                "expired"
            } else {
                "queued"
            };
            set_status(&mut tx, &q, status).await?;
        }
        let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_expertise_mailbox WHERE space=$1 AND responder=$2 AND status='running')")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).fetch_one(&mut *tx).await?;
        if active {
            tx.commit().await?;
            return Ok(None);
        }
        let rows=sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND responder=$2 AND status='queued' ORDER BY created_at,request_id LIMIT 32 FOR UPDATE")
            .bind(fence.space.as_uuid()).bind(authenticated.as_uuid()).fetch_all(&mut *tx).await?;
        for row in rows {
            let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
            let policy = match self.question_policy(&mut tx, fence, &q, at).await {
                Ok(p) => p,
                Err(StoreError::Expertise(reason)) => {
                    set_status(
                        &mut tx,
                        &q,
                        if reason == ExpertiseReject::Expired {
                            "expired"
                        } else {
                            "denied"
                        },
                    )
                    .await?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            let token = Uuid::new_v4();
            let until = (at + chrono::Duration::seconds(30)).min(q.expires_at);
            sqlx::query("UPDATE federation_expertise_mailbox SET status='running',lease_token=$4,lease_until=$5,policy_digest=$6 WHERE space=$1 AND requester=$2 AND request_id=$3")
                .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(token).bind(until).bind(Json(&policy)).execute(&mut *tx).await?;
            journal(&mut tx, &q, "claimed").await?;
            tx.commit().await?;
            return Ok(Some(QuestionLease {
                question: q,
                lease_token: token,
                lease_until: until,
                policy_digest: policy,
            }));
        }
        tx.commit().await?;
        Ok(None)
    }
    async fn fence_question(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        fence: WriterFence,
        authenticated: InstanceId,
        lease: &QuestionLease,
    ) -> Result<PgRow> {
        let q = &lease.question;
        if q.space != fence.space || authenticated != q.capability.publisher {
            return Err(StoreError::Binding);
        }
        self.lock(tx, fence).await?;
        let row=sqlx::query("SELECT * FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3 AND responder=$4 AND status='running' AND lease_token=$5 AND lease_until>clock_timestamp() AND expires_at>clock_timestamp() FOR UPDATE")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(authenticated.as_uuid()).bind(lease.lease_token)
            .fetch_optional(&mut **tx).await?.ok_or(StoreError::LostLease)?;
        if row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0 != *q
            || row.try_get::<Json<ContentDigest>, _>("policy_digest")?.0 != lease.policy_digest
        {
            return Err(StoreError::Binding);
        }
        let at = now(tx).await?;
        if self.question_policy(tx, fence, q, at).await? != lease.policy_digest {
            return Err(StoreError::Expertise(ExpertiseReject::DestinationDenied));
        }
        Ok(row)
    }
    pub async fn renew_question(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        lease: &QuestionLease,
    ) -> Result<DateTime<Utc>> {
        let mut tx = self.pool.begin().await?;
        self.fence_question(&mut tx, fence, authenticated, lease)
            .await?;
        let q = &lease.question;
        let until = (now(&mut tx).await? + chrono::Duration::seconds(30)).min(q.expires_at);
        sqlx::query("UPDATE federation_expertise_mailbox SET lease_until=$4 WHERE space=$1 AND requester=$2 AND request_id=$3")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(until).execute(&mut *tx).await?;
        journal(&mut tx, q, "renewed").await?;
        tx.commit().await?;
        Ok(until)
    }
    /// Reserve before the provider side effect. Never release uncertain usage
    /// or treat a repeated reservation as permission to repeat the invocation.
    pub async fn reserve_question_model(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        lease: &QuestionLease,
        daily_calls: u32,
        daily_tokens: u32,
        tokens: u32,
    ) -> Result<()> {
        if daily_calls == 0
            || daily_calls > 20
            || tokens == 0
            || tokens > daily_tokens
            || lease.question.mode != AnswerMode::Model
        {
            return Err(StoreError::Limits);
        }
        let mut tx = self.pool.begin().await?;
        let row = self
            .fence_question(&mut tx, fence, authenticated, lease)
            .await?;
        if row.try_get::<bool, _>("provider_started")? {
            return Err(StoreError::Binding);
        }
        let q = &lease.question;
        let n=sqlx::query("INSERT INTO federation_expertise_daily_budget(space,responder,day,calls,reserved_tokens) VALUES($1,$2,(clock_timestamp() AT TIME ZONE 'UTC')::date,1,$3) ON CONFLICT(space,responder,day) DO UPDATE SET calls=federation_expertise_daily_budget.calls+1,reserved_tokens=federation_expertise_daily_budget.reserved_tokens+EXCLUDED.reserved_tokens WHERE federation_expertise_daily_budget.calls<$4 AND federation_expertise_daily_budget.reserved_tokens+EXCLUDED.reserved_tokens<=$5")
            .bind(q.space.as_uuid()).bind(authenticated.as_uuid()).bind(i64::from(tokens)).bind(i64::from(daily_calls)).bind(i64::from(daily_tokens)).execute(&mut *tx).await?.rows_affected();
        if n != 1 {
            return Err(StoreError::Limits);
        }
        sqlx::query("UPDATE federation_expertise_mailbox SET provider_started=TRUE WHERE space=$1 AND requester=$2 AND request_id=$3")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).execute(&mut *tx).await?;
        journal(&mut tx, q, "provider_started").await?;
        tx.commit().await?;
        Ok(())
    }
    /// The gateway validates structure/attribution; it cannot prove private
    /// collection membership. The responder MUST independently validate against
    /// its selected eligible sources before persisting/sending the result.
    pub async fn complete_question(
        &self,
        fence: WriterFence,
        authenticated: InstanceId,
        lease: &QuestionLease,
        answer: &ExpertiseAnswer,
    ) -> Result<()> {
        answer.validate(&lease.question, &answer.citations)?;
        let q = &lease.question;
        if authenticated != q.capability.publisher || q.space != fence.space {
            return Err(StoreError::Binding);
        }
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let old:Option<Json<ExpertiseAnswer>>=sqlx::query_scalar("SELECT answer FROM federation_expertise_mailbox WHERE space=$1 AND requester=$2 AND request_id=$3 AND responder=$4 AND answer IS NOT NULL AND payload_until>clock_timestamp()")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(authenticated.as_uuid()).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            if old.0 != *answer {
                return Err(StoreError::Binding);
            }
            self.question_policy(&mut tx, fence, q, q.created_at)
                .await?;
            tx.commit().await?;
            return Ok(());
        }
        let row = self
            .fence_question(&mut tx, fence, authenticated, lease)
            .await?;
        if q.mode == AnswerMode::Model
            && answer.status == ExpertiseStatus::Answered
            && !row.try_get::<bool, _>("provider_started")?
        {
            return Err(StoreError::Binding);
        }
        sqlx::query("UPDATE federation_expertise_mailbox SET status=$4,answer=$5,lease_token=NULL,lease_until=NULL WHERE space=$1 AND requester=$2 AND request_id=$3")
            .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).bind(status_name(answer.status)).bind(Json(answer)).execute(&mut *tx).await?;
        journal(&mut tx, q, status_name(answer.status)).await?;
        tx.commit().await?;
        Ok(())
    }
    /// TTL applies to operational payloads, never publication history. Holds
    /// suspend cleanup, but do not extend authorization or delivery lifetimes.
    pub async fn cleanup_questions(&self, fence: WriterFence) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.lock(&mut tx, fence).await?;
        let hold:Option<bool>=sqlx::query_scalar("SELECT action='hold' FROM federation_retention_decisions WHERE space=$1 AND action IN ('hold','release_hold') ORDER BY decision DESC LIMIT 1")
            .bind(fence.space.as_uuid()).fetch_optional(&mut *tx).await?;
        if hold == Some(true) {
            tx.commit().await?;
            return Ok(());
        }
        sqlx::query("UPDATE federation_expertise_mailbox SET question=NULL,answer=NULL,status=CASE WHEN status IN ('queued','running') THEN CASE WHEN provider_started THEN 'uncertain' ELSE 'expired' END ELSE status END,lease_token=NULL,lease_until=NULL WHERE space=$1 AND payload_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM federation_expertise_deliveries WHERE space=$1 AND metadata_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM federation_expertise_mailbox WHERE space=$1 AND metadata_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM federation_expertise_attempts WHERE space=$1 AND metadata_until<=clock_timestamp()")
            .bind(fence.space.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
