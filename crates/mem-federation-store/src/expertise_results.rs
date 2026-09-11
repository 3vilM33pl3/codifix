// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private immutable deterministic results, committed before gateway completion.
//! Caller verifies gateway leases and locks current local authority/sources.
use crate::{QuestionLease, Result, StoreError};
use mem_federation::{CanonicalJsonCodec, ContentDigest, expertise::*};
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

pub async fn persist_expertise_answer(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    lease: &QuestionLease,
    answer: &ExpertiseAnswer,
) -> Result<ExpertiseAnswer> {
    let q = &lease.question;
    if project.is_nil()
        || lease.lease_token.is_nil()
        || q.mode != AnswerMode::Deterministic
        || lease.lease_until > q.expires_at
    {
        return Err(StoreError::Binding);
    }
    // Structural validation only; runtime checks exact eligible sources.
    answer.validate(q, &answer.citations)?;
    let digest = CanonicalJsonCodec::digest(answer).map_err(|_| StoreError::Binding)?;
    sqlx::query("INSERT INTO federation_expertise_origin_results(project_id,space,responder,requester,request_id,request_digest,answer_digest,answer,lease_token) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9 WHERE $10>clock_timestamp() ON CONFLICT DO NOTHING")
        .bind(project).bind(q.space.as_uuid()).bind(q.capability.publisher.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id)
        .bind(Json(q.digest()?)).bind(Json(&digest)).bind(Json(answer)).bind(lease.lease_token).bind(lease.lease_until)
        .execute(&mut **tx).await?;
    let row=sqlx::query("SELECT project_id,request_digest,answer_digest,answer FROM federation_expertise_origin_results WHERE space=$1 AND responder=$2 AND requester=$3 AND request_id=$4 AND payload_until>clock_timestamp() AND $5>clock_timestamp() FOR SHARE")
        .bind(q.space.as_uuid()).bind(q.capability.publisher.as_uuid()).bind(q.requester.as_uuid()).bind(q.request_id)
        .bind(lease.lease_until).fetch_optional(&mut **tx).await?.ok_or(StoreError::LostLease)?;
    if row.try_get::<Uuid, _>("project_id")? != project
        || row.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != q.digest()?
        || row.try_get::<Json<ContentDigest>, _>("answer_digest")?.0 != digest
    {
        return Err(StoreError::Binding);
    }
    let stored = row
        .try_get::<Option<Json<ExpertiseAnswer>>, _>("answer")?
        .ok_or(StoreError::Binding)?
        .0;
    if stored != *answer {
        return Err(StoreError::Corrupt);
    }
    // Gateway completion is idempotent; a lost HTTP ack cannot undo this result.
    Ok(stored)
}
pub async fn cleanup_expertise_results(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
) -> Result<()> {
    sqlx::query("UPDATE federation_expertise_origin_results SET answer=NULL WHERE project_id=$1 AND payload_until<=clock_timestamp() AND NOT hold")
        .bind(project).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM federation_expertise_origin_results WHERE project_id=$1 AND metadata_until<=clock_timestamp() AND NOT hold")
        .bind(project).execute(&mut **tx).await?;
    Ok(())
}
