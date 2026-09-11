// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in local-human browser consent. Tokens/MCP cannot self-name destinations.
use super::*;
use axum::extract::Query;
use mem_federation::{ContentDigest, expertise::*};
use mem_federation_store::{QuestionResult, QuestionReview};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpertiseBrowseQuery {
    after_publisher: Option<InstanceId>,
    after_record: Option<RecordId>,
    limit: Option<u32>,
}
#[derive(Debug, Serialize)]
pub struct ExpertiseCatalog {
    pub capabilities: Vec<CapabilityAdvertisement>,
    pub next_after: Option<RecordKey>,
}
#[derive(Deserialize)]
pub(crate) struct ExpertiseResultPath {
    slug: String,
    space: SpaceId,
    id: Uuid,
}

pub(crate) async fn expertise_capabilities(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Query(query): Query<ExpertiseBrowseQuery>,
) -> Result<Json<ExpertiseCatalog>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    browser(&runtime, &principal)?;
    let limit = query.limit.unwrap_or(32);
    if !(1..=32).contains(&limit) {
        return Err(bad_request("capability limit must be 1 through 32"));
    }
    let after = match (query.after_publisher, query.after_record) {
        (None, None) => None,
        (Some(publisher), Some(id)) => Some(RecordKey { publisher, id }),
        _ => return Err(bad_request("both cursor identifiers are required")),
    };
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    mem_federation_store::authorize_expertise_browse(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
    )
    .await
    .map_err(store_error)?;
    let ads = runtime
        .sync
        .expertise_capabilities(after, limit)
        .await
        .map_err(runtime_error)?;
    let next_after = if ads.len() == limit as usize {
        ads.last().map(|ad| ad.key())
    } else {
        None
    };
    let capabilities = ads
        .into_iter()
        .filter(|ad| {
            ad.modes.contains(&AnswerMode::Deterministic)
                && ad.output_destinations.iter().any(|d| d == "local_human")
        })
        .collect();
    current_access(&state, &mut tx, &principal, &access).await?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(ExpertiseCatalog {
        capabilities,
        next_after,
    }))
}
pub(crate) async fn expertise_result(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<ExpertiseResultPath>,
) -> Result<Json<QuestionResult>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    browser(&runtime, &principal)?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let consent = mem_federation_store::retained_expertise_consent(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        path.id,
    )
    .await
    .map_err(store_error)?;
    let q = &consent.question;
    if q.mode != AnswerMode::Deterministic
        || q.model_destination.is_some()
        || q.output_destination != "local_human"
    {
        return Err(ApiError::forbidden("retained destination is not permitted"));
    }
    let ad = advertisement(&runtime, q.capability).await?;
    if mem_federation::CanonicalJsonCodec::digest(&ad).map_err(|_| unavailable())?
        != consent.advertisement_digest
    {
        return Err(ApiError::forbidden("retained capability review changed"));
    }
    // Execution expiry does not erase a completed result. Gateway payload TTL,
    // current policy and fresh origin approval still govern every delivery.
    ad.authorize_question(q, q.created_at)
        .map_err(|_| ApiError::forbidden("retained capability consent denied"))?;
    let result = runtime
        .sync
        .expertise_result(q)
        .await
        .map_err(runtime_error)?;
    // TTL/session may expire while waiting for the answering origin.
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    mem_federation_store::retained_expertise_consent(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        path.id,
    )
    .await
    .map_err(store_error)?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertisePrepareRequest {
    pub request_id: Uuid,
    pub capability: RecordKey,
    pub question: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseSubmitRequest {
    pub review: QuestionReview,
    pub confirmed_digest: ContentDigest,
}
fn browser(runtime: &ConfiguredInstance, p: &AuthenticatedPrincipal) -> Result<(), ApiError> {
    if !runtime.expertise_browser
        || p.credential_source != CredentialSource::BrowserSession
        || p.session_id.is_none()
    {
        return Err(ApiError::forbidden(
            "expertise requires enabled local-human browser policy",
        ));
    }
    Ok(())
}
async fn current_access(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &AuthenticatedPrincipal,
    access: &LocalProjectAccess,
) -> Result<LocalProjectAccess, ApiError> {
    sqlx::query("SET LOCAL lock_timeout='2s'")
        .execute(&mut **tx)
        .await
        .map_err(|_| unavailable())?;
    sqlx::query("SET LOCAL statement_timeout='3s'")
        .execute(&mut **tx)
        .await
        .map_err(|_| unavailable())?;
    let current = crate::auth::background_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Credential {
            principal: p.id,
            token: None,
            session: p.session_id,
        },
        access.project,
    )
    .await?;
    for permission in [Permission::MemoryRead, Permission::BundleExport] {
        if !current.contains(permission) || !access.permissions.contains(permission) {
            return Err(ApiError::forbidden(
                "current project read/export permission required",
            ));
        }
    }
    Ok(LocalProjectAccess {
        permissions: mem_record::PermissionSet::from_permissions(&[
            Permission::MemoryRead,
            Permission::BundleExport,
        ]),
        ..access.clone()
    })
}
async fn advertisement(
    runtime: &ConfiguredInstance,
    key: RecordKey,
) -> Result<CapabilityAdvertisement, ApiError> {
    // Verified retained history, explicitly not a live authorization assertion.
    // Gateway submission rechecks the current revision and membership.
    runtime
        .sync
        .verified_snapshot()
        .await
        .map_err(runtime_error)?
        .expertise(key)
        .cloned()
        .ok_or_else(|| ApiError::not_found("capability unavailable; synchronize first"))
}
pub(crate) async fn expertise_prepare(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<ExpertisePrepareRequest>,
) -> Result<Json<QuestionReview>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    browser(&runtime, &principal)?;
    let ad = advertisement(&runtime, request.capability).await?;
    let now = chrono::Utc::now();
    let question = ExpertiseQuestion {
        request_id: request.request_id,
        space: runtime.space,
        requester: runtime.instance,
        principal: principal.id,
        capability: ad.key(),
        revision: ad.revision,
        collection_digest: ad.collection_digest.clone(),
        question: request.question,
        mode: AnswerMode::Deterministic,
        model_destination: None,
        output_destination: "local_human".into(),
        created_at: now,
        expires_at: now + chrono::Duration::seconds(120),
    };
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let review = mem_federation_store::prepare_expertise_question(
        &mut tx,
        &current,
        &question,
        &ad,
        &["local_human".into()],
    )
    .await
    .map_err(store_error)?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(review))
}
pub(crate) async fn expertise_submit(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<ExpertiseSubmitRequest>,
) -> Result<Json<QuestionResult>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    browser(&runtime, &principal)?;
    let q = &request.review.question;
    if q.space != runtime.space
        || q.requester != runtime.instance
        || q.principal != principal.id
        || q.mode != AnswerMode::Deterministic
        || q.model_destination.is_some()
        || q.output_destination != "local_human"
    {
        return Err(ApiError::forbidden(
            "question identity or destination does not match authenticated scope",
        ));
    }
    let ad = advertisement(&runtime, q.capability).await?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    mem_federation_store::confirm_expertise_question(
        &mut tx,
        &current,
        &request.review,
        &ad,
        &["local_human".into()],
        &request.confirmed_digest,
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    // Never send uncommitted consent. Network errors leave the exact request
    // recoverable; retrying reuses its ID rather than inventing a new question.
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let question = mem_federation_store::consented_expertise_question(
        &mut tx,
        &current,
        runtime.space,
        runtime.instance,
        q.request_id,
        &ad,
        &["local_human".into()],
    )
    .await
    .map_err(store_error)?;
    let result = runtime
        .sync
        .submit_expertise_question(question)
        .await
        .map_err(runtime_error)?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(result))
}
