// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exact company delegate plus fresh owner-local human authority. Enrollment
//! attests device identity; it never substitutes for permission to curate.
use super::{
    AuthenticatedPrincipal,
    company_device_delivery::{approved, digest},
};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use mem_record::*;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, types::Json as SqlJson};
use uuid::Uuid;

#[cfg(test)]
mod tests;

fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "curated contribution input or source identity changed",
    )
}

pub(crate) async fn company_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((area, owner, project)): Path<(String, Uuid, Uuid)>,
    Json(input): Json<CuratedDeviceContribution>,
) -> Result<Json<DeviceContributionReceipt>, ApiError> {
    input.validate().map_err(ApiError::validation)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let binding =
        super::company_devices::company_binding(&state, &mut tx, &actor, &area, owner, project)
            .await?
            .ok_or_else(|| ApiError::forbidden("selected company device required"))?;
    let before = super::company_device_proxy::binding_hash(&binding)?;
    if input.binding.device_id != binding.device_id
        || input.binding.scope.company_id != binding.company_id
        || input.binding.scope.project_id != project
        || input.binding.scope.owner_instance_id != owner
        || input.binding.scope.area.as_str() != area
        || input.binding.manifest_revision != binding.manifest.revision
        || input.binding.manifest_sha256
            != super::company_device_delivery::digest(&binding.manifest)?
        || input.binding.scope.space_id != binding.bootstrap.scope.space.as_uuid()
    {
        return Err(changed());
    }
    tx.commit().await.map_err(ApiError::sql)?;
    let transport = OwnerDeviceContributionRequest {
        approval_id: binding.approval.request.operation_id,
        contribution: input.clone(),
    };
    let receipt: DeviceContributionReceipt = super::company_device_proxy::post(
        &binding.owner,
        "/v1/onboarding/company-delivery/contributions",
        &transport,
        65536,
    )
    .await?;
    if receipt.operation_id != input.operation_id
        || receipt.binding != input.binding
        || receipt.local_memory_id != input.local_memory_id
        || receipt.request_sha256 != digest(&transport)?
    {
        return Err(changed());
    }
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let current =
        super::company_devices::company_binding(&state, &mut tx, &actor, &area, owner, project)
            .await?
            .ok_or_else(|| ApiError::forbidden("selected device access revoked"))?;
    if super::company_device_proxy::binding_hash(&current)? != before {
        return Err(changed());
    }
    super::onboarding::audit(&mut tx,actor.id,Some(current.human_id),"onboarding.company.contribution.delivery",json!({"operation_id":input.operation_id,"device_id":current.device_id,"project_id":project,"outcome":receipt.outcome})).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(receipt))
}
pub(crate) async fn company_status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((area, owner, project)): Path<(String, Uuid, Uuid)>,
    Json(operation): Json<Uuid>,
) -> Result<Json<Option<DeviceContributionReceipt>>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let binding =
        super::company_devices::company_binding(&state, &mut tx, &actor, &area, owner, project)
            .await?
            .ok_or_else(|| ApiError::forbidden("selected company device required"))?;
    let before = super::company_device_proxy::binding_hash(&binding)?;
    tx.commit().await.map_err(ApiError::sql)?;
    let receipt: Option<DeviceContributionReceipt> = super::company_device_proxy::post(
        &binding.owner,
        "/v1/onboarding/company-delivery/contributions/status",
        &OwnerDeviceContributionStatusRequest {
            approval_id: binding.approval.request.operation_id,
            operation_id: operation,
        },
        65536,
    )
    .await?;
    if receipt.as_ref().is_some_and(|r| {
        r.operation_id != operation
            || r.binding.device_id != binding.device_id
            || r.binding.scope.project_id != project
            || r.binding.scope.owner_instance_id != owner
    }) {
        return Err(changed());
    }
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let current =
        super::company_devices::company_binding(&state, &mut tx, &actor, &area, owner, project)
            .await?
            .ok_or_else(|| ApiError::forbidden("selected device access revoked"))?;
    if super::company_device_proxy::binding_hash(&current)? != before {
        return Err(changed());
    }
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(receipt))
}
async fn authorize(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    approval: Uuid,
) -> Result<(OwnerDeviceApprovalReceipt, OnboardingProject), ApiError> {
    let result = approved(state, tx, actor, approval).await?;
    let permissions = super::locked_project_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Principal(result.0.human_principal_id),
        result.0.request.project_id,
    )
    .await?;
    if !permissions.contains(Permission::MemoryCurate)
        || !permissions.contains(Permission::ActivityCapture)
    {
        return Err(ApiError::forbidden(
            "current owner-local curation and capture permissions required",
        ));
    }
    Ok(result)
}
pub(crate) async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceContributionRequest>,
) -> Result<Json<DeviceContributionReceipt>, ApiError> {
    request
        .contribution
        .validate()
        .map_err(ApiError::validation)?;
    let input = &request.contribution;
    let hash = digest(&request)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let (approval, manifest) = authorize(&state, &mut tx, &actor, request.approval_id).await?;
    let binding = &input.binding;
    let selected = &approval.request;
    if binding.device_id != selected.device_id
        || binding.scope.company_id != selected.company_id
        || binding.scope.project_id != selected.project_id
        || binding.scope.owner_instance_id != selected.owner_instance_id
        || binding.scope.area != selected.area
        || binding.manifest_revision != selected.manifest_revision
        || binding.manifest_sha256 != selected.manifest_sha256
    {
        return Err(changed());
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "device-contribution-source:{}:{}:{}",
            binding.device_id, selected.project_id, input.local_memory_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("device-contribution:{}", input.operation_id))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let existing: Option<SqlJson<DeviceContributionReceipt>> = sqlx::query_scalar(
        "SELECT receipt_json FROM device_curated_contribution_receipts WHERE operation_id=$1",
    )
    .bind(input.operation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if let Some(existing) = existing {
        if existing.0.request_sha256 != hash
            || existing.0.binding != *binding
            || existing.0.human_principal_id != approval.human_principal_id
        {
            return Err(changed());
        }
        authorize(&state, &mut tx, &actor, request.approval_id).await?;
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(existing.0));
    }
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM device_curated_contribution_receipts WHERE device_id=$1 AND project_id=$2 AND local_memory_id=$3)").bind(binding.device_id).bind(selected.project_id).bind(input.local_memory_id).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    if duplicate {
        return Err(changed());
    }
    let mut receipt = DeviceContributionReceipt {
        schema_version: 2,
        operation_id: input.operation_id,
        binding: binding.clone(),
        local_memory_id: input.local_memory_id,
        request_sha256: hash,
        human_principal_id: approval.human_principal_id,
        outcome: DeviceContributionOutcome::Accepted,
        owner_memory_id: None,
        owner_canonical_id: None,
        owner_version: None,
        proposal_id: None,
        committed_at: sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::sql)?,
    };
    let context = mem_store::events::MutationContext {
        actor_id: approval.human_principal_id.to_string(),
        actor_name: approval.human_principal_id.to_string(),
        source: "device-curated-contribution".into(),
        occurred_at: None,
    };
    let mut event;
    match &input.change {
        CuratedDeviceChange::Add { content } => {
            let id = Uuid::new_v5(&input.operation_id, b"owner-curated-memory");
            sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,FALSE,$3,$4,$5,'project',$6,$7,$8,clock_timestamp(),to_tsvector('english',$3 || ' ' || $4))")
                .bind(id).bind(selected.project_id).bind(&content.canonical_text).bind(&content.summary).bind(content.memory_type.to_string()).bind(content.importance).bind(content.confidence).bind(input.authored_at).execute(&mut *tx).await.map_err(ApiError::sql)?;
            // Provenance is metadata only; no local path, capture, or session body.
            sqlx::query("INSERT INTO memory_sources(id,memory_entry_id,source_kind,excerpt,created_at) VALUES($1,$2,'note',$3,clock_timestamp())")
                .bind(Uuid::new_v5(&input.operation_id,b"device-provenance")).bind(id).bind(format!("Curated device {} operation {}; authenticated author {}",binding.device_id,input.operation_id,approval.human_principal_id)).execute(&mut *tx).await.map_err(ApiError::sql)?;
            mem_curate::rebuild_memory_chunks_tx(&mut tx, id)
                .await
                .map_err(ApiError::sql)?;
            mem_store::post_curation::enqueue(
                &mut tx,
                selected.project_id,
                input.operation_id,
                &[id],
                &mem_store::jobs::JobActor::Principal(approval.human_principal_id),
            )
            .await
            .map_err(|_| ApiError::internal("curated contribution enrichment intent failed"))?;
            receipt.owner_memory_id = Some(id);
            receipt.owner_canonical_id = Some(id);
            receipt.owner_version = Some(1);
            event = mem_store::events::TimelineEvent::new(
                selected.project_id,
                ActivityKind::Curate,
                "Accepted curated device knowledge".into(),
            );
            event.memory_id = Some(id);
            event.canonical_id = Some(id);
            event.version_no = Some(1);
        }
        CuratedDeviceChange::Correct {
            target_canonical_id,
            expected_version,
            content,
        } => {
            let row = sqlx::query("SELECT m.id,m.version_no FROM memory_canonical_state cs JOIN memory_entries m ON m.canonical_id=cs.canonical_id WHERE cs.canonical_id=$1 AND cs.project_id=$2 ORDER BY m.version_no DESC LIMIT 1 FOR UPDATE OF cs")
                .bind(target_canonical_id).bind(selected.project_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(changed)?;
            let version: i32 = row.try_get("version_no").map_err(ApiError::sql)?;
            let target: Uuid = row.try_get("id").map_err(ApiError::sql)?;
            let pending = crate::repository::handlers::loops::proposal_creation::prepare_tx(&mut tx,&LoopMemoryProposalCreateRequest { project:manifest.project.clone(),loop_id:"device-curated-correction".into(),proposal_type:"update".into(),run_id:None,target_memory_id:Some(target),candidate:json!(content),evidence:json!([]),confidence:content.confidence,risk_notes:Some(format!("Device correction expects version {expected_version}; current version {version}. Original canonical {target_canonical_id}")) },input.operation_id,&context).await.map_err(|_| ApiError::internal("curated correction proposal failed"))?;
            receipt.outcome = if version == *expected_version {
                DeviceContributionOutcome::Proposed
            } else {
                DeviceContributionOutcome::Conflict
            };
            receipt.owner_canonical_id = Some(*target_canonical_id);
            receipt.owner_version = Some(version);
            receipt.proposal_id = Some(pending.response.proposal.id);
            event = pending.event.ok_or_else(changed)?;
        }
    }
    sqlx::query("INSERT INTO device_curated_contribution_receipts(operation_id,approval_id,project_id,device_id,human_principal_id,local_memory_id,request_sha256,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(input.operation_id).bind(request.approval_id).bind(selected.project_id).bind(binding.device_id).bind(approval.human_principal_id).bind(input.local_memory_id).bind(&receipt.request_sha256).bind(json!(receipt)).execute(&mut *tx).await.map_err(ApiError::sql)?;
    super::onboarding::audit(&mut tx,actor.id,Some(approval.human_principal_id),"onboarding.company.curated.contribution",json!({"operation_id":input.operation_id,"device_id":binding.device_id,"project_id":selected.project_id,"outcome":receipt.outcome})).await?;
    authorize(&state, &mut tx, &actor, request.approval_id).await?;
    event.id = Uuid::new_v5(&input.operation_id, b"device-contribution-event");
    event.operation_id = Some(input.operation_id.to_string());
    context.stamp(&mut event);
    let sequence = mem_store::events::append(&mut tx, &event)
        .await
        .map_err(|_| ApiError::internal("curated contribution event append failed"))?;
    // The commit-ordered counter may wait behind another transaction. Recheck
    // real-time expiry after that wait while retaining the original auth locks.
    authorize(&state, &mut tx, &actor, request.approval_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    crate::repository::events::broadcast_committed(&state, manifest.project, event, sequence);
    Ok(Json(receipt))
}
pub(crate) async fn status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceContributionStatusRequest>,
) -> Result<Json<Option<DeviceContributionReceipt>>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let (approval, _) = authorize(&state, &mut tx, &actor, request.approval_id).await?;
    let receipt:Option<SqlJson<DeviceContributionReceipt>>=sqlx::query_scalar("SELECT receipt_json FROM device_curated_contribution_receipts WHERE operation_id=$1 AND approval_id=$2 AND device_id=$3 AND project_id=$4 AND human_principal_id=$5")
        .bind(request.operation_id).bind(request.approval_id).bind(approval.request.device_id).bind(approval.request.project_id).bind(approval.human_principal_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(receipt.map(|r| r.0)))
}
