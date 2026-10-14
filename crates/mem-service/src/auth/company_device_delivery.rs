// SPDX-License-Identifier: AGPL-3.0-or-later
//! Owner-local authorization for a company-approved external device. The
//! company service is trusted only to attest enrollment, not to grant a human
//! access or create an owner-local identity. No device bearer is accepted here.
use super::{AuthenticatedPrincipal, CredentialSource};
use crate::{ApiError, AppState};
use axum::{Extension, Json, extract::State, http::StatusCode, response::Response};
use mem_federation::CanonicalJsonCodec;
use mem_record::*;
use serde_json::json;
use sqlx::{Postgres, Transaction, types::Json as SqlJson};
use uuid::Uuid;

#[cfg(test)]
pub(super) mod tests;

fn denied() -> ApiError {
    ApiError::forbidden(
        "exact current company delivery delegate and owner-local human access required",
    )
}
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "company device approval or reviewed owner manifest changed",
    )
}
pub(super) fn digest<T: serde::Serialize>(value: &T) -> Result<String, ApiError> {
    CanonicalJsonCodec::encode(value)
        .map(|bytes| {
            use sha2::{Digest, Sha256};
            Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        })
        .map_err(|_| changed())
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn delegate<'a>(
    state: &'a AppState,
    actor: &AuthenticatedPrincipal,
    company: Uuid,
) -> Result<&'a mem_config::CompanyDeliveryDelegationConfig, ApiError> {
    state
        .config
        .auth
        .validate_company_delivery_delegations()
        .map_err(|_| denied())?;
    if state.config.auth.mode != AuthMode::MultiUser
        || state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
        || actor.kind != AuthPrincipalKind::ServiceToken
        || actor.credential_source != CredentialSource::ServiceToken
    {
        return Err(denied());
    }
    let cfg = state
        .config
        .auth
        .company_delivery_delegations
        .iter()
        .find(|c| {
            c.company_id == company
                && c.principal_id == actor.id
                && Some(c.token_id) == actor.token_id
        })
        .ok_or_else(denied)?;
    let area = state.config.area.as_ref().ok_or_else(denied)?;
    if cfg.owner_instance_id != area.instance_id
        || cfg.area.as_str() != area.name
        || cfg.area == OnboardingArea::Trust
    {
        return Err(denied());
    }
    Ok(cfg)
}
async fn authorize_delegate(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    cfg: &mem_config::CompanyDeliveryDelegationConfig,
    project: Uuid,
) -> Result<(), ApiError> {
    if !cfg.projects.contains(&project) {
        return Err(denied());
    }
    let permissions = super::locked_project_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Credential {
            principal: actor.id,
            token: actor.token_id,
            session: None,
        },
        project,
    )
    .await?;
    // Exact capability, not an admin preset or a broadened membership. This
    // prevents adoption of another credential with the same display name.
    if permissions != PermissionSet::from_permissions(&[Permission::ScopedProjectDelivery]) {
        return Err(denied());
    }
    Ok(())
}
async fn human(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    issuer: &str,
    subject: &str,
) -> Result<Uuid, ApiError> {
    if issuer != state.config.auth.oidc.issuer_url
        || subject.is_empty()
        || subject.len() > 512
        || subject.chars().any(char::is_control)
    {
        return Err(denied());
    }
    // Never create/reactivate an identity, and never bind by an email alias.
    sqlx::query_scalar("SELECT id FROM auth_principals WHERE kind='human_oidc' AND issuer=$1 AND subject=$2 AND disabled_at IS NULL FOR SHARE")
        .bind(issuer).bind(subject).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)
}
async fn manifest(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    request: &OwnerDeviceApprovalRequest,
) -> Result<(Uuid, OnboardingProject), ApiError> {
    let cfg = delegate(state, actor, request.company_id)?;
    if request.owner_instance_id != cfg.owner_instance_id
        || request.area != cfg.area
        || request.issuer != cfg.issuer
        || request.delivery_policy_revision != cfg.policy_revision
        || request.operation_id.is_nil()
        || request.device_request_id.is_nil()
        || request.device_id.is_nil()
        || request.project_id.is_nil()
        || request.manifest_revision < 1
        || !fingerprint(&request.credential_sha256)
        || !fingerprint(&request.manifest_sha256)
    {
        return Err(denied());
    }
    let current: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    if request.expires_at <= current || request.expires_at > current + chrono::Duration::days(30) {
        return Err(denied());
    }
    // Human -> delegate/token -> project/catalog, consistent on approval/replay.
    let principal = human(state, tx, &request.issuer, &request.subject).await?;
    authorize_delegate(state, tx, actor, cfg, request.project_id).await?;
    let permissions = super::locked_project_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Principal(principal),
        request.project_id,
    )
    .await?;
    if !permissions.contains(Permission::MemoryRead)
        || !permissions.contains(Permission::BundleExport)
    {
        return Err(denied());
    }
    if request.curated_contributions_allowed
        && (!permissions.contains(Permission::MemoryCurate)
            || !permissions.contains(Permission::ActivityCapture))
    {
        return Err(denied());
    }
    sqlx::query("SELECT id FROM projects WHERE id=$1 FOR SHARE")
        .bind(request.project_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let row = sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1 FOR SHARE OF v")
        .bind(request.project_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    let selected = super::onboarding::project_row(&row)?;
    if selected.entry.area != request.area
        || selected.entry.owner_instance_id != request.owner_instance_id
        || selected.revision != request.manifest_revision
        || !selected.entry.offline_allowed
        || selected.entry.area == OnboardingArea::Trust
        || digest(&selected)? != request.manifest_sha256
    {
        return Err(changed());
    }
    Ok((principal, selected))
}

/// Saved immutable enrollment plus fresh current authority. A retained success
/// receipt is history, never a substitute for reauthorization.
pub(super) async fn approved(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    id: Uuid,
) -> Result<(OwnerDeviceApprovalReceipt, OnboardingProject), ApiError> {
    let receipt: SqlJson<OwnerDeviceApprovalReceipt> = sqlx::query_scalar(
        "SELECT receipt_json FROM onboarding_delegated_device_approvals WHERE operation_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::sql)?
    .ok_or_else(denied)?;
    let (principal, selected) = manifest(state, tx, actor, &receipt.0.request).await?;
    if receipt.0.request.operation_id != id
        || receipt.0.request_sha256 != digest(&receipt.0.request)?
        || receipt.0.human_principal_id != principal
    {
        return Err(changed());
    }
    Ok((receipt.0, selected))
}

pub(crate) async fn approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceApprovalRequest>,
) -> Result<Json<OwnerDeviceApprovalReceipt>, ApiError> {
    let hash = digest(&request)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let (principal, _) = manifest(&state, &mut tx, &actor, &request).await?;
    // Serialize operation IDs after authorization, without HTTP or external
    // effects. A collision cannot change any existing binding.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("memory:company-device:{}", request.operation_id))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let existing: Option<SqlJson<OwnerDeviceApprovalReceipt>> = sqlx::query_scalar(
        "SELECT receipt_json FROM onboarding_delegated_device_approvals WHERE operation_id=$1",
    )
    .bind(request.operation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    let receipt = if let Some(existing) = existing {
        if existing.0.request != request
            || existing.0.request_sha256 != hash
            || existing.0.human_principal_id != principal
        {
            return Err(changed());
        }
        existing.0
    } else {
        let committed_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::sql)?;
        let receipt = OwnerDeviceApprovalReceipt {
            request: request.clone(),
            request_sha256: hash,
            human_principal_id: principal,
            committed_at,
        };
        use sha2::{Digest, Sha256};
        let bytes = CanonicalJsonCodec::encode(&request).map_err(|_| changed())?;
        sqlx::query("INSERT INTO onboarding_delegated_device_approvals(operation_id,company_id,device_id,project_id,human_principal_id,request_sha256,request_json,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(request.operation_id).bind(request.company_id).bind(request.device_id).bind(request.project_id)
            .bind(principal).bind(Sha256::digest(bytes).to_vec()).bind(json!(request)).bind(json!(receipt)).execute(&mut *tx).await.map_err(ApiError::sql)?;
        super::onboarding::audit(&mut tx,actor.id,Some(principal),"onboarding.company.device.approval",json!({"operation_id":request.operation_id,"company_id":request.company_id,"device_id":request.device_id,"project_id":request.project_id,"manifest_revision":request.manifest_revision})).await?;
        receipt
    };
    manifest(&state, &mut tx, &actor, &request).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(receipt))
}

pub(crate) async fn catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceCatalogRequest>,
) -> Result<Json<OwnerDeviceCatalogResponse>, ApiError> {
    let cfg = delegate(&state, &actor, request.company_id)?;
    if request.owner_instance_id != cfg.owner_instance_id
        || request.area != cfg.area
        || request.issuer != cfg.issuer
        || request.delivery_policy_revision != cfg.policy_revision
    {
        return Err(denied());
    }
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let principal = human(&state, &mut tx, &request.issuer, &request.subject).await?;
    let mut projects = Vec::new();
    for project in &cfg.projects {
        authorize_delegate(&state, &mut tx, &actor, cfg, *project).await?;
        let current = super::locked_project_permissions(
            &state,
            &mut tx,
            &mem_store::jobs::JobActor::Principal(principal),
            *project,
        )
        .await?;
        // Authorize before selecting even the catalog metadata.
        if !current.contains(Permission::MemoryRead) {
            continue;
        }
        sqlx::query("SELECT id FROM projects WHERE id=$1 FOR SHARE")
            .bind(project)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::sql)?
            .ok_or_else(denied)?;
        let row = sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1 FOR SHARE OF v")
            .bind(project).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
        let Some(row) = row else { continue };
        let selected = super::onboarding::project_row(&row)?;
        if selected.entry.area != cfg.area
            || selected.entry.owner_instance_id != cfg.owner_instance_id
        {
            return Err(changed());
        }
        let manifest_sha256 = digest(&selected)?;
        let offline_available =
            selected.entry.offline_allowed && current.contains(Permission::BundleExport);
        projects.push(OwnerDeviceCatalogProject {
            project: OnboardingDeviceReviewProject {
                manifest: selected,
                allowed_roles: vec![AuthRole::Reader],
                offline_available,
            },
            manifest_sha256,
            curated_contributions_available: offline_available
                && current.contains(Permission::MemoryCurate)
                && current.contains(Permission::ActivityCapture),
        });
    }
    let response = OwnerDeviceCatalogResponse {
        request_sha256: digest(&request)?,
        projects,
        observed_at: sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::sql)?,
    };
    if serde_json::to_vec(&response).map_err(|_| changed())?.len() > 512 * 1024 {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "owner catalog exceeds bounded response size",
        ));
    }
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
}

pub(crate) async fn subscription(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceSubscriptionRequest>,
) -> Result<Json<mem_federation_runtime::DeviceProjectSubscription>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let (receipt, _) = approved(&state, &mut tx, &actor, request.approval_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    super::scoped_delivery::subscription_for(
        state,
        actor,
        receipt.request.project_id,
        Some(request.approval_id),
    )
    .await
}

pub(crate) async fn snapshot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OwnerDeviceSnapshotRequest>,
) -> Result<Response, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let (receipt, _) = approved(&state, &mut tx, &actor, request.approval_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    super::scoped_delivery::snapshot_for(
        state,
        actor,
        receipt.request.project_id,
        request.snapshot,
        Some(request.approval_id),
    )
    .await
}
