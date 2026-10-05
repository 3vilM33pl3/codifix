// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit local human cache view. No ordinary query/model/MCP integration.
use crate::{ApiError, AppState, AuthenticatedPrincipal, CredentialSource};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use mem_config::LaptopCacheSubscription;
use mem_federation::{
    publication::{InstanceId, SpaceId},
    subscription::ProjectScope,
};
use mem_federation_store::{ProjectProjectionStatus, ProjectProjectionStore};
use mem_record::*;
use sqlx::{Row, types::Json as SqlJson};
use std::net::SocketAddr;
use uuid::Uuid;

#[cfg(test)]
mod tests;

fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "local approved project cache unavailable or binding changed; no fallback, network lookup or model call performed",
    )
}
fn scope(area: &str, binding: &LaptopCacheSubscription) -> Result<ProjectScope, ApiError> {
    let value = ProjectScope {
        company: binding.company_id,
        area: area.into(),
        owner: InstanceId::new(binding.owner_instance_id).map_err(|_| unavailable())?,
        project: binding.project_id,
        space: SpaceId::new(binding.space_id).map_err(|_| unavailable())?,
    };
    if !value.valid() {
        return Err(unavailable());
    }
    Ok(value)
}
fn authorize(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    peer: SocketAddr,
) -> Result<(), ApiError> {
    if state.config.laptop_cache.is_none() {
        return Err(ApiError::not_found("local laptop cache is not configured"));
    }
    state
        .config
        .validate_laptop_cache()
        .map_err(|_| unavailable())?;
    if !peer.ip().is_loopback()
        || actor.id != crate::auth::local_owner_id()
        || !matches!(
            actor.credential_source,
            CredentialSource::LocalApiToken | CredentialSource::BrowserSession
        )
        || !actor.has_global(Permission::MemoryRead)
    {
        return Err(ApiError::forbidden(
            "authenticated loopback local owner required for approved cached knowledge",
        ));
    }
    Ok(())
}
fn describe(
    area: &str,
    binding: &LaptopCacheSubscription,
    status: Option<ProjectProjectionStatus>,
) -> LaptopCacheProject {
    LaptopCacheProject {
        subscription_id: binding.subscription_id,
        device_id: binding.device_id,
        company_id: binding.company_id,
        area: area.into(),
        owner_instance_id: binding.owner_instance_id,
        project_id: binding.project_id,
        space_id: binding.space_id,
        cursor: status.as_ref().map_or(0, |s| s.cursor),
        observed_at: status.as_ref().and_then(|s| s.observed_at),
        initialized: status.is_some(),
        read_only: true,
    }
}
fn response(value: &impl serde::Serialize) -> Result<Response, ApiError> {
    let bytes = serde_json::to_vec(value).map_err(|_| unavailable())?;
    if bytes.len() > 256 * 1024 {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "approved cache response exceeds 256 KiB; narrow the query or result limit",
        ));
    }
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        bytes,
    )
        .into_response())
}
pub(crate) async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<Response, ApiError> {
    authorize(&state, &actor, peer)?;
    let cache = state.config.laptop_cache.as_ref().ok_or_else(unavailable)?;
    let area = state.config.area.as_ref().ok_or_else(unavailable)?;
    let pool = state.pool().map_err(|_| unavailable())?;
    let mut projects = Vec::new();
    for binding in &cache.subscriptions {
        let expected = scope(&area.name, binding)?;
        let row = sqlx::query("SELECT device,scope,cursor,observed_at FROM onboarding_project_projections WHERE subscription=$1")
            .bind(binding.subscription_id).fetch_optional(&pool).await.map_err(|_|unavailable())?;
        let status = if let Some(row) = row {
            if row
                .try_get::<Uuid, _>("device")
                .map_err(|_| unavailable())?
                != binding.device_id
                || row
                    .try_get::<SqlJson<ProjectScope>, _>("scope")
                    .map_err(|_| unavailable())?
                    .0
                    != expected
            {
                return Err(unavailable());
            }
            Some(ProjectProjectionStatus {
                cursor: u64::try_from(row.try_get::<i64, _>("cursor").map_err(|_| unavailable())?)
                    .map_err(|_| unavailable())?,
                observed_at: row.try_get("observed_at").map_err(|_| unavailable())?,
            })
        } else {
            None
        };
        projects.push(describe(&area.name, binding, status));
    }
    response(&LaptopCacheList {
        schema_version: 1,
        area: area.name.clone(),
        instance_id: area.instance_id,
        projects,
    })
}
pub(crate) async fn search(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(subscription): Path<Uuid>,
    Json(request): Json<LaptopCacheSearchRequest>,
) -> Result<Response, ApiError> {
    authorize(&state, &actor, peer)?;
    if request.question.trim().is_empty()
        || request.question.len() > 4096
        || request
            .question
            .chars()
            .any(|c| c.is_control() && !c.is_whitespace())
        || !(1..=50).contains(&request.limit)
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "question requires 1..4096 UTF-8 bytes and limit 1..50",
        ));
    }
    let area = state.config.area.as_ref().ok_or_else(unavailable)?;
    let binding = state
        .config
        .laptop_cache
        .as_ref()
        .and_then(|c| {
            c.subscriptions
                .iter()
                .find(|s| s.subscription_id == subscription)
        })
        .ok_or_else(|| ApiError::not_found("subscription was not selected for this laptop area"))?;
    let store = ProjectProjectionStore::open(
        state.pool().map_err(|_| unavailable())?,
        binding.subscription_id,
        binding.device_id,
        scope(&area.name, binding)?,
    )
    .await
    .map_err(|_| unavailable())?;
    let (status, hits) = store
        .search_with_status(&request.question, request.limit)
        .await
        .map_err(|_| unavailable())?;
    let hits = hits
        .into_iter()
        .map(|hit| LaptopCacheHit {
            stream_id: hit.stream.canonical.as_uuid(),
            author: hit.author.to_string(),
            authored_at: hit.authored_at,
            locator: hit.locator.to_string(),
            digest: hit.digest.as_hex().into(),
            shared_version: hit.shared_version.get(),
            source_record_id: hit.source.record_id.as_uuid(),
            source_version: hit.source.version.get(),
            text: hit.content.text,
            summary: hit.content.summary,
            evidence: hit
                .evidence
                .into_iter()
                .map(|e| LaptopCacheEvidence {
                    label: e.snapshot().public_source_label.clone(),
                    text: e.snapshot().selected_text.clone(),
                    digest: e.review().snapshot_digest.as_hex().into(),
                    reviewer: e.review().reviewer.to_string(),
                    reviewed_at: e.review().reviewed_at,
                })
                .collect(),
            read_only: true,
        })
        .collect();
    response(&LaptopCacheSearchResponse {
        schema_version: 1,
        project: describe(&area.name, binding, Some(status)),
        hits,
    })
}
