// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operator-bound caller consent. Tokens/MCP cannot self-name destinations.
use super::*;
use axum::extract::Query;
use mem_federation::{ContentDigest, expertise::*};
use mem_federation_store::{QuestionResult, QuestionReview};
use sqlx::{Row, types::Json as SqlJson};

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
#[derive(Deserialize)]
pub(crate) struct ExpertiseCapabilityPath {
    slug: String,
    space: SpaceId,
    publisher: InstanceId,
    record: RecordId,
}
pub(crate) async fn expertise_capability(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<ExpertiseCapabilityPath>,
    headers: axum::http::HeaderMap,
) -> Result<Json<CapabilityAdvertisement>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let destination = caller_destination(&runtime, &principal, &headers)?;
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
    let ad = advertisement(
        &runtime,
        RecordKey {
            publisher: path.publisher,
            id: path.record,
        },
    )
    .await?;
    if !ad.output_destinations.iter().any(|d| d == destination)
        || (!ad.modes.contains(&AnswerMode::Deterministic)
            && !ad
                .model_destinations
                .iter()
                .any(|d| runtime.expertise_model_destinations.contains(d)))
    {
        return Err(ApiError::forbidden("capability destination denied"));
    }
    current_access(&state, &mut tx, &principal, &access).await?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(ad))
}

pub(crate) async fn expertise_capabilities(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Query(query): Query<ExpertiseBrowseQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ExpertiseCatalog>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let destination = caller_destination(&runtime, &principal, &headers)?;
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
            (ad.modes.contains(&AnswerMode::Deterministic)
                || ad
                    .model_destinations
                    .iter()
                    .any(|d| runtime.expertise_model_destinations.contains(d)))
                && ad.output_destinations.iter().any(|d| d == destination)
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
    headers: axum::http::HeaderMap,
) -> Result<Json<QuestionResult>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let destination = caller_destination(&runtime, &principal, &headers)?;
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
    if q.model_destination
        .as_ref()
        .is_some_and(|d| !runtime.expertise_model_destinations.contains(d))
        || q.output_destination != destination
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
    #[serde(default)]
    pub model_destination: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseSubmitRequest {
    pub review: QuestionReview,
    pub confirmed_digest: ContentDigest,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentAskRequest {
    pub request_id: Uuid,
    pub capability: RecordKey,
    pub question: String,
}

/// A watched agent gets only this preconfigured, exact-token path. It cannot
/// assert a human confirmation or choose its onward output destination.
#[axum::debug_handler]
pub(crate) async fn expertise_agent_ask(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
    Json(request): Json<AgentAskRequest>,
) -> Result<Json<mem_federation_store::AddressedReceipt>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let caller = runtime
        .expertise_callers
        .iter()
        .find(|caller| {
            principal.credential_source == CredentialSource::ServiceToken
                && principal.session_id.is_none()
                && caller.principal == principal.id
                && Some(caller.token) == principal.token_id
                && caller.agent_ask.is_some()
        })
        .ok_or_else(|| ApiError::forbidden("watched-agent expertise policy unavailable"))?;
    let policy = caller
        .agent_ask
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("agent policy unavailable"))?;
    if !policy.allowed_capabilities.contains(&request.capability)
        || request.request_id.is_nil()
        || request.question.trim().is_empty()
        || request.question.len() > MAX_QUESTION_BYTES
    {
        return Err(ApiError::forbidden(
            "question outside standing agent policy",
        ));
    }
    let ad = advertisement(&runtime, request.capability).await?;
    let policy_digest = mem_federation::CanonicalJsonCodec::digest(&(
        policy,
        &caller.output_destination,
        caller.token,
    ))
    .map_err(|_| unavailable())?;
    let input_digest = mem_federation::CanonicalJsonCodec::digest(&request)
        .map_err(|_| bad_request("invalid question"))?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let prior = sqlx::query("SELECT principal,token_id,input_digest,policy_digest,record FROM federation_agent_expertise_intents WHERE project_id=$1 AND request_id=$2 FOR UPDATE")
        .bind(runtime.project).bind(request.request_id)
        .fetch_optional(&mut *tx).await.map_err(|_| unavailable())?;
    let record: AddressedExpertiseRecord = if let Some(row) = prior {
        if row
            .try_get::<Uuid, _>("principal")
            .map_err(|_| unavailable())?
            != principal.id
            || row
                .try_get::<Uuid, _>("token_id")
                .map_err(|_| unavailable())?
                != caller.token
            || row
                .try_get::<SqlJson<ContentDigest>, _>("input_digest")
                .map_err(|_| unavailable())?
                .0
                != input_digest
            || row
                .try_get::<SqlJson<ContentDigest>, _>("policy_digest")
                .map_err(|_| unavailable())?
                .0
                != policy_digest
        {
            return Err(ApiError::forbidden("request ID or policy binding changed"));
        }
        row.try_get::<SqlJson<AddressedExpertiseRecord>, _>("record")
            .map_err(|_| unavailable())?
            .0
    } else {
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
            output_destination: caller.output_destination.clone(),
            created_at: now,
            expires_at: now + chrono::Duration::seconds(120),
        };
        let record =
            AddressedExpertiseRecord::question(question, policy.revision, policy_digest.clone())
                .map_err(|_| bad_request("invalid addressed question"))?;
        let day = now.date_naive();
        sqlx::query("INSERT INTO federation_agent_expertise_daily(project_id,token_id,utc_day,used) VALUES($1,$2,$3,0) ON CONFLICT DO NOTHING")
            .bind(runtime.project).bind(caller.token).bind(day).execute(&mut *tx).await.map_err(|_| unavailable())?;
        let slot = sqlx::query("UPDATE federation_agent_expertise_daily SET used=used+1 WHERE project_id=$1 AND token_id=$2 AND utc_day=$3 AND used<$4 RETURNING used")
            .bind(runtime.project).bind(caller.token).bind(day).bind(i16::try_from(policy.daily_limit).map_err(|_| unavailable())?)
            .fetch_optional(&mut *tx).await.map_err(|_| unavailable())?;
        if slot.is_none() {
            return Err(ApiError::forbidden("daily agent question limit reached"));
        }
        sqlx::query("INSERT INTO federation_agent_expertise_intents(project_id,principal,token_id,space,request_id,input_digest,policy_digest,record) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(runtime.project).bind(principal.id).bind(caller.token).bind(runtime.space.as_uuid()).bind(request.request_id)
            .bind(SqlJson(&input_digest)).bind(SqlJson(&policy_digest)).bind(SqlJson(&record))
            .execute(&mut *tx).await.map_err(|_| unavailable())?;
        record
    };
    let AddressedExpertisePayload::Question(question) = &record.payload else {
        return Err(unavailable());
    };
    if question.expires_at <= chrono::Utc::now()
        || record.policy_digest != policy_digest
        || question.output_destination != caller.output_destination
        || question.mode != AnswerMode::Deterministic
    {
        return Err(ApiError::forbidden(
            "agent question expired or policy changed",
        ));
    }
    mem_federation_store::prepare_expertise_question_with_models(
        &mut tx,
        &current,
        question,
        &ad,
        std::slice::from_ref(&caller.output_destination),
        &[],
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    // The durable intent is committed before transmission. A fresh permission
    // lock is held while the gateway accepts it, so a concurrent local revoke
    // cannot slip between the last check and the network release.
    let mut release_tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let release_access = current_access(&state, &mut release_tx, &principal, &access).await?;
    mem_federation_store::prepare_expertise_question_with_models(
        &mut release_tx,
        &release_access,
        question,
        &ad,
        std::slice::from_ref(&caller.output_destination),
        &[],
    )
    .await
    .map_err(store_error)?;
    let receipt = runtime
        .sync
        .submit_addressed_expertise(record)
        .await
        .map_err(runtime_error)?;
    release_tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(receipt))
}

#[derive(Debug, Serialize)]
pub struct AgentQuestionStatus {
    pub request_id: Uuid,
    pub question: Option<mem_federation_store::AddressedReceipt>,
    pub answer: Option<ExpertiseAnswer>,
    pub answer_receipt: Option<mem_federation_store::AddressedReceipt>,
}

#[derive(Debug, Serialize)]
pub struct AgentExchangeSummary {
    pub request_id: Uuid,
    pub recipient: InstanceId,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// None means gateway status is unavailable or denied; never healthy.
    pub question_status: Option<mem_federation_store::AddressedStatus>,
    pub answer_status: Option<mem_federation_store::AddressedStatus>,
}

pub(crate) async fn expertise_agent_exchanges(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<SpacePath>,
) -> Result<Json<Vec<AgentExchangeSummary>>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    current_access(&state, &mut tx, &principal, &access).await?;
    let rows = sqlx::query("SELECT request_id,created_at,record FROM federation_agent_expertise_intents WHERE project_id=$1 AND space=$2 ORDER BY created_at DESC,request_id DESC LIMIT 20")
        .bind(runtime.project).bind(runtime.space.as_uuid()).fetch_all(&mut *tx).await.map_err(|_| unavailable())?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let request_id: Uuid = row.try_get("request_id").map_err(|_| unavailable())?;
        let record = row
            .try_get::<SqlJson<AddressedExpertiseRecord>, _>("record")
            .map_err(|_| unavailable())?
            .0;
        items.push(AgentExchangeSummary {
            request_id,
            recipient: record.recipient,
            created_at: row.try_get("created_at").map_err(|_| unavailable())?,
            question_status: None,
            answer_status: None,
        });
    }
    tx.rollback().await.map_err(|_| unavailable())?;
    for item in &mut items {
        item.question_status = runtime
            .sync
            .addressed_expertise_result(item.request_id, "question")
            .await
            .ok()
            .flatten()
            .map(|delivery| delivery.receipt.status);
        item.answer_status = runtime
            .sync
            .addressed_expertise_result(item.request_id, "answer")
            .await
            .ok()
            .flatten()
            .map(|delivery| delivery.receipt.status);
    }
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    current_access(&state, &mut tx, &principal, &access).await?;
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(items))
}

pub(crate) async fn expertise_agent_result(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path(path): Path<ExpertiseResultPath>,
) -> Result<Json<AgentQuestionStatus>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let caller = runtime
        .expertise_callers
        .iter()
        .find(|caller| {
            principal.credential_source == CredentialSource::ServiceToken
                && principal.session_id.is_none()
                && caller.principal == principal.id
                && Some(caller.token) == principal.token_id
                && caller.agent_ask.is_some()
        })
        .ok_or_else(|| ApiError::forbidden("watched-agent expertise policy unavailable"))?;
    let policy = caller
        .agent_ask
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("agent policy unavailable"))?;
    let digest = mem_federation::CanonicalJsonCodec::digest(&(
        policy,
        &caller.output_destination,
        caller.token,
    ))
    .map_err(|_| unavailable())?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let row = sqlx::query("SELECT principal,token_id,policy_digest,record FROM federation_agent_expertise_intents WHERE project_id=$1 AND request_id=$2 FOR SHARE")
        .bind(runtime.project).bind(path.id).fetch_optional(&mut *tx).await.map_err(|_| unavailable())?
        .ok_or_else(|| ApiError::not_found("agent question not found"))?;
    if row
        .try_get::<Uuid, _>("principal")
        .map_err(|_| unavailable())?
        != principal.id
        || row
            .try_get::<Uuid, _>("token_id")
            .map_err(|_| unavailable())?
            != caller.token
        || row
            .try_get::<SqlJson<ContentDigest>, _>("policy_digest")
            .map_err(|_| unavailable())?
            .0
            != digest
    {
        return Err(ApiError::forbidden("agent question policy changed"));
    }
    let record = row
        .try_get::<SqlJson<AddressedExpertiseRecord>, _>("record")
        .map_err(|_| unavailable())?
        .0;
    let AddressedExpertisePayload::Question(q) = &record.payload else {
        return Err(unavailable());
    };
    if !policy.allowed_capabilities.contains(&q.capability)
        || q.output_destination != caller.output_destination
        || q.mode != AnswerMode::Deterministic
    {
        return Err(ApiError::forbidden("agent question outside current policy"));
    }
    let ad = advertisement(&runtime, q.capability).await?;
    // A past deadline does not grant new authority. Check current local grant,
    // destination and collection revision without resending the question.
    let mut review_q = q.clone();
    review_q.created_at = chrono::Utc::now();
    review_q.expires_at = review_q.created_at + chrono::Duration::seconds(120);
    mem_federation_store::prepare_expertise_question_with_models(
        &mut tx,
        &current,
        &review_q,
        &ad,
        std::slice::from_ref(&caller.output_destination),
        &[],
    )
    .await
    .map_err(store_error)?;
    let question = runtime
        .sync
        .addressed_expertise_result(path.id, "question")
        .await
        .map_err(runtime_error)?;
    let delivered = runtime
        .sync
        .addressed_expertise_result(path.id, "answer")
        .await
        .map_err(runtime_error)?;
    let answer = delivered
        .as_ref()
        .and_then(|delivery| delivery.record.as_ref())
        .and_then(|record| match &record.payload {
            AddressedExpertisePayload::Answer(answer) => Some(answer.clone()),
            _ => None,
        });
    let question_digest = q.digest().map_err(|_| unavailable())?;
    if answer.as_ref().is_some_and(|answer| {
        answer.request_digest != question_digest || answer.validate(q, &answer.citations).is_err()
    }) {
        return Err(ApiError::forbidden("invalid addressed answer"));
    }
    tx.rollback().await.map_err(|_| unavailable())?;
    Ok(Json(AgentQuestionStatus {
        request_id: path.id,
        question: question.map(|delivery| delivery.receipt),
        answer,
        answer_receipt: delivered.map(|delivery| delivery.receipt),
    }))
}
pub(super) fn browser(
    runtime: &ConfiguredInstance,
    p: &AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    if output_destination(runtime, p)? != "local_human" {
        return Err(ApiError::forbidden(
            "collection administration requires local-human output policy",
        ));
    }
    Ok(())
}
fn output_destination<'a>(
    runtime: &'a ConfiguredInstance,
    p: &AuthenticatedPrincipal,
) -> Result<&'a str, ApiError> {
    if runtime.expertise_browser
        && p.credential_source == CredentialSource::BrowserSession
        && p.session_id.is_some()
    {
        return Ok("local_human");
    }
    if p.credential_source == CredentialSource::ServiceToken
        && p.session_id.is_none()
        && let Some(caller) = runtime
            .expertise_callers
            .iter()
            .find(|c| c.principal == p.id && Some(c.token) == p.token_id)
    {
        return Ok(&caller.output_destination);
    }
    Err(ApiError::forbidden(
        "expertise caller has no operator-bound output policy",
    ))
}
fn caller_destination<'a>(
    runtime: &'a ConfiguredInstance,
    p: &AuthenticatedPrincipal,
    headers: &axum::http::HeaderMap,
) -> Result<&'a str, ApiError> {
    let destination = output_destination(runtime, p)?;
    // The bridge declares onward model disclosure. This marker only restricts
    // access; it cannot select a destination or grant any authority.
    if headers.contains_key("x-memory-expertise-mcp") && destination == "local_human" {
        return Err(ApiError::forbidden(
            "MCP expertise requires an operator-bound model output destination",
        ));
    }
    Ok(destination)
}
async fn current_access(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &AuthenticatedPrincipal,
    access: &LocalProjectAccess,
) -> Result<LocalProjectAccess, ApiError> {
    current_access_for(
        state,
        tx,
        p,
        access,
        &[Permission::MemoryRead, Permission::BundleExport],
    )
    .await
}
pub(super) async fn current_access_for(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &AuthenticatedPrincipal,
    access: &LocalProjectAccess,
    required: &[Permission],
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
            token: p.token_id,
            session: p.session_id,
        },
        access.project,
    )
    .await?;
    for permission in required {
        if !current.contains(*permission) || !access.permissions.contains(*permission) {
            return Err(ApiError::forbidden(
                "required current project permissions are unavailable",
            ));
        }
    }
    Ok(LocalProjectAccess {
        permissions: mem_record::PermissionSet::from_permissions(required),
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
    headers: axum::http::HeaderMap,
    Json(request): Json<ExpertisePrepareRequest>,
) -> Result<Json<QuestionReview>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let destination = caller_destination(&runtime, &principal, &headers)?;
    let ad = advertisement(&runtime, request.capability).await?;
    if request
        .model_destination
        .as_ref()
        .is_some_and(|d| !runtime.expertise_model_destinations.contains(d))
    {
        return Err(ApiError::forbidden(
            "model destination is not operator-authorized",
        ));
    }
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
        mode: if request.model_destination.is_some() {
            AnswerMode::Model
        } else {
            AnswerMode::Deterministic
        },
        model_destination: request.model_destination,
        output_destination: destination.into(),
        created_at: now,
        expires_at: now + chrono::Duration::seconds(120),
    };
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let review = mem_federation_store::prepare_expertise_question_with_models(
        &mut tx,
        &current,
        &question,
        &ad,
        &[destination.into()],
        &runtime.expertise_model_destinations,
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
    headers: axum::http::HeaderMap,
    Json(request): Json<ExpertiseSubmitRequest>,
) -> Result<Json<QuestionResult>, ApiError> {
    let (runtime, access) = binding(&state, &principal, &path.slug, path.space).await?;
    let destination = caller_destination(&runtime, &principal, &headers)?;
    let q = &request.review.question;
    if q.space != runtime.space
        || q.requester != runtime.instance
        || q.principal != principal.id
        || q.model_destination
            .as_ref()
            .is_some_and(|d| !runtime.expertise_model_destinations.contains(d))
        || q.output_destination != destination
    {
        return Err(ApiError::forbidden(
            "question identity or destination does not match authenticated scope",
        ));
    }
    let ad = advertisement(&runtime, q.capability).await?;
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    mem_federation_store::confirm_expertise_question_with_models(
        &mut tx,
        &current,
        &request.review,
        &ad,
        &[destination.into()],
        &runtime.expertise_model_destinations,
        &request.confirmed_digest,
    )
    .await
    .map_err(store_error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    // Never send uncommitted consent. Network errors leave the exact request
    // recoverable; retrying reuses its ID rather than inventing a new question.
    let mut tx = state.pool()?.begin().await.map_err(|_| unavailable())?;
    let current = current_access(&state, &mut tx, &principal, &access).await?;
    let question = mem_federation_store::consented_expertise_question_with_models(
        &mut tx,
        &current,
        (runtime.space, runtime.instance, q.request_id),
        &ad,
        &[destination.into()],
        &runtime.expertise_model_destinations,
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
