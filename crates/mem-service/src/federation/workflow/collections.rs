// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-local exact review; only advertisements enter the publication outbox.
use super::*;
use axum::extract::Query;
use mem_federation::{ContentDigest, expertise::AnswerMode};
use mem_federation_store::{CollectionBlueprint, CollectionDraft, CollectionWithdrawal};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CollectionQuery {
    after: Option<RecordId>,
    limit: Option<u32>,
}
#[derive(Deserialize)]
pub(crate) struct CollectionPath {
    slug: String,
    space: SpaceId,
    collection: RecordId,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionGrantRequest {
    pub principal: Uuid,
    pub enabled: bool,
    pub confirmed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantQuery {
    principal: Uuid,
}
#[derive(Serialize)]
pub struct CollectionGrantState {
    pub principal: Uuid,
    pub enabled: Option<bool>,
}
pub(crate) async fn collection_grant_state(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<CollectionPath>,
    Query(query): Query<GrantQuery>,
) -> Result<Json<CollectionGrantState>, ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = requester::current_access_for(
        &state,
        &mut tx,
        &principal,
        &local,
        &[Permission::MemoryRead, Permission::AuthManage],
    )
    .await?;
    let enabled = mem_federation_store::collection_grant_state(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        path.collection,
        query.principal,
    )
    .await
    .map_err(store_error)?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(CollectionGrantState {
        principal: query.principal,
        enabled,
    }))
}

/// Does not create an identity, configure a worker, or confer remote authority.
pub(crate) async fn collection_grant(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<CollectionPath>,
    Json(request): Json<CollectionGrantRequest>,
) -> Result<Json<CollectionGrantRequest>, ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    if !request.confirmed || request.principal.is_nil() {
        return Err(bad_request(
            "explicit collection grant confirmation required",
        ));
    }
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = requester::current_access_for(
        &state,
        &mut tx,
        &principal,
        &local,
        &[Permission::MemoryRead, Permission::AuthManage],
    )
    .await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_expertise_collections WHERE space=$1 AND owner=$2 AND project_id=$3 AND collection=$4)")
        .bind(runtime.space.as_uuid()).bind(runtime.instance.as_uuid()).bind(runtime.project).bind(path.collection.as_uuid())
        .fetch_one(&mut *tx).await.map_err(|_| unavailable())?;
    if !exists {
        return Err(ApiError::not_found("collection unavailable"));
    }
    if request.enabled {
        super::super::expertise::access(&state, &mut tx, request.principal, runtime.project)
            .await
            .map_err(|_| {
                ApiError::forbidden("active internal expertise service authority required")
            })?;
    }
    mem_federation_store::set_collection_grant(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        path.collection,
        request.principal,
        request.enabled,
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(Json(request))
}

pub(crate) async fn collections(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Query(query): Query<CollectionQuery>,
) -> Result<Json<mem_federation_store::CollectionPage>, ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    let limit = query.limit.unwrap_or(32);
    if !(1..=32).contains(&limit) {
        return Err(bad_request("collection limit must be 1 through 32"));
    }
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = access(&state, &mut tx, &principal, &local).await?;
    let page = mem_federation_store::collection_page(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        query.after,
        limit,
    )
    .await
    .map_err(store_error)?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(page))
}

pub(crate) async fn collection_detail(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<CollectionPath>,
) -> Result<Json<mem_federation_store::CollectionDetail>, ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = access(&state, &mut tx, &principal, &local).await?;
    let detail = mem_federation_store::collection_detail(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        path.collection,
    )
    .await
    .map_err(store_error)?
    .ok_or_else(|| ApiError::not_found("collection unavailable"))?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionActivationRequest {
    pub draft: CollectionDraft,
    pub confirmed_digest: ContentDigest,
}

#[derive(Debug, Serialize)]
pub struct CollectionSubmission {
    pub request_id: Uuid,
    pub submission: &'static str,
}

fn destinations(
    runtime: &ConfiguredInstance,
    modes: &[AnswerMode],
    models: &[String],
    outputs: &[String],
) -> Result<(), ApiError> {
    if (modes.contains(&AnswerMode::Model) && runtime.expertise_model.is_none())
        || models.iter().any(|d| {
            runtime
                .expertise_model
                .as_ref()
                .is_none_or(|c| c.provider != *d)
        })
        || outputs.is_empty()
        || outputs
            .iter()
            .any(|d| !runtime.expertise_output_destinations.contains(d))
    {
        return Err(ApiError::forbidden(
            "collection policy permits only configured model and output destinations",
        ));
    }
    Ok(())
}

async fn access(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal: &AuthenticatedPrincipal,
    local: &LocalProjectAccess,
) -> Result<LocalProjectAccess, ApiError> {
    requester::current_access_for(
        state,
        tx,
        principal,
        local,
        &[
            Permission::MemoryRead,
            Permission::MemoryCurate,
            Permission::BundleExport,
        ],
    )
    .await
}

pub(crate) async fn collection_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(blueprint): Json<CollectionBlueprint>,
) -> Result<Json<CollectionDraft>, ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    destinations(
        &runtime,
        &blueprint.modes,
        &blueprint.model_destinations,
        &blueprint.output_destinations,
    )?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = access(&state, &mut tx, &principal, &local).await?;
    let draft = mem_federation_store::preview_collection_in(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        &blueprint,
    )
    .await
    .map_err(store_error)?;
    // No standing permission, local persistence or remote transmission yet.
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(draft))
}

pub(crate) async fn collection_activate(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<CollectionActivationRequest>,
) -> Result<(StatusCode, Json<CollectionSubmission>), ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    let ad = &request.draft.advertisement;
    destinations(
        &runtime,
        &ad.modes,
        &ad.model_destinations,
        &ad.output_destinations,
    )?;
    if request.confirmed_digest != request.draft.review_digest {
        return Err(bad_request(
            "exact collection review digest confirmation required",
        ));
    }
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = access(&state, &mut tx, &principal, &local).await?;
    mem_federation_store::activate_collection(
        &mut tx,
        &known,
        &current,
        runtime.instance,
        &request.draft,
        true,
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok((
        StatusCode::ACCEPTED,
        Json(CollectionSubmission {
            request_id: request.draft.request_id,
            submission: "queued",
        }),
    ))
}

pub(crate) async fn collection_withdraw(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<CollectionWithdrawal>,
) -> Result<(StatusCode, Json<CollectionSubmission>), ApiError> {
    let (runtime, local) = binding(&state, &principal, &path.slug, path.space).await?;
    requester::browser(&runtime, &principal)?;
    let known = runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = access(&state, &mut tx, &principal, &local).await?;
    mem_federation_store::withdraw_collection(
        &mut tx,
        &known,
        &current,
        runtime.instance,
        &request,
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok((
        StatusCode::ACCEPTED,
        Json(CollectionSubmission {
            request_id: request.request_id,
            submission: "queued",
        }),
    ))
}
