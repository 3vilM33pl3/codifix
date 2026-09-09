// SPDX-License-Identifier: AGPL-3.0-or-later
//! Locally authenticated workflow transactions, never a second remote authority.
use crate::{Result, SourceSelection, StoreError, review_and_share};
use chrono::Utc;
use mem_federation::{
    AuthorIdentity, CanonicalJsonCodec, ContentDigest, SpaceCapability, publication::*,
};
use mem_record::Permission;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, types::Json};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSelection {
    pub snapshot: EvidenceSnapshot,
    pub snapshot_digest: ContentDigest,
}

/// The client echoes the exact source preview and the digest of each reviewed
/// snapshot. Reviewer and review time are deliberately not client inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareReviewRequest {
    pub request_id: Uuid,
    pub canonical: RecordId,
    pub expected: SourceSelection,
    pub evidence: Vec<EvidenceSelection>,
    pub confirm_future_updates: bool,
    /// Echo the shared version displayed during review, including zero for a
    /// first publication. Old clients cannot silently undo an approved correction.
    #[serde(default)]
    pub expected_shared_version: u32,
}

/// A lost HTTP reply may be retried without another subscription change,
/// evidence record or version. Reusing an ID with different bytes fails closed.
/// Exact source comparison, review, source hook and response identity commit in
/// one transaction. Current local/known remote permission is required on retries.
pub async fn submit_share_review(
    pool: &PgPool,
    known: &PublicationState,
    access: &LocalProjectAccess,
    instance: InstanceId,
    request: &ShareReviewRequest,
) -> Result<Uuid> {
    if request.request_id.is_nil() || !request.confirm_future_updates || request.evidence.len() > 16
    {
        return Err(StoreError::Limits);
    }
    if access.read_only {
        return Err(StoreError::LocalPolicy(LocalPolicyReject::ReadOnly));
    }
    for permission in [
        Permission::MemoryRead,
        Permission::MemoryCurate,
        Permission::BundleExport,
    ] {
        if !access.permissions.contains(permission) {
            return Err(StoreError::LocalPolicy(
                LocalPolicyReject::MissingPermission(permission),
            ));
        }
    }
    if !known.has_capability(instance, SpaceCapability::Publish) {
        return Err(StoreError::LocalPolicy(
            LocalPolicyReject::MissingGatewayCapability(SpaceCapability::Publish),
        ));
    }
    let bytes = CanonicalJsonCodec::encode(request).map_err(PublicationError::from)?;
    // Leave room for envelope metadata inside the adapter's 64 KiB limit.
    if bytes.len() > 128 * 1024
        || CanonicalJsonCodec::encode(&request.expected)
            .map_err(PublicationError::from)?
            .len()
            > 48 * 1024
        || request
            .evidence
            .iter()
            .any(|item| CanonicalJsonCodec::encode(item).map_or(true, |b| b.len() > 48 * 1024))
    {
        return Err(StoreError::Limits);
    }
    let digest = ContentDigest::sha256_bytes(&bytes);
    let mut tx = pool.begin().await?;
    let grant = crate::sharing::load_grant(&mut tx, access, known.space(), instance).await?;
    if !grant.capabilities.contains(&SpaceCapability::Publish) {
        return Err(StoreError::LocalPolicy(
            LocalPolicyReject::MissingLocalCapability(SpaceCapability::Publish),
        ));
    }
    // PostgreSQL's transaction lock serializes only this local idempotency key.
    // Hash collisions merely serialize unrelated reviews; they confer no rights.
    let key = format!(
        "{}:{instance:?}:{}:{}:{}",
        known.space().as_uuid(),
        access.project,
        access.principal,
        request.request_id
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(key)
        .execute(&mut *tx)
        .await?;
    let old = sqlx::query("SELECT request_digest,review_id FROM federation_review_requests WHERE space=$1 AND instance=$2 AND project_id=$3 AND principal_id=$4 AND request_id=$5")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).bind(request.request_id)
        .fetch_optional(&mut *tx).await?;
    if let Some(old) = old {
        if old.try_get::<Json<ContentDigest>, _>("request_digest")?.0 != digest {
            return Err(StoreError::Binding);
        }
        return Ok(old.try_get("review_id")?);
    }
    let stream = StreamId {
        origin: instance,
        canonical: request.canonical,
    };
    if known.content_version(stream) != request.expected_shared_version {
        return Err(StoreError::Binding);
    }
    crate::actions::lock_quiet_stream(&mut tx, known, access.project, instance, request.canonical)
        .await?;
    // The shared branch can advance through origin-approved proposals without
    // copying imported text into implicit local/model pipelines. A renewed
    // explicit review reconciles counters and deliberately selects local content.
    if let Some(version) = known.history(stream).last() {
        let content = serde_json::json!({"source":version.publication.source,"content":version.publication.content,"evidence":version.publication.evidence});
        sqlx::query("UPDATE federation_source_subscriptions SET shared_version=$4,state_revision=$5,last_content=$6,last_record=$7 WHERE space=$1 AND instance=$2 AND canonical_id=$3")
            .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(request.canonical.as_uuid())
            .bind(i64::from(known.content_version(stream))).bind(i64::from(known.state_revision(stream))).bind(content).bind(Json(version.key))
            .execute(&mut *tx).await?;
    }
    let reviewer =
        AuthorIdentity::new(access.principal.to_string()).map_err(PublicationError::from)?;
    let evidence = request
        .evidence
        .iter()
        .map(|e| {
            ReviewedEvidence::new(
                e.snapshot.clone(),
                ReviewReceipt {
                    snapshot_digest: e.snapshot_digest.clone(),
                    reviewer: reviewer.clone(),
                    reviewed_at: Utc::now(),
                },
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let review = review_and_share(
        &mut tx,
        known,
        access,
        instance,
        request.canonical,
        &request.expected,
        &evidence,
    )
    .await?;
    sqlx::query("INSERT INTO federation_review_requests(space,instance,project_id,principal_id,request_id,request_digest,review_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(known.space().as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).bind(request.request_id).bind(Json(digest)).bind(review)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(review)
}

#[derive(Debug, Serialize)]
pub struct LocalWorkflowStatus {
    /// These counts belong to the caller, not other users' review queues.
    pub queued_intents: i64,
    pub accepted_not_replicated: i64,
    pub replicated_intents: i64,
    pub blocked_intents: i64,
    pub pending_source_changes: i64,
    pub sync_failures: i32,
    pub sync_error: Option<String>,
    pub retry_at: chrono::DateTime<Utc>,
}

/// Caller supplies a cryptographically verified local cursor. These observations
/// cannot assert the gateway's current publication head or another peer's cursor.
pub async fn local_workflow_status(
    pool: &PgPool,
    access: &LocalProjectAccess,
    space: SpaceId,
    instance: InstanceId,
    cursor: u32,
) -> Result<LocalWorkflowStatus> {
    crate::authorize_source_pull(pool, access, space, instance).await?;
    let row = sqlx::query("SELECT r.sync_failures,r.sync_error,r.retry_at, counts.*, (SELECT count(*) FROM federation_source_changes c JOIN federation_source_subscriptions s USING(space,instance,canonical_id) WHERE c.space=$1 AND c.instance=$2 AND s.project_id=$3 AND s.principal_id=$4 AND NOT c.prepared) pending_source_changes FROM federation_replicas r CROSS JOIN LATERAL (SELECT count(*) FILTER (WHERE acceptance IS NULL) queued_intents, count(*) FILTER (WHERE acceptance IS NOT NULL AND (acceptance->>'sequence')::bigint>$5) accepted_not_replicated, count(*) FILTER (WHERE acceptance IS NOT NULL AND (acceptance->>'sequence')::bigint<=$5) replicated_intents, count(*) FILTER (WHERE acceptance IS NULL AND blocked) blocked_intents FROM federation_source_outbox WHERE space=$1 AND publisher=$2 AND project_id=$3 AND principal_id=$4) counts WHERE r.space=$1 AND r.receiver=$2")
        .bind(space.as_uuid()).bind(instance.as_uuid()).bind(access.project).bind(access.principal).bind(i64::from(cursor))
        .fetch_one(pool).await?;
    Ok(LocalWorkflowStatus {
        queued_intents: row.try_get("queued_intents")?,
        accepted_not_replicated: row.try_get("accepted_not_replicated")?,
        replicated_intents: row.try_get("replicated_intents")?,
        blocked_intents: row.try_get("blocked_intents")?,
        pending_source_changes: row.try_get("pending_source_changes")?,
        sync_failures: row.try_get("sync_failures")?,
        sync_error: row.try_get("sync_error")?,
        retry_at: row.try_get("retry_at")?,
    })
}
