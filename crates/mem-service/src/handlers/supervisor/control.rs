// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use mem_record::{SupervisorPolicyRequest, SupervisorPolicyResponse};
use sqlx::Row;

fn storage(_: sqlx::Error) -> ApiError {
    ApiError::service_unavailable(
        "supervisor control outcome not confirmed; retry the same request ID",
    )
}
fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "supervisor identity, policy or request changed; inspect current state",
    )
}
fn authority(error: ApiError) -> ApiError {
    if error.status == StatusCode::FORBIDDEN || error.status == StatusCode::UNAUTHORIZED {
        ApiError::forbidden("current global administration and a writable service are required")
    } else {
        ApiError::service_unavailable("supervisor authority unavailable; no outcome confirmed")
    }
}

pub(crate) async fn control(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Json(request): Json<SupervisorPolicyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.request_id.is_nil()
        || request.instance_id.is_nil()
        || request.policy_revision.is_nil()
        || request.expected_enabled == request.enabled
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid supervisor policy request",
        ));
    }
    let identity = state.config.supervisor.as_ref().ok_or_else(conflict)?;
    if identity.instance_id != request.instance_id {
        return Err(conflict());
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(storage)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    crate::auth::lock_supervisor_administrator(&state, &mut tx, &principal)
        .await
        .map_err(authority)?;
    // Request reconciliation and policy changes serialize on the same authority
    // as worker admission. A lost response can safely replay without toggling.
    let area = serde_json::to_value(identity.area).map_err(|_| conflict())?;
    sqlx::query("SELECT area FROM supervisor_authorities WHERE area=$1 FOR UPDATE")
        .bind(area.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
    let bound: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM supervisor_instance_binding WHERE instance_id=$1 AND area=$2)",
    )
    .bind(identity.instance_id)
    .bind(area.as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(storage)?;
    if !bound {
        return Err(conflict());
    }
    let metadata = serde_json::json!({"request":request,"area":identity.area});
    let prior = sqlx::query("SELECT actor_principal_id,event_type,outcome,metadata_json FROM auth_audit_events WHERE id=$1")
        .bind(request.request_id).fetch_optional(&mut *tx).await.map_err(storage)?;
    let replayed = prior.is_some();
    if let Some(prior) = prior {
        if prior
            .try_get::<Option<uuid::Uuid>, _>("actor_principal_id")
            .map_err(storage)?
            != Some(principal.id)
            || prior.try_get::<String, _>("event_type").map_err(storage)? != "supervisor.policy.set"
            || prior.try_get::<String, _>("outcome").map_err(storage)? != "success"
            || prior
                .try_get::<serde_json::Value, _>("metadata_json")
                .map_err(storage)?
                != metadata
        {
            return Err(conflict());
        }
    } else {
        SupervisorStore::set_enabled_tx(
            &mut tx,
            identity.instance_id,
            identity.area,
            request.policy_revision,
            request.expected_enabled,
            request.enabled,
        )
        .await
        .map_err(|e| match e {
            StoreError::Database(error) => storage(error),
            _ => conflict(),
        })?;
        sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,metadata_json,recorded_at) VALUES($1,$2,'supervisor.policy.set','success',$3,clock_timestamp())")
            .bind(request.request_id).bind(principal.id).bind(metadata).execute(&mut *tx).await.map_err(storage)?;
    }
    // Expiry is a real clock: do not accept an expired credential after a lock wait.
    crate::auth::lock_supervisor_administrator(&state, &mut tx, &principal)
        .await
        .map_err(authority)?;
    tx.commit().await.map_err(storage)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(SupervisorPolicyResponse {
            request_id: request.request_id,
            instance_id: request.instance_id,
            policy_revision: request.policy_revision,
            enabled: request.enabled,
            replayed,
        }),
    ))
}
