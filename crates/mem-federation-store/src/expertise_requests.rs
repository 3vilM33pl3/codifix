// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trusted local consent persistence. The service binds identity, resolves and
//! locks current permissions, and supplies OPERATOR-owned destinations, never
//! a list taken from request arguments. No network or general activity writes.
use crate::{Result, StoreError};
use chrono::{DateTime, Utc};
use mem_federation::{
    CanonicalJsonCodec, ContentDigest, SpaceCapability, expertise::*, publication::*,
};
use mem_record::Permission;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionReview {
    pub project: Uuid,
    pub question: ExpertiseQuestion,
    pub advertisement_digest: ContentDigest,
    pub digest: ContentDigest,
}
fn review_digest(
    project: Uuid,
    question: &ExpertiseQuestion,
    ad: &ContentDigest,
) -> Result<ContentDigest> {
    CanonicalJsonCodec::digest(&("memory-expertise-consent-v1", project, question, ad))
        .map_err(|_| StoreError::Binding)
}
async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    question: &ExpertiseQuestion,
    ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
    operator_models: &[String],
) -> Result<()> {
    if access.read_only
        || access.principal != question.principal
        || !access.permissions.contains(Permission::MemoryRead)
        || !access.permissions.contains(Permission::BundleExport)
        || question
            .model_destination
            .as_ref()
            .is_some_and(|d| !operator_models.contains(d))
        || !operator_destinations.contains(&question.output_destination)
    {
        return Err(StoreError::Binding);
    }
    let grant = crate::sharing::load_grant(tx, access, question.space, question.requester).await?;
    if !grant.capabilities.contains(&SpaceCapability::AskExpertise) {
        return Err(StoreError::Binding);
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    ad.validate()?;
    ad.authorize_question(question, now)?;
    Ok(())
}
/// Current local authority for explicit remote discovery and retained reads.
pub async fn authorize_expertise_browse(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
) -> Result<()> {
    if access.read_only
        || !access.permissions.contains(Permission::MemoryRead)
        || !access.permissions.contains(Permission::BundleExport)
    {
        return Err(StoreError::Binding);
    }
    let grant = crate::sharing::load_grant(tx, access, space, instance).await?;
    if !grant.capabilities.contains(&SpaceCapability::AskExpertise) {
        return Err(StoreError::Binding);
    }
    Ok(())
}
/// Read-only consent for RETAINED results. Does not authorize resubmission or
/// extend the original execution deadline. Caller checks current advertisement
/// and operator destination before forwarding any gateway result.
pub async fn retained_expertise_consent(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
    id: Uuid,
) -> Result<QuestionReview> {
    authorize_expertise_browse(tx, access, space, instance).await?;
    let row=sqlx::query("SELECT question,review_digest,advertisement_digest,request_digest FROM federation_expertise_requests WHERE project_id=$1 AND space=$2 AND requester=$3 AND request_id=$4 AND principal_id=$5 AND question IS NOT NULL AND payload_until>clock_timestamp() FOR SHARE")
        .bind(access.project).bind(space.as_uuid()).bind(instance.as_uuid()).bind(id).bind(access.principal).fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
    let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
    let ad = row
        .try_get::<Json<ContentDigest>, _>("advertisement_digest")?
        .0;
    let digest = row.try_get::<Json<ContentDigest>, _>("review_digest")?.0;
    if q.space != space
        || q.requester != instance
        || q.request_id != id
        || q.principal != access.principal
        || q.digest()? != row.try_get::<Json<ContentDigest>, _>("request_digest")?.0
        || review_digest(access.project, &q, &ad)? != digest
    {
        return Err(StoreError::Binding);
    }
    Ok(QuestionReview {
        project: access.project,
        question: q,
        advertisement_digest: ad,
        digest,
    })
}
/// Review contains every transmitted field and binds the exact advertisement.
/// Preparing is read-only and does not authorize transmission.
pub async fn prepare_expertise_question(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    question: &ExpertiseQuestion,
    ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
) -> Result<QuestionReview> {
    prepare_expertise_question_with_models(tx, access, question, ad, operator_destinations, &[])
        .await
}
pub async fn prepare_expertise_question_with_models(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    question: &ExpertiseQuestion,
    ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
    operator_models: &[String],
) -> Result<QuestionReview> {
    authorize(
        tx,
        access,
        question,
        ad,
        operator_destinations,
        operator_models,
    )
    .await?;
    let advertisement_digest = CanonicalJsonCodec::digest(ad).map_err(|_| StoreError::Binding)?;
    Ok(QuestionReview {
        project: access.project,
        question: question.clone(),
        digest: review_digest(access.project, question, &advertisement_digest)?,
        advertisement_digest,
    })
}
/// Commit this transaction BEFORE any transmission. Exact confirmation retries
/// reuse the same receipt; changed inputs never replace prior consent.
pub async fn confirm_expertise_question(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    review: &QuestionReview,
    current_ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
    confirmed_digest: &ContentDigest,
) -> Result<()> {
    confirm_expertise_question_with_models(
        tx,
        access,
        review,
        current_ad,
        operator_destinations,
        &[],
        confirmed_digest,
    )
    .await
}
pub async fn confirm_expertise_question_with_models(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    review: &QuestionReview,
    current_ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
    operator_models: &[String],
    confirmed_digest: &ContentDigest,
) -> Result<()> {
    let expected = prepare_expertise_question_with_models(
        tx,
        access,
        &review.question,
        current_ad,
        operator_destinations,
        operator_models,
    )
    .await?;
    if review.project != access.project
        || review.digest != *confirmed_digest
        || review.digest != expected.digest
        || review.advertisement_digest != expected.advertisement_digest
    {
        return Err(StoreError::Binding);
    }
    let q = &review.question;
    let inserted = sqlx::query("INSERT INTO federation_expertise_requests(project_id,space,requester,request_id,principal_id,request_digest,review_digest,advertisement_digest,question,expires_at,payload_until,metadata_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp()+interval '24 hours',clock_timestamp()+interval '30 days') ON CONFLICT DO NOTHING")
        .bind(access.project).bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id)
        .bind(access.principal).bind(Json(q.digest()?)).bind(Json(&review.digest)).bind(Json(&review.advertisement_digest))
        .bind(Json(q)).bind(q.expires_at).execute(&mut **tx).await?.rows_affected();
    let row = sqlx::query("SELECT project_id,principal_id,review_digest FROM federation_expertise_requests WHERE space=$1 AND requester=$2 AND request_id=$3 FOR UPDATE")
        .bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id).fetch_one(&mut **tx).await?;
    if row.try_get::<Uuid, _>("project_id")? != access.project
        || row.try_get::<Uuid, _>("principal_id")? != access.principal
        || row.try_get::<Json<ContentDigest>, _>("review_digest")?.0 != review.digest
    {
        return Err(StoreError::Binding);
    }
    if inserted == 1 {
        sqlx::query("INSERT INTO federation_expertise_request_audit(project_id,space,requester,request_id,principal_id,review_digest) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(access.project).bind(q.space.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id)
            .bind(access.principal).bind(Json(&review.digest)).execute(&mut **tx).await?;
    }
    Ok(())
}
/// Load only already committed consent in a fresh transaction, then recheck
/// authority and destination policy. Holds never extend read/transmission TTL.
pub async fn consented_expertise_question(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    space: SpaceId,
    requester: InstanceId,
    request_id: Uuid,
    current_ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
) -> Result<ExpertiseQuestion> {
    consented_expertise_question_with_models(
        tx,
        access,
        (space, requester, request_id),
        current_ad,
        operator_destinations,
        &[],
    )
    .await
}
pub async fn consented_expertise_question_with_models(
    tx: &mut Transaction<'_, Postgres>,
    access: &LocalProjectAccess,
    key: (SpaceId, InstanceId, Uuid),
    current_ad: &CapabilityAdvertisement,
    operator_destinations: &[String],
    operator_models: &[String],
) -> Result<ExpertiseQuestion> {
    let (space, requester, request_id) = key;
    let row = sqlx::query("SELECT question,review_digest,advertisement_digest,request_digest FROM federation_expertise_requests WHERE project_id=$1 AND space=$2 AND requester=$3 AND request_id=$4 AND principal_id=$5 AND question IS NOT NULL AND payload_until>clock_timestamp() AND expires_at>clock_timestamp() FOR SHARE")
        .bind(access.project).bind(space.as_uuid()).bind(requester.as_uuid()).bind(request_id)
        .bind(access.principal).fetch_optional(&mut **tx).await?.ok_or(StoreError::Binding)?;
    let q = row.try_get::<Json<ExpertiseQuestion>, _>("question")?.0;
    let expected = prepare_expertise_question_with_models(
        tx,
        access,
        &q,
        current_ad,
        operator_destinations,
        operator_models,
    )
    .await?;
    if q.space != space
        || q.requester != requester
        || q.request_id != request_id
        || row.try_get::<Json<ContentDigest>, _>("review_digest")?.0 != expected.digest
        || row
            .try_get::<Json<ContentDigest>, _>("advertisement_digest")?
            .0
            != expected.advertisement_digest
        || row.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != q.digest()?
    {
        return Err(StoreError::Binding);
    }
    Ok(q)
}
/// Trusted project maintenance only. Payloads are never kept as search context.
pub async fn cleanup_expertise_requests(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
) -> Result<()> {
    sqlx::query("UPDATE federation_expertise_requests SET question=NULL WHERE project_id=$1 AND payload_until<=clock_timestamp() AND NOT hold")
        .bind(project).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM federation_expertise_request_audit a USING federation_expertise_requests r WHERE a.space=r.space AND a.requester=r.requester AND a.request_id=r.request_id AND r.project_id=$1 AND r.metadata_until<=clock_timestamp() AND NOT r.hold")
        .bind(project).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM federation_expertise_requests WHERE project_id=$1 AND metadata_until<=clock_timestamp() AND NOT hold")
        .bind(project).execute(&mut **tx).await?;
    Ok(())
}
