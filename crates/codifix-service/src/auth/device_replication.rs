// SPDX-License-Identifier: AGPL-3.0-or-later
//! Full curated project delivery to an approved personal device, independent of
//! publication review. Both company and origin recheck current device/human scope.
use super::{
    AuthenticatedPrincipal, company_device_delivery, company_device_proxy,
    company_devices::company_binding,
};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use codifix_federation::device_replica::*;
use codifix_federation_runtime::SignedDeviceReplicaPage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerReplicaRequest {
    pub approval_id: Uuid,
    pub request: DeviceReplicaRequest,
}
fn denied() -> ApiError {
    ApiError::forbidden("current enrolled device and selected curated project required")
}
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "project replica binding or cursor changed",
    )
}
fn store_error(e: codifix_federation_store::StoreError) -> ApiError {
    match e {
        codifix_federation_store::StoreError::Limits => ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "curated project exceeds bounded replica limits",
        ),
        codifix_federation_store::StoreError::Database(e) => ApiError::sql(e),
        _ => changed(),
    }
}
fn reply(signed: &SignedDeviceReplicaPage) -> Result<Response, ApiError> {
    let bytes = serde_json::to_vec(signed).map_err(|_| changed())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "replica page too large; request a smaller page",
        ));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
pub(crate) async fn owner_page(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(input): Json<OwnerReplicaRequest>,
) -> Result<Response, ApiError> {
    if !input.request.valid() {
        return Err(changed());
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let (approval, manifest) =
        company_device_delivery::approved(&state, &mut tx, &actor, input.approval_id).await?;
    let r = &input.request;
    if r.device != approval.request.device_id
        || r.scope.company != approval.request.company_id
        || r.scope.owner.as_uuid() != approval.request.owner_instance_id
        || r.scope.project != approval.request.project_id
        || r.scope.area != approval.request.area.as_str()
        || r.catalog_revision != manifest.revision as u64
    {
        return Err(denied());
    }
    let configured = state
        .federation
        .read()
        .map_err(|_| changed())?
        .clone()
        .ok_or_else(denied)?;
    let (sync, owner, space) = configured
        .project_delivery(r.scope.project)
        .ok_or_else(denied)?;
    if r.scope.owner != owner || r.scope.space != space {
        return Err(denied());
    }
    // Capture one committed source snapshot per pass. Subsequent pages keep its
    // high-water mark while current authorization is checked on every request.
    if r.through.is_none() {
        codifix_federation_store::refresh_device_replica(&mut tx, &r.scope)
            .await
            .map_err(store_error)?;
    }
    let page = codifix_federation_store::device_replica_page(&mut tx, r)
        .await
        .map_err(store_error)?;
    tx.commit().await.map_err(ApiError::sql)?;
    let signed = sync
        .attest_device_replica(&page)
        .await
        .map_err(|_| ApiError::service_unavailable("private replica attestation unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let (after, latest) =
        company_device_delivery::approved(&state, &mut tx, &actor, input.approval_id).await?;
    if after != approval || latest.revision != manifest.revision {
        return Err(changed());
    }
    sync.verify_device_replica(&signed, r)
        .map_err(|_| changed())?;
    let response = reply(&signed)?;
    super::onboarding::audit(
        &mut tx,
        actor.id,
        Some(approval.human_principal_id),
        "onboarding.device.replica",
        json!({"device":r.device,"project":r.scope.project,"nonce":r.nonce,"next":page.next}),
    )
    .await?;
    company_device_delivery::approved(&state, &mut tx, &actor, input.approval_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(response)
}
pub(crate) async fn company_page(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((area, owner, project)): Path<(String, Uuid, Uuid)>,
    Json(request): Json<DeviceReplicaRequest>,
) -> Result<Response, ApiError> {
    if !request.valid() {
        return Err(changed());
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let binding = company_binding(&state, &mut tx, &actor, &area, owner, project)
        .await?
        .ok_or_else(denied)?;
    if request.device != binding.device_id
        || request.scope != binding.bootstrap.scope
        || request.catalog_revision != binding.bootstrap.catalog_revision
    {
        return Err(denied());
    }
    let before = company_device_proxy::binding_hash(&binding)?;
    tx.commit().await.map_err(ApiError::sql)?;
    let signed: SignedDeviceReplicaPage = company_device_proxy::post(
        &binding.owner,
        "/v1/onboarding/company-delivery/replica",
        &OwnerReplicaRequest {
            approval_id: binding.approval.request.operation_id,
            request: request.clone(),
        },
        4 * 1024 * 1024,
    )
    .await?;
    let trust = binding.bootstrap.validate().map_err(|_| changed())?;
    trust
        .verify_device_replica_page(&signed, &request, binding.bootstrap.authority)
        .map_err(|_| changed())?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let after = company_binding(&state, &mut tx, &actor, &area, owner, project)
        .await?
        .ok_or_else(denied)?;
    if company_device_proxy::binding_hash(&after)? != before {
        return Err(changed());
    }
    let response = reply(&signed)?;
    super::onboarding::audit(&mut tx,actor.id,Some(after.human_id),"onboarding.company.replica",json!({"device":after.device_id,"project":project,"nonce":request.nonce,"next":signed.claims.page.next})).await?;
    company_binding(&state, &mut tx, &actor, &area, owner, project)
        .await?
        .ok_or_else(denied)?;
    trust
        .verify_device_replica_page(&signed, &request, binding.bootstrap.authority)
        .map_err(|_| changed())?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(response)
}
