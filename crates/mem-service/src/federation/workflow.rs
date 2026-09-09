// SPDX-License-Identifier: AGPL-3.0-or-later
//! Experimental local HTTP contracts shared by future Web/CLI/TUI clients.
//! Authentication/project permission is centralized; these handlers additionally
//! enforce explicit federation grants and administrator-configured space binding.
use super::ConfiguredInstance;
use crate::{
    ApiError, AppState,
    auth::{AuthenticatedPrincipal, CredentialSource},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use mem_federation::{SpaceCapability, publication::*};
use mem_federation_runtime::{RuntimeError, UntrustedContext};
pub use mem_federation_store::{EvidenceSelection, ShareReviewRequest};
use mem_federation_store::{LocalWorkflowStatus, SourceSelection, StoreError};
use mem_record::Permission;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub(crate) struct SpacePath {
    slug: String,
    space: SpaceId,
}
#[derive(Deserialize)]
pub(crate) struct PreviewPath {
    slug: String,
    space: SpaceId,
    canonical: RecordId,
}

#[derive(Debug, Serialize)]
pub struct SpaceStatus {
    pub project: Uuid,
    pub space: SpaceId,
    pub instance: InstanceId,
    pub authority: InstanceId,
    pub retained_through: u32,
    /// Membership in the retained signed prefix, not a live authorization claim.
    pub observed_capabilities: Vec<SpaceCapability>,
    pub local: LocalWorkflowStatus,
    pub offline_copies_may_remain: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalGrantRequest {
    pub principal: Uuid,
    pub capabilities: Vec<SpaceCapability>,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct PublicationPreview {
    pub space: SpaceId,
    pub stream: StreamId,
    pub selection: SourceSelection,
    pub requires_future_update_consent: bool,
}

#[derive(Debug, Serialize)]
pub struct ShareReviewResponse {
    pub request_id: Uuid,
    pub review_id: Uuid,
    /// This is the result of local submission, not the latest delivery status.
    pub submission: &'static str,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedContextRequest {
    pub streams: Vec<StreamId>,
}

async fn binding(
    state: &AppState,
    principal: &AuthenticatedPrincipal,
    slug: &str,
    space: SpaceId,
) -> Result<(Arc<ConfiguredInstance>, LocalProjectAccess), ApiError> {
    // The legacy anonymous local reader is not an authenticated sharing actor.
    if principal.credential_source == CredentialSource::AnonymousSingleUser {
        return Err(ApiError::unauthorized(
            "federation requires an authenticated local principal",
        ));
    }
    let runtime = state
        .federation
        .read()
        .expect("federation registry poisoned")
        .clone()
        .ok_or_else(|| ApiError::not_found("federation is not configured"))?;
    let project: Option<Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(slug)
        .fetch_optional(&state.pool()?)
        .await
        .map_err(|_| unavailable())?;
    if runtime.space != space || project != Some(runtime.project) {
        return Err(ApiError::not_found(
            "federation space is not configured for this project",
        ));
    }
    // Use the authenticated request's exact-project permissions, including token
    // scope. Never accept principal/project/access/publisher from request JSON.
    let access = LocalProjectAccess {
        principal: principal.id,
        project: runtime.project,
        permissions: principal.permissions_for_project(slug),
        read_only: state.config.service.read_only,
    };
    if !access.permissions.contains(Permission::MemoryRead) {
        return Err(ApiError::forbidden("local memory read permission required"));
    }
    Ok((runtime, access))
}

async fn require_pull(
    state: &AppState,
    runtime: &ConfiguredInstance,
    access: &LocalProjectAccess,
) -> Result<(), ApiError> {
    mem_federation_store::authorize_source_pull(
        &state.pool()?,
        access,
        runtime.space,
        runtime.instance,
    )
    .await
    .map_err(|error| match error {
        StoreError::Binding => ApiError::forbidden("local pull grant required"),
        other => store_error(other),
    })
}

pub(crate) async fn status(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
) -> Result<Json<SpaceStatus>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let local = mem_federation_store::local_workflow_status(
        &state.pool()?,
        &access,
        runtime.space,
        runtime.instance,
        known.sequence(),
    )
    .await
    .map_err(store_error)?;
    Ok(Json(SpaceStatus {
        project: runtime.project,
        space: runtime.space,
        instance: runtime.instance,
        authority: runtime.authority,
        retained_through: known.sequence(),
        observed_capabilities: [
            SpaceCapability::Pull,
            SpaceCapability::Publish,
            SpaceCapability::ManageMembership,
        ]
        .into_iter()
        .filter(|c| known.has_capability(runtime.instance, *c))
        .collect(),
        local,
        offline_copies_may_remain: true,
    }))
}

pub(crate) async fn grant(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<LocalGrantRequest>,
) -> Result<Json<LocalGrantRequest>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    if request.principal.is_nil()
        || request.capabilities.len() > 3
        || request
            .capabilities
            .iter()
            .enumerate()
            .any(|(i, c)| request.capabilities[..i].contains(c))
    {
        return Err(bad_request("invalid local grant"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(|_| unavailable())?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1 AND disabled_at IS NULL)",
    )
    .bind(request.principal)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    if !exists {
        return Err(bad_request(
            "grant target must be an active local principal",
        ));
    }
    mem_federation_store::set_local_grant(
        &mut tx,
        &access,
        &LocalSpaceGrant {
            principal: request.principal,
            project: runtime.project,
            space: runtime.space,
            instance: runtime.instance,
            capabilities: request.capabilities.clone(),
        },
        request.enabled,
    )
    .await
    .map_err(store_error)?;
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,project_id,target_principal_id,metadata_json) VALUES($1,$2,'federation.local_grant','success',$3,$4,$5)")
        .bind(Uuid::new_v4()).bind(access.principal).bind(runtime.project).bind(request.principal)
        .bind(serde_json::json!({"space":runtime.space,"instance":runtime.instance,"enabled":request.enabled,"capabilities":request.capabilities}))
        .execute(&mut *tx).await.map_err(|_| unavailable())?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(Json(request))
}

pub(crate) async fn preview(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<PreviewPath>,
) -> Result<Json<PublicationPreview>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let selection =
        mem_federation_store::preview_source(&state.pool()?, runtime.project, path.canonical)
            .await
            .map_err(store_error)?;
    Ok(Json(PublicationPreview {
        space: runtime.space,
        stream: StreamId {
            origin: runtime.instance,
            canonical: path.canonical,
        },
        selection,
        requires_future_update_consent: true,
    }))
}

pub(crate) async fn review(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<ShareReviewRequest>,
) -> Result<(StatusCode, Json<ShareReviewResponse>), ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    require_pull(&state, &runtime, &access).await?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let review_id = mem_federation_store::submit_share_review(
        &state.pool()?,
        &known,
        &access,
        runtime.instance,
        &request,
    )
    .await
    .map_err(store_error)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ShareReviewResponse {
            request_id: request.request_id,
            review_id,
            submission: "queued",
        }),
    ))
}

pub(crate) async fn context(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<SharedContextRequest>,
) -> Result<Json<UntrustedContext>, ApiError> {
    if request.streams.is_empty() || request.streams.len() > 16 {
        return Err(bad_request("select between one and sixteen shared streams"));
    }
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    Ok(Json(
        runtime
            .sync
            .retained_context(&access, &request.streams)
            .await
            .map_err(runtime_error)?,
    ))
}

fn bad_request(message: &str) -> ApiError {
    ApiError::status_message(StatusCode::BAD_REQUEST, message)
}
fn unavailable() -> ApiError {
    ApiError::service_unavailable("federation state is unavailable or failed verification")
}
fn store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::LocalPolicy(_) => {
            ApiError::forbidden("local sharing grant or publication permission denied")
        }
        StoreError::Binding => ApiError::status_message(
            StatusCode::CONFLICT,
            "review, source or sharing binding changed; refresh before retrying",
        ),
        StoreError::Publication(_) | StoreError::Limits => {
            bad_request("invalid federation review or request limits")
        }
        StoreError::Database(sqlx::Error::RowNotFound) => {
            ApiError::not_found("federation source not found in this project")
        }
        _ => unavailable(),
    }
}
fn runtime_error(error: RuntimeError) -> ApiError {
    match error {
        RuntimeError::Unauthorized => {
            ApiError::forbidden("shared material is not available to this principal")
        }
        RuntimeError::Configuration => bad_request("invalid shared selection"),
        RuntimeError::Store(error) => store_error(error),
        _ => unavailable(),
    }
}

pub(crate) async fn private_response(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let private = request.uri().path().split('/').nth(4) == Some("federation");
    let mut response = next.run(request).await;
    if private {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
    }
    response
}

#[cfg(test)]
mod tests;
