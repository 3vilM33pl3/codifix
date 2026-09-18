// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use mem_record::{SupervisorExecution, SupervisorResponse};
use mem_supervisor_store::{StoreError, SupervisorStore};
use serde::Deserialize;

#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod tests;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filters {
    after: Option<String>,
    limit: Option<u32>,
}

pub(crate) async fn inspect(
    State(state): State<AppState>,
    Extension(principal): Extension<crate::AuthenticatedPrincipal>,
    Query(filters): Query<Filters>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = filters.after.as_deref().unwrap_or("0");
    if cursor.is_empty() || !cursor.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid supervisor cursor",
        ));
    }
    let after = cursor.parse::<i64>().map_err(|_| {
        ApiError::status_message(StatusCode::BAD_REQUEST, "invalid supervisor cursor")
    })?;
    let limit = filters.limit.unwrap_or(50);
    if after < 0 || !(1..=100).contains(&limit) {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid supervisor page bounds",
        ));
    }
    let snapshot = match &state.config.supervisor {
        None => None,
        Some(identity) => Some(
            SupervisorStore::new(state.pool()?)
                .inspect(identity.instance_id, identity.area, after, limit)
                .await
                .map_err(|error| match error {
                    StoreError::Binding | StoreError::InvalidInput => ApiError::status_message(
                        StatusCode::CONFLICT,
                        "configured supervisor identity does not match its database",
                    ),
                    _ => ApiError::service_unavailable("supervisor inspection unavailable"),
                })?,
        ),
    };
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(SupervisorResponse {
            read_only: state.config.service.read_only,
            execution: SupervisorExecution::NotIntegrated,
            model_work: state.supervisor_model.snapshot(),
            deterministic_checks_configured: state
                .config
                .supervisor
                .as_ref()
                .is_some_and(|s| s.deterministic_checks),
            can_control: principal.has_global(mem_record::Permission::SystemAdmin)
                && !state.config.service.read_only
                && !state.config.evaluation.active()
                && state.is_primary()
                && snapshot.as_ref().is_some_and(|s| {
                    s.area.is_some() && s.policy.as_ref().is_some_and(|p| !p.revoked)
                }),
            snapshot,
        }),
    ))
}

mod control;
pub(crate) use control::control;
pub(crate) mod proposals;
