// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local device delivery linearizes at the final protected transaction commit.
use super::{AuthenticatedPrincipal, CredentialSource};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use codifix_federation::{CanonicalJsonCodec, publication::*, subscription::*};
use codifix_federation_store::{CurrentProjectSources, ProjectBindingReceipt, ProjectEvent};
use codifix_record::*;
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, types::Json as SqlJson};
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(test)]
mod tests;

fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "project delivery authority, source selection or cursor changed; refresh before retrying",
    )
}
fn store_error(error: codifix_federation_store::StoreError) -> ApiError {
    if matches!(error, codifix_federation_store::StoreError::Limits) {
        ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "project source selection exceeds bounded stream/record limits",
        )
    } else {
        changed()
    }
}
fn unavailable(id: Uuid) -> ApiError {
    ApiError::service_unavailable(&format!(
        "project registration confirmation uncertain; retry the exact snapshot request; operation {id}"
    ))
}
fn hash<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, ApiError> {
    use sha2::{Digest, Sha256};
    CanonicalJsonCodec::encode(value)
        .map(|v| Sha256::digest(v).to_vec())
        .map_err(|_| ApiError::internal("invalid project delivery binding"))
}
pub(super) struct Eligibility {
    device: Uuid,
    scope: ProjectScope,
    catalog: u64,
    sources: CurrentProjectSources,
    fingerprint: Vec<u8>,
}
async fn eligibility(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    project: Uuid,
    owner: InstanceId,
    space: SpaceId,
    known: &PublicationState,
) -> Result<Eligibility, ApiError> {
    if actor.credential_source != CredentialSource::ServiceToken
        || actor.kind != AuthPrincipalKind::ServiceToken
        || actor.token_id.is_none()
        || state.config.auth.mode != AuthMode::MultiUser
        || state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
    {
        return Err(ApiError::forbidden(
            "local enrolled device service credential required",
        ));
    }
    let area = state
        .config
        .area
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("explicit local owner area required"))?;
    let company = state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .map(|c| c.company_id)
        .filter(|id| !id.is_nil())
        .ok_or_else(|| ApiError::forbidden("explicit company identity required"))?;
    if area.instance_id != owner.as_uuid() || area.name == "trust" {
        return Err(ApiError::forbidden(
            "remote-owner and Trust offline delivery are not enabled",
        ));
    }
    let permission = super::locked_project_permissions(
        state,
        tx,
        &codifix_store::jobs::JobActor::Credential {
            principal: actor.id,
            token: actor.token_id,
            session: None,
        },
        project,
    )
    .await?;
    if !permission.contains(Permission::MemoryRead)
        || !permission.contains(Permission::BundleExport)
    {
        return Err(ApiError::forbidden(
            "current linked human read and offline export permission required",
        ));
    }
    let row=sqlx::query("SELECT d.id,r.selection_json FROM onboarding_devices d JOIN onboarding_device_requests r ON r.device_id=d.id WHERE d.machine_principal_id=$1 AND d.service_token_id=$2 AND d.revoked_at IS NULL AND r.status='approved' FOR SHARE OF d,r")
        .bind(actor.id).bind(actor.token_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?
        .ok_or_else(||ApiError::forbidden("exact approved device credential required"))?;
    let device: Uuid = row.try_get("id").map_err(ApiError::sql)?;
    let selections: Vec<OnboardingDeviceSelection> = row
        .try_get::<SqlJson<Vec<OnboardingDeviceSelection>>, _>("selection_json")
        .map_err(ApiError::sql)?
        .0;
    let selected = selections
        .iter()
        .find(|s| s.project_id == project)
        .ok_or_else(|| ApiError::forbidden("project was not approved for this device"))?;
    // Catalog writers take this row FOR UPDATE. Keep it through the final byte
    // serialization and commit; UUID identity never comes from a slug.
    sqlx::query("SELECT id FROM projects WHERE id=$1 FOR SHARE")
        .bind(project)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::not_found("local project not found"))?;
    let catalog=sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1 FOR SHARE OF v")
        .bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::forbidden("reviewed project required"))?;
    let manifest = super::onboarding::project_row(&catalog)?;
    if manifest.revision != selected.revision
        || !manifest.entry.offline_allowed
        || manifest.entry.area == OnboardingArea::Trust
        || manifest.entry.owner_instance_id != owner.as_uuid()
        || manifest.entry.area.as_str() != area.name
    {
        return Err(changed());
    }
    let scope = ProjectScope {
        company,
        area: area.name.clone(),
        owner,
        project,
        space,
    };
    if !scope.valid() {
        return Err(ApiError::forbidden(
            "unsupported local project delivery scope",
        ));
    }
    sources(state, tx, device, scope, manifest, known).await
}

/// Reused only after an audience has been independently authorized. Source
/// review, version and publication checks remain identical for both paths.
pub(super) async fn sources(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    device: Uuid,
    scope: ProjectScope,
    manifest: OnboardingProject,
    known: &PublicationState,
) -> Result<Eligibility, ApiError> {
    let project = scope.project;
    let space = scope.space;
    let owner = scope.owner;
    let mut reviewers = BTreeMap::new();
    for id in codifix_federation_store::project_source_reviewers(tx, project, space, owner)
        .await
        .map_err(store_error)?
    {
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1 AND disabled_at IS NULL)",
        )
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
        let current = if active {
            super::locked_project_permissions(
                state,
                tx,
                &codifix_store::jobs::JobActor::Principal(id),
                project,
            )
            .await?
        } else {
            PermissionSet::EMPTY
        };
        reviewers.insert(id, current);
    }
    let sources = codifix_federation_store::current_project_sources(
        tx, project, space, owner, known, &reviewers,
    )
    .await
    .map_err(store_error)?;
    let fingerprint =
        hash(&json!({"scope":scope,"manifest":manifest,"sources":sources.fingerprint}))?;
    Ok(Eligibility {
        device,
        scope,
        catalog: u64::try_from(manifest.revision).map_err(|_| changed())?,
        sources,
        fingerprint,
    })
}

async fn audience_eligibility(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    project: Uuid,
    binding: (InstanceId, SpaceId),
    known: &PublicationState,
    approval: Option<Uuid>,
) -> Result<Eligibility, ApiError> {
    let (owner, space) = binding;
    match approval {
        None => eligibility(state, tx, actor, project, owner, space, known).await,
        Some(id) => {
            let (receipt, manifest) =
                super::company_device_delivery::approved(state, tx, actor, id).await?;
            if receipt.request.project_id != project
                || receipt.request.owner_instance_id != owner.as_uuid()
            {
                return Err(changed());
            }
            sources(
                state,
                tx,
                receipt.request.device_id,
                ProjectScope {
                    company: receipt.request.company_id,
                    area: receipt.request.area.as_str().into(),
                    owner,
                    project,
                    space,
                },
                manifest,
                known,
            )
            .await
        }
    }
}

struct Registration {
    request: ProjectFeedRegistration,
    fingerprint: Vec<u8>,
    receipt: Option<ProjectBindingReceipt>,
}
async fn prepare(
    tx: &mut Transaction<'_, Postgres>,
    current: &Eligibility,
    actor: Uuid,
) -> Result<Registration, ApiError> {
    sqlx::query("INSERT INTO onboarding_owner_project_heads(project_id,scope) VALUES($1,$2) ON CONFLICT DO NOTHING")
        .bind(current.scope.project).bind(json!(current.scope)).execute(&mut **tx).await.map_err(ApiError::sql)?;
    let head =
        sqlx::query("SELECT * FROM onboarding_owner_project_heads WHERE project_id=$1 FOR UPDATE")
            .bind(current.scope.project)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::sql)?;
    if head.try_get::<Value, _>("scope").map_err(ApiError::sql)? != json!(current.scope) {
        return Err(changed());
    }
    if let Some(id) = head
        .try_get::<Option<Uuid>, _>("pending_operation")
        .map_err(ApiError::sql)?
    {
        let row = sqlx::query("SELECT * FROM onboarding_owner_project_operations WHERE id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::sql)?;
        return Ok(Registration {
            request: row
                .try_get::<SqlJson<ProjectFeedRegistration>, _>("request")
                .map_err(ApiError::sql)?
                .0,
            fingerprint: row.try_get("fingerprint").map_err(ApiError::sql)?,
            receipt: None,
        });
    }
    let revision: i64 = head.try_get("revision").map_err(ApiError::sql)?;
    if head
        .try_get::<Option<Vec<u8>>, _>("fingerprint")
        .map_err(ApiError::sql)?
        .as_ref()
        == Some(&current.fingerprint)
    {
        let row=sqlx::query("SELECT * FROM onboarding_owner_project_operations WHERE project_id=$1 AND receipt IS NOT NULL ORDER BY confirmed_at DESC,id LIMIT 1")
            .bind(current.scope.project).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
        return Ok(Registration {
            request: row
                .try_get::<SqlJson<ProjectFeedRegistration>, _>("request")
                .map_err(ApiError::sql)?
                .0,
            fingerprint: current.fingerprint.clone(),
            receipt: row
                .try_get::<Option<SqlJson<ProjectBindingReceipt>>, _>("receipt")
                .map_err(ApiError::sql)?
                .map(|v| v.0),
        });
    }
    let prior: Vec<ProjectStreamReview> = head
        .try_get::<SqlJson<Vec<ProjectStreamReview>>, _>("reviews")
        .map_err(ApiError::sql)?
        .0;
    let reviews = complete_reviews(&prior, &current.sources.reviews)?;
    let policy: i64 = head.try_get("policy_revision").map_err(ApiError::sql)?;
    let request = ProjectFeedRegistration {
        operation_id: Uuid::new_v4(),
        scope: current.scope.clone(),
        expected_revision: u64::try_from(revision).map_err(|_| changed())?,
        catalog_revision: current.catalog,
        policy_revision: u64::try_from(policy.checked_add(1).ok_or_else(changed)?)
            .map_err(|_| changed())?,
        reviews,
    };
    sqlx::query("INSERT INTO onboarding_owner_project_operations(id,project_id,request,request_sha256,fingerprint) VALUES($1,$2,$3,$4,$5)")
        .bind(request.operation_id).bind(current.scope.project).bind(json!(request)).bind(hash(&request)?).bind(&current.fingerprint).execute(&mut **tx).await.map_err(ApiError::sql)?;
    sqlx::query(
        "UPDATE onboarding_owner_project_heads SET pending_operation=$2 WHERE project_id=$1",
    )
    .bind(current.scope.project)
    .bind(request.operation_id)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    super::onboarding::audit(tx,actor,None,"onboarding.project.registration.intent",json!({"operation_id":request.operation_id,"project_id":current.scope.project,"expected_revision":request.expected_revision,"request_sha256":CanonicalJsonCodec::digest(&request).map_err(|_|changed())?})).await?;
    Ok(Registration {
        request,
        fingerprint: current.fingerprint.clone(),
        receipt: None,
    })
}
fn complete_reviews(
    prior: &[ProjectStreamReview],
    active: &[ProjectStreamReview],
) -> Result<Vec<ProjectStreamReview>, ApiError> {
    let mut all: BTreeMap<_, _> = prior
        .iter()
        .map(|review| {
            let mut disabled = review.clone();
            disabled.enabled = false;
            disabled.records.clear();
            (review.stream, disabled)
        })
        .collect();
    for review in active {
        all.insert(review.stream, review.clone());
    }
    if all.len() > 32 {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "project registration exceeds retained stream bound",
        ));
    }
    Ok(all.into_values().collect())
}
async fn confirm(
    state: &AppState,
    registration: &Registration,
    receipt: &ProjectBindingReceipt,
    actor: Uuid,
) -> Result<(), ApiError> {
    if receipt.operation_id != registration.request.operation_id
        || receipt.revision != registration.request.expected_revision + 1
        || receipt.request_digest
            != CanonicalJsonCodec::digest(&registration.request).map_err(|_| changed())?
    {
        return Err(unavailable(registration.request.operation_id));
    }
    let mut tx = state
        .pool()?
        .begin()
        .await
        .map_err(|_| unavailable(registration.request.operation_id))?;
    let head=sqlx::query("SELECT pending_operation,revision FROM onboarding_owner_project_heads WHERE project_id=$1 FOR UPDATE")
        .bind(registration.request.scope.project).fetch_one(&mut *tx).await.map_err(|_|unavailable(registration.request.operation_id))?;
    let pending: Option<Uuid> = head
        .try_get("pending_operation")
        .map_err(|_| unavailable(registration.request.operation_id))?;
    if pending == Some(registration.request.operation_id) {
        sqlx::query("UPDATE onboarding_owner_project_operations SET receipt=$2,confirmed_at=clock_timestamp() WHERE id=$1 AND receipt IS NULL")
            .bind(registration.request.operation_id).bind(json!(receipt)).execute(&mut *tx).await.map_err(|_|unavailable(registration.request.operation_id))?;
        sqlx::query("UPDATE onboarding_owner_project_heads SET revision=$2,policy_revision=$3,fingerprint=$4,reviews=$5,pending_operation=NULL WHERE project_id=$1")
            .bind(registration.request.scope.project).bind(i64::try_from(receipt.revision).map_err(|_|changed())?)
            .bind(i64::try_from(registration.request.policy_revision).map_err(|_|changed())?).bind(&registration.fingerprint).bind(json!(registration.request.reviews))
            .execute(&mut *tx).await.map_err(|_|unavailable(registration.request.operation_id))?;
        super::onboarding::audit(&mut tx,actor,None,"onboarding.project.registration.confirmed",json!({"operation_id":registration.request.operation_id,"project_id":registration.request.scope.project,"binding_revision":receipt.revision})).await.map_err(|_|unavailable(registration.request.operation_id))?;
    } else {
        let saved: Option<SqlJson<ProjectBindingReceipt>> = sqlx::query_scalar(
            "SELECT receipt FROM onboarding_owner_project_operations WHERE id=$1",
        )
        .bind(registration.request.operation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| unavailable(registration.request.operation_id))?;
        if saved.map(|v| v.0).as_ref() != Some(receipt) {
            return Err(changed());
        }
    }
    tx.commit()
        .await
        .map_err(|_| unavailable(registration.request.operation_id))
}

pub(crate) async fn snapshot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(project): Path<Uuid>,
    Json(request): Json<OnboardingProjectSnapshotRequest>,
) -> Result<Response, ApiError> {
    snapshot_for(state, actor, project, request, None).await
}

fn require_local_route(state: &AppState, area: &str, owner: Uuid) -> Result<(), ApiError> {
    let local = state.config.area.as_ref().ok_or_else(changed)?;
    if local.name != area || local.instance_id != owner || area == "trust" || owner.is_nil() {
        return Err(ApiError::forbidden(
            "exact selected area and owner required; no fallback",
        ));
    }
    Ok(())
}

/// Complete identity for new clients. A remote owner never falls through to a
/// coincidentally same-named/same-UUID local project. Fleet bridging is explicit.
pub(crate) async fn scoped_snapshot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((area, owner, project)): Path<(String, Uuid, Uuid)>,
    Json(request): Json<OnboardingProjectSnapshotRequest>,
) -> Result<Response, ApiError> {
    if let Some(response) = super::company_device_proxy::snapshot(
        &state,
        &actor,
        &area,
        owner,
        project,
        request.clone(),
    )
    .await?
    {
        return Ok(response);
    }
    require_local_route(&state, &area, owner)?;
    snapshot_for(state, actor, project, request, None).await
}

pub(crate) async fn scoped_subscription(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((area, owner, project)): Path<(String, Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    if let Some(response) =
        super::company_device_proxy::subscription(&state, &actor, &area, owner, project).await?
    {
        return Ok(response);
    }
    require_local_route(&state, &area, owner)?;
    subscription_for(state, actor, project, None)
        .await
        .map(IntoResponse::into_response)
}

pub(super) async fn snapshot_for(
    state: AppState,
    actor: AuthenticatedPrincipal,
    project: Uuid,
    request: OnboardingProjectSnapshotRequest,
    approval: Option<Uuid>,
) -> Result<Response, ApiError> {
    if request.nonce.is_nil()
        || project.is_nil()
        || !(1..=32).contains(&request.limit)
        || request.after > i64::MAX as u64
        || request
            .through
            .is_some_and(|v| v < request.after || v > i64::MAX as u64)
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid bounded project snapshot cursor",
        ));
    }
    let configured = state
        .federation
        .read()
        .map_err(|_| ApiError::service_unavailable("owner delivery unavailable"))?
        .clone()
        .ok_or_else(|| ApiError::forbidden("local owner federation instance is not enrolled"))?;
    let (sync, owner, space) = configured
        .project_delivery(project)
        .ok_or_else(|| ApiError::forbidden("project is not enrolled on this local owner"))?;
    // All retained receipts are verified before they become key candidates.
    let (known, _) = sync.verified_project_sources().await.map_err(|_| {
        ApiError::service_unavailable("verified local source publications unavailable")
    })?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let current = audience_eligibility(
        &state,
        &mut tx,
        &actor,
        project,
        (owner, space),
        &known,
        approval,
    )
    .await?;
    if let Some(id) = approval {
        let bound = OwnerDeviceSnapshotRequest {
            approval_id: id,
            snapshot: request.clone(),
        };
        sqlx::query("INSERT INTO onboarding_delegated_snapshot_requests(company_id,device_id,nonce,approval_id,request_json) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(current.scope.company).bind(current.device).bind(request.nonce).bind(id).bind(json!(bound)).execute(&mut *tx).await.map_err(ApiError::sql)?;
        let saved: Value = sqlx::query_scalar("SELECT request_json FROM onboarding_delegated_snapshot_requests WHERE company_id=$1 AND device_id=$2 AND nonce=$3")
            .bind(current.scope.company).bind(current.device).bind(request.nonce).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        if saved != json!(bound) {
            return Err(changed());
        }
    } else {
        sqlx::query("INSERT INTO onboarding_owner_snapshot_requests(device_id,nonce,project_id,request) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(current.device).bind(request.nonce).bind(project).bind(json!(request)).execute(&mut *tx).await.map_err(ApiError::sql)?;
        let saved=sqlx::query("SELECT project_id,request FROM onboarding_owner_snapshot_requests WHERE device_id=$1 AND nonce=$2")
        .bind(current.device).bind(request.nonce).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        if saved
            .try_get::<Uuid, _>("project_id")
            .map_err(ApiError::sql)?
            != project
            || saved
                .try_get::<Value, _>("request")
                .map_err(ApiError::sql)?
                != json!(request)
        {
            return Err(changed());
        }
    }
    let registration = prepare(&mut tx, &current, actor.id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    // No SQL transaction, credential lock or source lock is held across HTTP.
    let receipt = match &registration.receipt {
        Some(v) => v.clone(),
        None => {
            let receipt = sync
                .register_project_delivery(&registration.request)
                .await
                .map_err(|_| unavailable(registration.request.operation_id))?;
            confirm(&state, &registration, &receipt, actor.id).await?;
            receipt
        }
    };
    if registration.fingerprint != current.fingerprint {
        return Err(changed());
    }
    let page_request = ProjectPageRequest {
        nonce: request.nonce,
        device: current.device,
        scope: current.scope.clone(),
        catalog_revision: current.catalog,
        policy_revision: registration.request.policy_revision,
        binding_revision: receipt.revision,
        after: request.after,
        through: request.through,
        limit: request.limit,
    };
    let signed = sync
        .project_delivery_page(&page_request)
        .await
        .map_err(|_| {
            ApiError::service_unavailable("verified selective gateway page unavailable")
        })?;
    let (latest, _) = sync.verified_project_sources().await.map_err(|_| {
        ApiError::service_unavailable("verified local source publications unavailable")
    })?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let final_state = audience_eligibility(
        &state,
        &mut tx,
        &actor,
        project,
        (owner, space),
        &latest,
        approval,
    )
    .await?;
    if final_state.fingerprint != registration.fingerprint || final_state.device != current.device {
        return Err(changed());
    }
    let head=sqlx::query("SELECT revision,pending_operation,fingerprint FROM onboarding_owner_project_heads WHERE project_id=$1 FOR SHARE")
        .bind(project).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    if head.try_get::<i64, _>("revision").map_err(ApiError::sql)?
        != i64::try_from(receipt.revision).map_err(|_| changed())?
        || head
            .try_get::<Option<Uuid>, _>("pending_operation")
            .map_err(ApiError::sql)?
            .is_some()
        || head
            .try_get::<Option<Vec<u8>>, _>("fingerprint")
            .map_err(ApiError::sql)?
            .as_ref()
            != Some(&final_state.fingerprint)
    {
        return Err(changed());
    }
    let page = sync
        .verify_project_delivery_page(signed.clone(), &page_request)
        .map_err(|_| changed())?;
    validate_entries(
        &final_state.sources,
        &registration.request.reviews,
        &page.entries,
    )?;
    let bytes = serde_json::to_vec(&signed)
        .map_err(|_| ApiError::internal("project page encoding failed"))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "project page exceeds bounded response size",
        ));
    }
    super::onboarding::audit(&mut tx,actor.id,None,"onboarding.project.snapshot",json!({"project_id":project,"device_id":current.device,"nonce":request.nonce,"binding_revision":receipt.revision,"after":request.after,"next":page.next})).await?;
    sync.verify_project_delivery_page(signed, &page_request)
        .map_err(|_| changed())?;
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_service_tokens WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>clock_timestamp()))")
        .bind(actor.token_id).bind(actor.id).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    if !live {
        return Err(ApiError::forbidden(
            "device credential expired before delivery commit",
        ));
    }
    if let Some(id) = approval {
        // Clock expiry is not protected by row locks. Recheck immediately before
        // this owner's delivery linearization point, including delegate expiry.
        super::company_device_delivery::approved(&state, &mut tx, &actor, id).await?;
    }
    tx.commit().await.map_err(ApiError::sql)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}

/// Authenticated control-plane bootstrap. Trust is never learned from a scoped
/// content page, and no gateway credential/endpoint is returned to the laptop.
pub(crate) async fn subscription(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(project): Path<Uuid>,
) -> Result<Json<codifix_federation_runtime::DeviceProjectSubscription>, ApiError> {
    subscription_for(state, actor, project, None).await
}

pub(super) async fn subscription_for(
    state: AppState,
    actor: AuthenticatedPrincipal,
    project: Uuid,
    approval: Option<Uuid>,
) -> Result<Json<codifix_federation_runtime::DeviceProjectSubscription>, ApiError> {
    let configured = state
        .federation
        .read()
        .map_err(|_| ApiError::service_unavailable("owner delivery unavailable"))?
        .clone()
        .ok_or_else(|| ApiError::forbidden("local owner federation instance is not enrolled"))?;
    let (sync, owner, space) = configured
        .project_delivery(project)
        .ok_or_else(|| ApiError::forbidden("project is not enrolled on this local owner"))?;
    let (known, _) = sync.verified_project_sources().await.map_err(|_| {
        ApiError::service_unavailable("verified local source publications unavailable")
    })?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let current = audience_eligibility(
        &state,
        &mut tx,
        &actor,
        project,
        (owner, space),
        &known,
        approval,
    )
    .await?;
    let (authority, receipt_keys) = sync.project_delivery_trust();
    let response = codifix_federation_runtime::DeviceProjectSubscription {
        schema_version: 1,
        device: current.device,
        scope: current.scope,
        catalog_revision: current.catalog,
        authority,
        receipt_keys,
    };
    response.validate().map_err(|_| {
        ApiError::service_unavailable("owner receipt trust configuration unavailable")
    })?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
}
fn validate_entries(
    sources: &CurrentProjectSources,
    complete: &[ProjectStreamReview],
    entries: &[codifix_federation_store::ProjectEntry],
) -> Result<(), ApiError> {
    for entry in entries {
        match &entry.event {
            ProjectEvent::Record {
                stream, delivery, ..
            } => {
                if sources.records.get(&delivery.envelope.key()) != Some(delivery)
                    || !sources.reviews.iter().any(|r| {
                        r.stream == *stream
                            && r.enabled
                            && r.records.contains(&delivery.envelope.key())
                    })
                {
                    return Err(changed());
                }
            }
            ProjectEvent::Review { review } => {
                if !complete.contains(review) {
                    return Err(changed());
                }
            }
            ProjectEvent::Withheld { stream } => {
                if !complete.iter().any(|r| r.stream == *stream) {
                    return Err(changed());
                }
            }
        }
    }
    Ok(())
}
