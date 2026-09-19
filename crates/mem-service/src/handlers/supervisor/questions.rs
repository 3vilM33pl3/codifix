//! Explicit, project-private paid-work intent. HTTP never invokes a provider.
use super::{proposals::*, *};
use axum::extract::Path;
use mem_record::{
    SupervisorQuestionList, SupervisorQuestionPrepare, SupervisorQuestionPreview,
    SupervisorQuestionRequest,
};
use mem_supervisor_runtime::{model::PreparedRequest, work::PinnedWork};
use mem_supervisor_store::{
    questions::{self, QuestionActor, QuestionSubmission},
    sources,
};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

fn invalid() -> ApiError {
    ApiError::status_message(
        StatusCode::BAD_REQUEST,
        "invalid supervisor question or confirmation",
    )
}

fn parse_digest(value: &str) -> Result<[u8; 32], ApiError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| invalid())?;
    }
    Ok(digest)
}

fn actor(principal: &crate::AuthenticatedPrincipal) -> Result<QuestionActor, ApiError> {
    use crate::auth::CredentialSource;
    match principal.credential_source {
        CredentialSource::ServiceToken if principal.token_id.is_some() => {}
        CredentialSource::BrowserSession if principal.session_id.is_some() => {}
        _ => {
            return Err(ApiError::forbidden(
                "a revocable session or service token is required",
            ));
        }
    }
    Ok(QuestionActor {
        principal_id: principal.id,
        token_id: principal.token_id,
        session_id: principal.session_id,
    })
}

fn store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict | StoreError::Transition | StoreError::Policy(_) => conflict(),
        StoreError::Binding => {
            ApiError::forbidden("current question execution authority is required")
        }
        StoreError::InvalidInput => invalid(),
        StoreError::QueueFull => ApiError::status_message(
            StatusCode::TOO_MANY_REQUESTS,
            "supervisor question queue is full; inspect outstanding questions before submitting more",
        ),
        _ => unavailable(error),
    }
}

async fn preview_tx(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    principal: &crate::AuthenticatedPrincipal,
    slug: &str,
    work: &PinnedWork,
    question: String,
) -> Result<SupervisorQuestionPreview, ApiError> {
    questions::validate_question(&question).map_err(|_| invalid())?;
    access(state, tx, principal, slug, work.grant().project_id, true).await?;
    bound(state, tx, work).await?;
    questions::authorize_requester_tx(tx, &actor(principal)?, work.grant().project_id)
        .await
        .map_err(store_error)?;
    let evidence = sources::authorize_tx(tx, work.grant())
        .await
        .map_err(store_error)?;
    let prepared = PreparedRequest::new(&question, &evidence).map_err(|_| invalid())?;
    // authorize_tx locks and validates the current policy through commit.
    let policy = sqlx::query("SELECT daily_calls,daily_usd_micros FROM supervisor_policies WHERE area='workshop' AND revision=$1")
        .bind(work.grant().policy_revision).fetch_one(&mut **tx).await.map_err(unavailable)?;
    let daily_call_limit = u32::try_from(
        policy
            .try_get::<i32, _>("daily_calls")
            .map_err(unavailable)?,
    )
    .map_err(|_| conflict())?;
    let daily_usd_micros = u64::try_from(
        policy
            .try_get::<i64, _>("daily_usd_micros")
            .map_err(unavailable)?,
    )
    .map_err(|_| conflict())?;
    Ok(SupervisorQuestionPreview {
        profile_digest: hex(&work.digest()),
        request_digest: prepared.digest().to_owned(),
        question,
        model: prepared.generation_body()["model"]
            .as_str()
            .ok_or_else(conflict)?
            .to_owned(),
        destination: "openai".to_owned(),
        source_count: evidence.len(),
        daily_call_limit,
        daily_usd_micros: daily_usd_micros.to_string(),
    })
}

pub(crate) async fn prepare(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path(slug): Path<String>,
    Json(request): Json<SupervisorQuestionPrepare>,
) -> Result<impl IntoResponse, ApiError> {
    let work = profile(&state).await?;
    let mut tx = state.pool()?.begin().await.map_err(unavailable)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    let preview = preview_tx(&state, &mut tx, &principal, &slug, &work, request.question).await?;
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        true,
    )
    .await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(preview)))
}

pub(crate) async fn submit(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path(slug): Path<String>,
    Json(request): Json<SupervisorQuestionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.request_id.is_nil() {
        return Err(invalid());
    }
    let profile_digest = parse_digest(&request.profile_digest)?;
    let request_digest = parse_digest(&request.request_digest)?;
    let work = profile(&state).await?;
    let mut tx = state.pool()?.begin().await.map_err(unavailable)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    let preview = preview_tx(&state, &mut tx, &principal, &slug, &work, request.question).await?;
    if preview.profile_digest != request.profile_digest
        || preview.request_digest != request.request_digest
    {
        return Err(conflict());
    }
    let question = questions::enqueue_tx(
        &mut tx,
        &QuestionSubmission {
            request_id: request.request_id,
            project_id: work.grant().project_id,
            instance_id: work.grant().instance_id,
            profile_digest,
            request_digest,
            policy_revision: work.grant().policy_revision,
            actor: actor(&principal)?,
            question: preview.question,
        },
    )
    .await
    .map_err(store_error)?;
    let result = questions::status_tx(&mut tx, &question)
        .await
        .map_err(store_error)?;
    // Intake may have waited on a competing submission. Do not acknowledge
    // newly queued work after the standing disclosure grant has expired.
    sources::authorize_tx(&mut tx, work.grant())
        .await
        .map_err(store_error)?;
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        true,
    )
    .await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    after: Option<Uuid>,
}

pub(crate) async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let work = profile(&state).await?;
    let mut tx = state.pool()?.begin().await.map_err(unavailable)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        false,
    )
    .await?;
    bound(&state, &mut tx, &work).await?;
    let mut records =
        questions::list_tx(&mut tx, work.grant().project_id, principal.id, query.after)
            .await
            .map_err(store_error)?;
    let next_after = if records.len() > 50 {
        records.truncate(50);
        records.last().map(|r| r.submission.request_id)
    } else {
        None
    };
    let mut statuses = Vec::with_capacity(records.len());
    for record in records {
        statuses.push(
            questions::status_tx(&mut tx, &record)
                .await
                .map_err(store_error)?,
        );
    }
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        false,
    )
    .await?;
    tx.commit().await.map_err(unavailable)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(SupervisorQuestionList {
            questions: statuses,
            next_after,
        }),
    ))
}

pub(crate) async fn status(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path((slug, id)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let work = profile(&state).await?;
    let mut tx = state.pool()?.begin().await.map_err(unavailable)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        false,
    )
    .await?;
    bound(&state, &mut tx, &work).await?;
    let record = questions::read_tx(&mut tx, work.grant().project_id, id)
        .await
        .map_err(store_error)?
        .filter(|record| record.submission.actor.principal_id == principal.id)
        .ok_or_else(|| {
            ApiError::status_message(StatusCode::NOT_FOUND, "supervisor question unavailable")
        })?;
    let result = questions::status_tx(&mut tx, &record)
        .await
        .map_err(store_error)?;
    access(
        &state,
        &mut tx,
        &principal,
        &slug,
        work.grant().project_id,
        false,
    )
    .await?;
    tx.commit().await.map_err(unavailable)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)))
}
