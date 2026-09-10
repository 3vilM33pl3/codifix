// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use mem_record::{
    JobDetail, JobQueueStats, JobRetryRequest, JobRetryResponse, JobStatus, JobWorkerHealth,
    Permission, ProjectJobsResponse,
};
use mem_store::jobs::{self, JobActor};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

#[derive(Default, Deserialize)]
pub(crate) struct Filters {
    after: Option<Uuid>,
    status: Option<JobStatus>,
    limit: Option<u32>,
}

fn error(error: mem_store::Error) -> ApiError {
    match error {
        mem_store::Error::Database(sqlx::Error::RowNotFound) => {
            ApiError::not_found("job not found")
        }
        mem_store::Error::Conflict | mem_store::Error::StaleLease => ApiError::status_message(
            axum::http::StatusCode::CONFLICT,
            "job cannot be retried in its current state or request ID was reused",
        ),
        mem_store::Error::Invalid(_) => {
            ApiError::status_message(axum::http::StatusCode::BAD_REQUEST, "invalid job request")
        }
        _ => ApiError::service_unavailable("background storage unavailable"),
    }
}
async fn project(pool: &sqlx::PgPool, slug: &str) -> Result<Uuid, ApiError> {
    sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(slug)
        .fetch_one(pool)
        .await
        .map_err(|e| error(e.into()))
}

pub(crate) async fn list(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(filters): Query<Filters>,
) -> Result<Json<ProjectJobsResponse>, ApiError> {
    let pool = state.pool()?;
    let project = project(&pool, &slug).await?;
    let page = jobs::list(
        &pool,
        project,
        filters.after,
        filters.status,
        filters.limit.unwrap_or(50),
    )
    .await
    .map_err(error)?;
    let row=sqlx::query("SELECT count(*) FILTER(WHERE status='queued') AS queued,count(*) FILTER(WHERE status='running') AS running,count(*) FILTER(WHERE status='retry_wait') AS retry_wait,count(*) FILTER(WHERE status='blocked') AS blocked,count(*) FILTER(WHERE status='failed') AS failed,EXTRACT(EPOCH FROM clock_timestamp()-min(available_at) FILTER(WHERE status IN ('queued','retry_wait') AND available_at<=clock_timestamp() AND NOT EXISTS (SELECT 1 FROM background_job_dependencies d JOIN background_jobs parent ON parent.id=d.depends_on WHERE d.job_id=j.id AND parent.status NOT IN ('succeeded','skipped'))))::float8 AS oldest_ready_age_seconds FROM background_jobs j WHERE project_id=$1")
        .bind(project).fetch_one(&pool).await.map_err(|e|error(e.into()))?;
    let last = state
        .background
        .last_poll
        .load(std::sync::atomic::Ordering::Relaxed);
    let stats = JobQueueStats {
        queued: row.try_get("queued").map_err(|e| error(e.into()))?,
        running: row.try_get("running").map_err(|e| error(e.into()))?,
        retry_wait: row.try_get("retry_wait").map_err(|e| error(e.into()))?,
        blocked: row.try_get("blocked").map_err(|e| error(e.into()))?,
        failed: row.try_get("failed").map_err(|e| error(e.into()))?,
        oldest_ready_age_seconds: row
            .try_get("oldest_ready_age_seconds")
            .map_err(|e| error(e.into()))?,
    };
    Ok(Json(ProjectJobsResponse {
        read_only: state.config.service.read_only,
        page,
        stats,
        worker: JobWorkerHealth {
            enabled: state.is_primary()
                && state.config.background_work.enabled
                && !state.config.service.read_only,
            last_poll_at: (last > 0).then_some(last),
        },
    }))
}
pub(crate) async fn detail(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, Uuid)>,
) -> Result<Json<JobDetail>, ApiError> {
    let pool = state.pool()?;
    let project = project(&pool, &slug).await?;
    jobs::detail(&pool, project, id)
        .await
        .map(Json)
        .map_err(error)
}
pub(crate) async fn retry(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    Path((slug, id)): Path<(String, Uuid)>,
    Json(request): Json<JobRetryRequest>,
) -> Result<Json<JobRetryResponse>, ApiError> {
    let pool = state.pool()?;
    let project = project(&pool, &slug).await?;
    let mut tx = pool.begin().await.map_err(|e| error(e.into()))?;
    sqlx::query("SELECT project_id FROM background_project_slots WHERE project_id=$1 FOR UPDATE")
        .bind(project)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| error(e.into()))?;
    let row =
        sqlx::query("SELECT actor FROM background_jobs WHERE project_id=$1 AND id=$2 FOR UPDATE")
            .bind(project)
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| error(e.into()))?;
    let original: JobActor =
        serde_json::from_value(row.try_get("actor").map_err(|e| error(e.into()))?)
            .map_err(|_| ApiError::internal("invalid job identity"))?;
    let actor = JobActor::Credential {
        principal: principal.id,
        token: principal.token_id,
        session: principal.session_id,
    };
    for identity in [&actor, &original] {
        let permissions = crate::auth::background_permissions(&state, &mut tx, identity, project)
            .await
            .map_err(|_| {
                ApiError::forbidden("current background execution authority is required")
            })?;
        if !permissions.contains(Permission::LoopsRun)
            || !permissions.contains(Permission::MemoryCurate)
        {
            return Err(ApiError::forbidden(
                "retry requires loops_run and memory_curate",
            ));
        }
    }
    let response = jobs::retry_tx(&mut tx, project, id, request.request_id, &actor)
        .await
        .map_err(error)?;
    tx.commit().await.map_err(|e| error(e.into()))?;
    state.background.notify.notify_one();
    Ok(Json(response))
}
