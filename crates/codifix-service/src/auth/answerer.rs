// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit installation identity, not a caller-selected role or login token.
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use axum::{
    Extension, Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use codifix_record::{ExpertiseAnswererRequest, ExpertiseAnswererResponse, Permission};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

fn storage(_: sqlx::Error) -> ApiError {
    ApiError::service_unavailable(
        "answerer provisioning outcome unconfirmed; inspect or retry the same request",
    )
}
fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "answerer request or current identity changed; no permissions restored",
    )
}

fn authority(error: ApiError) -> ApiError {
    if matches!(
        error.status,
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
    ) {
        ApiError::forbidden(
            "current global administration and a writable primary service are required",
        )
    } else {
        ApiError::service_unavailable("answerer authority unavailable; no outcome confirmed")
    }
}

pub(crate) async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<ExpertiseAnswererRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let response = provision(&state, &actor, &request).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(response)))
}

pub(crate) async fn provision(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    request: &ExpertiseAnswererRequest,
) -> Result<ExpertiseAnswererResponse, ApiError> {
    if !request.confirmed
        || request.request_id.is_nil()
        || request.principal_id.is_nil()
        || request.principal_id == actor.id
        || request.principal_id == super::local_owner_id()
        || request.name.trim().is_empty()
        || request.name.len() > 120
        || request.name.trim() != request.name
        || request.name.chars().any(char::is_control)
        || request.project.trim().is_empty()
        || request.project.len() > 120
        || request.project.trim() != request.project
        || request.project.chars().any(char::is_control)
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid or unconfirmed answerer provisioning request",
        ));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(storage)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    super::lock_global_administrator(state, &mut tx, actor)
        .await
        .map_err(authority)?;
    // Rare installation operations serialize together: both request-ID reuse
    // across targets and target-ID reuse across requests have one lock order.
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:expertise-answerer-provision',0))",
    )
    .execute(&mut *tx)
    .await
    .map_err(storage)?;
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1 FOR SHARE")
        .bind(&request.project)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let permissions = vec![Permission::MemoryRead, Permission::BundleExport];
    let permission_json = json!(permissions);
    // Bounded typed request only; no credentials, source bytes or model payload.
    let metadata = json!({"request":request,"project_id":project,"permissions":permissions});
    let prior = sqlx::query("SELECT actor_principal_id,target_principal_id,project_id,event_type,outcome,metadata_json FROM auth_audit_events WHERE id=$1")
        .bind(request.request_id).fetch_optional(&mut *tx).await.map_err(storage)?;
    let replayed = prior.is_some();
    let membership = Uuid::new_v5(
        &request.request_id,
        b"expertise-answerer-project-membership",
    );
    if let Some(row) = prior {
        if row
            .try_get::<Option<Uuid>, _>("actor_principal_id")
            .map_err(storage)?
            != Some(actor.id)
            || row
                .try_get::<Option<Uuid>, _>("target_principal_id")
                .map_err(storage)?
                != Some(request.principal_id)
            || row
                .try_get::<Option<Uuid>, _>("project_id")
                .map_err(storage)?
                != Some(project)
            || row.try_get::<String, _>("event_type").map_err(storage)?
                != "expertise.answerer.provision"
            || row.try_get::<String, _>("outcome").map_err(storage)? != "success"
            || row
                .try_get::<serde_json::Value, _>("metadata_json")
                .map_err(storage)?
                != metadata
        {
            return Err(conflict());
        }
        let current: Option<bool> = sqlx::query_scalar("SELECT kind='internal' AND display_name=$2 AND email IS NULL AND issuer IS NULL AND subject IS NULL AND global_role IS NULL AND global_permissions_json='[]'::jsonb AND groups_json='[]'::jsonb AND disabled_at IS NULL FROM auth_principals WHERE id=$1 FOR SHARE")
            .bind(request.principal_id).bind(&request.name).fetch_optional(&mut *tx).await.map_err(storage)?;
        let rows = sqlx::query("SELECT id,project_id,role,source,permissions_json,created_by FROM auth_project_memberships WHERE principal_id=$1 ORDER BY project_id FOR SHARE")
            .bind(request.principal_id).fetch_all(&mut *tx).await.map_err(storage)?;
        if current != Some(true) || rows.len() != 1 {
            return Err(conflict());
        }
        let row = &rows[0];
        if row.try_get::<Uuid, _>("id").map_err(storage)? != membership
            || row.try_get::<Uuid, _>("project_id").map_err(storage)? != project
            || row.try_get::<String, _>("role").map_err(storage)? != "reader"
            || row.try_get::<String, _>("source").map_err(storage)? != "bootstrap"
            || row
                .try_get::<Option<Uuid>, _>("created_by")
                .map_err(storage)?
                != Some(actor.id)
            || row
                .try_get::<Option<serde_json::Value>, _>("permissions_json")
                .map_err(storage)?
                != Some(permission_json)
        {
            return Err(conflict());
        }
        let has_login: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_service_tokens WHERE principal_id=$1) OR EXISTS(SELECT 1 FROM auth_web_sessions WHERE principal_id=$1)")
            .bind(request.principal_id).fetch_one(&mut *tx).await.map_err(storage)?;
        if has_login {
            return Err(conflict());
        }
    } else {
        let inserted = sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json,groups_json) VALUES($1,'internal',$2,'[]'::jsonb,'[]'::jsonb) ON CONFLICT DO NOTHING")
            .bind(request.principal_id).bind(&request.name).execute(&mut *tx).await.map_err(storage)?;
        if inserted.rows_affected() != 1 {
            return Err(conflict());
        }
        sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source,permissions_json,created_by) VALUES($1,$2,$3,'reader','bootstrap',$4,$5)")
            .bind(membership).bind(request.principal_id).bind(project).bind(permission_json).bind(actor.id).execute(&mut *tx).await.map_err(storage)?;
        sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,target_principal_id,project_id,event_type,outcome,metadata_json,recorded_at) VALUES($1,$2,$3,$4,'expertise.answerer.provision','success',$5,clock_timestamp())")
            .bind(request.request_id).bind(actor.id).bind(request.principal_id).bind(project).bind(metadata).execute(&mut *tx).await.map_err(storage)?;
    }
    // Credentials can expire while waiting on another transaction.
    super::lock_global_administrator(state, &mut tx, actor)
        .await
        .map_err(authority)?;
    tx.commit().await.map_err(storage)?;
    Ok(ExpertiseAnswererResponse {
        request_id: request.request_id,
        principal_id: request.principal_id,
        project_id: project,
        project: request.project.clone(),
        permissions,
        replayed,
        credential_issued: false,
    })
}

#[cfg(test)]
mod tests;
