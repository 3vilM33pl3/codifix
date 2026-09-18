//! Exact-project private output and human review; never action authorization.
use super::*;
use axum::extract::Path;
use mem_record::{
    Permission, SupervisorCitation, SupervisorProposalDetail, SupervisorProposalList,
    SupervisorReviewDecision, SupervisorReviewReceipt, SupervisorReviewRequest,
};
use mem_supervisor_runtime::{model::PreparedRequest, work::PinnedWork};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
#[cfg(test)]
mod tests;

fn unavailable(_: impl std::fmt::Display) -> ApiError {
    ApiError::service_unavailable(
        "private supervisor operation unconfirmed; retry the same request ID",
    )
}
fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "supervisor profile, sources or review changed; refresh before continuing",
    )
}
fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

async fn profile(state: &AppState) -> Result<PinnedWork, ApiError> {
    let identity = state.config.supervisor.as_ref().ok_or_else(conflict)?;
    let config = identity.interaction.clone().ok_or_else(|| {
        ApiError::status_message(
            StatusCode::NOT_FOUND,
            "private supervisor interaction is not configured",
        )
    })?;
    let instance = identity.instance_id;
    let area = identity.area;
    tokio::task::spawn_blocking(move || {
        PinnedWork::load(&config.manifest, &config.sha256, instance, area)
    })
    .await
    .map_err(unavailable)?
    .map_err(|_| conflict())
}

async fn access(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    principal: &crate::AuthenticatedPrincipal,
    slug: &str,
    project: Uuid,
    review: bool,
) -> Result<(), ApiError> {
    use crate::auth::CredentialSource;
    if review
        && (state.config.service.read_only
            || state.config.evaluation.active()
            || !state.is_primary())
    {
        return Err(ApiError::forbidden(
            "supervisor review is disabled on this service",
        ));
    }
    match principal.credential_source {
        CredentialSource::LocalApiToken
            if state.config.auth.mode == mem_record::AuthMode::SingleUser
                && principal.id == crate::auth::local_owner_id() => {}
        CredentialSource::ServiceToken if principal.token_id.is_some() => {}
        CredentialSource::BrowserSession if principal.session_id.is_some() => {}
        _ => {
            return Err(ApiError::forbidden(
                "a current authenticated credential is required",
            ));
        }
    }
    let actor = mem_store::jobs::JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: principal.session_id,
    };
    let permissions = crate::auth::locked_project_permissions(state, tx, &actor, project)
        .await
        .map_err(|_| ApiError::forbidden("current project access is required"))?;
    let matches: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id=$1 AND slug=$2)")
            .bind(project)
            .bind(slug)
            .fetch_one(&mut **tx)
            .await
            .map_err(unavailable)?;
    if !matches
        || !permissions.contains(Permission::MemoryRead)
        || (review && !permissions.contains(Permission::LoopsRun))
    {
        return Err(ApiError::forbidden(
            "current project read and review permissions are required",
        ));
    }
    Ok(())
}

async fn bound(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    work: &PinnedWork,
) -> Result<(), ApiError> {
    let grant = work.grant();
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supervisor_instance_binding WHERE instance_id=$1 AND area='workshop')")
        .bind(grant.instance_id).fetch_one(&mut **tx).await.map_err(unavailable)?;
    if !valid
        || state
            .config
            .supervisor
            .as_ref()
            .is_none_or(|s| s.instance_id != grant.instance_id)
    {
        return Err(conflict());
    }
    Ok(())
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
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(unavailable)?;
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
    let mut actions: Vec<Uuid> = sqlx::query_scalar("SELECT action_id FROM supervisor_proposals WHERE area='workshop' AND project_id=$1 AND ($2::uuid IS NULL OR action_id>$2) ORDER BY action_id LIMIT 51")
        .bind(work.grant().project_id).bind(query.after).fetch_all(&mut *tx).await.map_err(unavailable)?;
    let next_after = if actions.len() > 50 {
        actions.truncate(50);
        actions.last().copied()
    } else {
        None
    };
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
        Json(SupervisorProposalList {
            actions,
            next_after,
        }),
    ))
}

async fn receipt(
    tx: &mut Transaction<'_, Postgres>,
    action: Uuid,
) -> Result<Option<SupervisorReviewReceipt>, ApiError> {
    let row = sqlx::query("SELECT * FROM supervisor_proposal_reviews WHERE action_id=$1")
        .bind(action)
        .fetch_optional(&mut **tx)
        .await
        .map_err(unavailable)?;
    row.map(|r| {
        Ok(SupervisorReviewReceipt {
            request_id: r.try_get("request_id").map_err(unavailable)?,
            action_id: action,
            proposal_digest: hex(&r
                .try_get::<Vec<u8>, _>("proposal_digest")
                .map_err(unavailable)?),
            decision: match r
                .try_get::<String, _>("decision")
                .map_err(unavailable)?
                .as_str()
            {
                "approve" => SupervisorReviewDecision::Approve,
                "reject" => SupervisorReviewDecision::Reject,
                _ => return Err(conflict()),
            },
            reviewer_id: r.try_get("reviewer_id").map_err(unavailable)?,
            recorded_at: r.try_get("recorded_at").map_err(unavailable)?,
        })
    })
    .transpose()
}

async fn detail_tx(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    work: &PinnedWork,
    action: Uuid,
) -> Result<SupervisorProposalDetail, ApiError> {
    bound(state, tx, work).await?;
    let grant = work.grant();
    let stored = SupervisorStore::private_proposal_tx(
        tx,
        mem_record::SupervisorArea::Workshop,
        grant.project_id,
        action,
    )
    .await
    .map_err(|_| {
        ApiError::status_message(
            StatusCode::NOT_FOUND,
            "private proposal unavailable in this project",
        )
    })?;
    // Never infer citations by searching for similar text or silently substitute
    // newer versions. The immutable request/evidence must match this profile.
    let evidence = mem_supervisor_store::sources::validate_tx(tx, grant.project_id, &grant.sources)
        .await
        .map_err(|_| conflict())?;
    let prepared = PreparedRequest::new(work.question(), &evidence).map_err(|_| conflict())?;
    if stored.policy_revision != grant.policy_revision
        || stored.evidence_digests != prepared.evidence_digests()
        || hex(&stored.request_digest) != prepared.digest()
    {
        return Err(conflict());
    }
    let citations = stored
        .proposal
        .evidence_indices
        .iter()
        .map(|&index| {
            let source = &grant.sources[index];
            SupervisorCitation {
                index,
                canonical_id: source.canonical_id,
                version_id: source.version_id,
                version: source.version,
                evidence_digest: hex(&source.evidence_digest),
            }
        })
        .collect();
    Ok(SupervisorProposalDetail {
        project_id: grant.project_id,
        action_id: action,
        proposal_digest: hex(&stored.proposal_digest),
        question: work.question().into(),
        summary: stored.proposal.summary,
        proposal: stored.proposal.proposal,
        citations,
        created_at: stored.created_at,
        review: receipt(tx, action).await?,
        can_review: false,
    })
}

pub(crate) async fn detail(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path((slug, action)): Path<(String, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    let work = profile(&state).await?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(unavailable)?;
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
    let mut result = detail_tx(&state, &mut tx, &work, action).await?;
    result.can_review = result.review.is_none()
        && !state.config.service.read_only
        && !state.config.evaluation.active()
        && state.is_primary()
        && principal
            .permissions_for_project(&slug)
            .contains(Permission::LoopsRun);
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

pub(crate) async fn review(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Path((slug, action)): Path<(String, Uuid)>,
    Json(request): Json<SupervisorReviewRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.request_id.is_nil()
        || request.proposal_digest.len() != 64
        || !request
            .proposal_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(conflict());
    }
    let work = profile(&state).await?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(unavailable)?;
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
        true,
    )
    .await?;
    // Lock sources before the proposal; shared source fences prevent withdrawn
    // material being approved after a competing mutation commits.
    let detail = detail_tx(&state, &mut tx, &work, action).await?;
    if detail.proposal_digest != request.proposal_digest {
        return Err(conflict());
    }
    sqlx::query("SELECT action_id FROM supervisor_proposals WHERE action_id=$1 FOR UPDATE")
        .bind(action)
        .fetch_one(&mut *tx)
        .await
        .map_err(unavailable)?;
    if let Some(prior) = receipt(&mut tx, action).await? {
        if prior.request_id != request.request_id
            || prior.proposal_digest != request.proposal_digest
            || prior.decision != request.decision
            || prior.reviewer_id != principal.id
        {
            return Err(conflict());
        }
    } else {
        let decision = match request.decision {
            SupervisorReviewDecision::Approve => "approve",
            SupervisorReviewDecision::Reject => "reject",
        };
        let inserted=sqlx::query("INSERT INTO supervisor_proposal_reviews(action_id,request_id,project_id,reviewer_id,proposal_digest,decision) VALUES($1,$2,$3,$4,decode($5,'hex'),$6) ON CONFLICT DO NOTHING")
            .bind(action).bind(request.request_id).bind(work.grant().project_id).bind(principal.id).bind(&request.proposal_digest).bind(decision).execute(&mut *tx).await.map_err(unavailable)?;
        if inserted.rows_affected() != 1 {
            return Err(conflict());
        }
    }
    let result = receipt(&mut tx, action).await?.ok_or_else(conflict)?;
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
