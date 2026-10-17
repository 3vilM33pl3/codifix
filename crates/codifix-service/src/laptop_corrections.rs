// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit local correction staging. Remote-owned records remain immutable;
//! the existing contribution outbox routes a new local version to owner review.
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
};
use codifix_config::LaptopCacheReplicationMode;
use codifix_federation::{
    CanonicalJsonCodec, device_replica::DeviceReplicaRecord, subscription::ProjectScope,
};
use codifix_record::*;
use sqlx::{Row, types::Json as SqlJson};
use std::net::SocketAddr;
use uuid::Uuid;

#[cfg(test)]
mod tests;
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "downloaded source or correction operation changed; review the current owner version",
    )
}
pub(crate) async fn stage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(subscription): Path<Uuid>,
    Json(request): Json<LaptopCorrectionRequest>,
) -> Result<Json<LaptopCorrectionReceipt>, ApiError> {
    crate::laptop_cache::authorize(&state, &actor, peer)?;
    if state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
        || !actor.has_global(Permission::MemoryCurate)
        || !actor.has_global(Permission::ActivityCapture)
    {
        return Err(ApiError::forbidden(
            "writable local owner curation and capture authority required",
        ));
    }
    if request.operation_id.is_nil()
        || request.source_memory_id.is_nil()
        || !contribution_sha256_valid(&request.expected_digest)
    {
        return Err(ApiError::validation(ValidationError::new(
            "invalid exact correction identity or digest",
        )));
    }
    let selected = state
        .config
        .laptop_cache
        .as_ref()
        .and_then(|c| {
            c.subscriptions.iter().find(|s| {
                s.subscription_id == subscription
                    && s.replication_mode == LaptopCacheReplicationMode::CuratedProjectV1
            })
        })
        .ok_or_else(|| ApiError::forbidden("selected curated project required"))?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let projection = sqlx::query(
        "SELECT device,scope FROM device_replica_projections WHERE subscription=$1 FOR SHARE",
    )
    .bind(subscription)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::sql)?
    .ok_or_else(changed)?;
    let scope: SqlJson<ProjectScope> = projection.try_get("scope").map_err(ApiError::sql)?;
    let area = state.config.area.as_ref().ok_or_else(changed)?;
    if projection
        .try_get::<Uuid, _>("device")
        .map_err(ApiError::sql)?
        != selected.device_id
        || scope.0.project != selected.project_id
        || scope.0.company != selected.company_id
        || scope.0.owner.as_uuid() != selected.owner_instance_id
        || scope.0.space.as_uuid() != selected.space_id
        || scope.0.area != area.name
    {
        return Err(changed());
    }
    let binding:SqlJson<LaptopContributionBinding>=sqlx::query_scalar("SELECT binding FROM laptop_curated_bindings WHERE local_project_id=$1 AND enabled FOR SHARE").bind(selected.project_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::forbidden("current writable project binding required"))?;
    binding.0.validate().map_err(ApiError::validation)?;
    if binding.0.device_id != selected.device_id
        || binding.0.scope.company_id != scope.0.company
        || binding.0.scope.project_id != scope.0.project
        || binding.0.scope.owner_instance_id != scope.0.owner.as_uuid()
        || binding.0.scope.space_id != scope.0.space.as_uuid()
        || binding.0.scope.area.as_str() != scope.0.area
    {
        return Err(changed());
    }
    let request_sha256 = CanonicalJsonCodec::digest(&(subscription, &binding.0, &request))
        .map_err(|_| changed())?
        .as_hex()
        .to_owned();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("laptop-correction:{}", request.operation_id))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let previous:Option<(String,SqlJson<LaptopCorrectionReceipt>)>=sqlx::query_as("SELECT request_sha256,receipt_json FROM laptop_correction_staging_receipts WHERE operation_id=$1").bind(request.operation_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    if let Some((digest, receipt)) = previous {
        if digest != request_sha256 || receipt.0.subscription_id != subscription {
            return Err(changed());
        }
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(receipt.0));
    }
    let record:SqlJson<DeviceReplicaRecord>=sqlx::query_scalar("SELECT record FROM device_replica_records WHERE subscription=$1 AND record_key=$2 FOR SHARE").bind(subscription).bind(format!("version:{}",request.source_memory_id)).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(changed)?;
    let digest = CanonicalJsonCodec::digest(&record.0).map_err(|_| changed())?;
    if digest.as_hex() != request.expected_digest {
        return Err(changed());
    }
    let DeviceReplicaRecord::MemoryVersion(source) = record.0 else {
        return Err(changed());
    };
    if source.id != request.source_memory_id
        || !DeviceReplicaRecord::MemoryVersion(source.clone()).valid()
    {
        return Err(changed());
    }
    let current:SqlJson<DeviceReplicaRecord>=sqlx::query_scalar("SELECT record FROM device_replica_records WHERE subscription=$1 AND record_key=$2 FOR SHARE").bind(subscription).bind(format!("state:{}",source.canonical_id)).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(changed)?;
    let DeviceReplicaRecord::CanonicalState(current) = current.0 else {
        return Err(changed());
    };
    if current.canonical_id != source.canonical_id
        || source.tombstone
        || current.status != MemoryStatus::Active
        || current.current_version_id != source.id
        || source.version > i32::MAX as u32
    {
        return Err(changed());
    }
    let local = Uuid::new_v5(&request.operation_id, b"local-correction-v1");
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let contribution = CuratedDeviceContribution {
        schema_version: 2,
        operation_id: local,
        binding: binding.0.clone(),
        local_memory_id: local,
        authored_at: now,
        change: CuratedDeviceChange::Correct {
            target_canonical_id: source.canonical_id,
            expected_version: source.version as i32,
            content: request.content.clone(),
        },
    };
    contribution.validate().map_err(ApiError::validation)?;
    sqlx::query("INSERT INTO laptop_curated_origins(local_canonical_id,owner_canonical_id,owner_version) VALUES($1,$2,$3)").bind(local).bind(source.canonical_id).bind(source.version as i32).execute(&mut *tx).await.map_err(ApiError::sql)?;
    let content = &request.content;
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,FALSE,$3,$4,$5,'project',$6,$7,$8,$8,to_tsvector('english',$3 || ' ' || $4))").bind(local).bind(selected.project_id).bind(&content.canonical_text).bind(&content.summary).bind(content.memory_type.to_string()).bind(content.importance).bind(content.confidence).bind(now).execute(&mut *tx).await.map_err(ApiError::sql)?;
    codifix_curate::rebuild_memory_chunks_tx(&mut tx, local)
        .await
        .map_err(ApiError::sql)?;
    let receipt = LaptopCorrectionReceipt {
        operation_id: request.operation_id,
        subscription_id: subscription,
        local_memory_id: local,
        local_canonical_id: local,
        owner_canonical_id: source.canonical_id,
        expected_version: source.version as i32,
        request_sha256,
        staged_at: now,
    };
    sqlx::query("INSERT INTO laptop_correction_staging_receipts(operation_id,subscription_id,local_project_id,request_sha256,receipt_json) VALUES($1,$2,$3,$4,$5)").bind(request.operation_id).bind(subscription).bind(selected.project_id).bind(&receipt.request_sha256).bind(SqlJson(&receipt)).execute(&mut *tx).await.map_err(ApiError::sql)?;
    let mut event = codifix_store::events::TimelineEvent::new(
        selected.project_id,
        ActivityKind::MemoryReplacement,
        "Staged correction to downloaded owner knowledge; approval pending".into(),
    );
    event.id = Uuid::new_v5(&request.operation_id, b"laptop-correction-event");
    event.memory_id = Some(local);
    event.canonical_id = Some(local);
    event.version_no = Some(1);
    event.operation_id = Some(request.operation_id.to_string());
    codifix_store::events::MutationContext {
        actor_id: actor.id.to_string(),
        actor_name: actor.display_name.clone(),
        source: "laptop-correction".into(),
        occurred_at: None,
    }
    .stamp(&mut event);
    let project: String = sqlx::query_scalar("SELECT slug FROM projects WHERE id=$1")
        .bind(selected.project_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let sequence = codifix_store::events::append(&mut tx, &event)
        .await
        .map_err(|error| ApiError::io(error.into()))?;
    tx.commit().await.map_err(ApiError::sql)?;
    crate::repository::events::broadcast_committed(&state, project, event, sequence);
    Ok(Json(receipt))
}
