// SPDX-License-Identifier: AGPL-3.0-or-later
//! Origin-local exact review; only advertisements enter the publication outbox.
use super::*;
use mem_federation::{ContentDigest, expertise::AnswerMode};
use mem_federation_store::{CollectionBlueprint, CollectionDraft, CollectionWithdrawal};

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
    modes: &[AnswerMode],
    models: &[String],
    outputs: &[String],
) -> Result<(), ApiError> {
    if modes != [AnswerMode::Deterministic] || !models.is_empty() || outputs != ["local_human"] {
        return Err(ApiError::forbidden(
            "collection browser policy permits deterministic local_human answers only",
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
    destinations(&ad.modes, &ad.model_destinations, &ad.output_destinations)?;
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
