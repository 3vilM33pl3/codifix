// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separate config-bound custom delivery credential; no management role import.
use super::{
    AuthenticatedPrincipal,
    company_delegate::{self, DelegateKind},
};
use crate::{ApiError, AppState};
use axum::{Extension, Json, extract::State, http::header, response::IntoResponse};
use mem_record::*;
use serde_json::json;

#[cfg(test)]
mod tests;

pub(crate) async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
) -> Result<impl IntoResponse, ApiError> {
    let plans = company_delegate::list_plans(&state, &actor, DelegateKind::Delivery).await?;
    let delegates: Vec<_> = plans
        .into_iter()
        .map(|p| CompanyDeliveryDelegatePlan {
            company_id: p.company_id,
            owner_instance_id: p.owner_instance_id,
            area: p.area,
            issuer: p.issuer,
            principal_id: p.principal_id,
            token_id: p.token_id,
            delivery_policy_revision: p.delegation_policy_revision,
            projects: p.projects.into_iter().map(|p| p.project_id).collect(),
            permissions: vec![Permission::ScopedProjectDelivery.as_str().into()],
            status: p.status,
            expires_at: p.expires_at,
        })
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"delegates":delegates})),
    ))
}
pub(crate) async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<CompanyDeliveryDelegateProvisionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let result = provision(&state, &actor, &request).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)))
}
pub(crate) async fn provision(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    request: &CompanyDeliveryDelegateProvisionRequest,
) -> Result<CompanyDeliveryDelegateProvisionResponse, ApiError> {
    let normalized = CompanyDelegateProvisionRequest {
        request_id: request.request_id,
        company_id: request.company_id,
        owner_instance_id: request.owner_instance_id,
        area: request.area,
        issuer: request.issuer.clone(),
        principal_id: request.principal_id,
        token_id: request.token_id,
        delegation_policy_revision: request.delivery_policy_revision,
        name: request.name.clone(),
        projects: request
            .projects
            .iter()
            .map(|id| CompanyDelegateProject {
                project_id: *id,
                role_ceiling: AuthRole::Reader,
            })
            .collect(),
        expires_in_days: request.expires_in_days,
        confirmed: request.confirmed,
    };
    let p =
        company_delegate::provision_with(state, actor, &normalized, DelegateKind::Delivery).await?;
    Ok(CompanyDeliveryDelegateProvisionResponse {
        request_id: p.request_id,
        company_id: p.company_id,
        owner_instance_id: p.owner_instance_id,
        area: p.area,
        issuer: p.issuer,
        principal_id: p.principal_id,
        token_id: p.token_id,
        delivery_policy_revision: p.delegation_policy_revision,
        expires_at: p.expires_at,
        projects: p.projects.into_iter().map(|p| p.project_id).collect(),
        permissions: vec![Permission::ScopedProjectDelivery.as_str().into()],
        replayed: p.replayed,
        credential_issued: p.credential_issued,
        token: p.token,
    })
}
