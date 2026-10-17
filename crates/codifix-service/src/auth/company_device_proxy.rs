// SPDX-License-Identifier: AGPL-3.0-or-later
//! A company-enrolled laptop contacts only the company. Its bearer credential
//! never reaches an owner. Owners independently authorize every selective page.
use super::{
    AuthenticatedPrincipal,
    company_devices::{CompanyDeviceBinding, company_binding},
};
use crate::{ApiError, AppState};
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use codifix_federation::CanonicalJsonCodec;
use codifix_federation_runtime::{DeviceProjectSubscription, SignedProjectPage};
use codifix_record::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

fn denied() -> ApiError {
    ApiError::forbidden("current selected company device and owner delivery access required")
}
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "selected project delivery binding changed; explicit review required",
    )
}
fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "selective owner delivery unavailable; no owner diagnostics or alternate destination used",
    )
}
pub(super) fn binding_hash(binding: &CompanyDeviceBinding) -> Result<Vec<u8>, ApiError> {
    use sha2::{Digest, Sha256};
    CanonicalJsonCodec::encode(&json!({"device":binding.device_id,"human":binding.human_id,
        "issuer":binding.issuer,"subject":binding.subject,"company":binding.company_id,
        "manifest":binding.manifest,"approval":binding.approval,"bootstrap":binding.bootstrap,
        "registry":binding.registry_sha256,"owner":binding.owner}))
    .map(|v| Sha256::digest(v).to_vec())
    .map_err(|_| changed())
}
fn origin(owner: &codifix_config::CompanyOwnerConfig) -> Result<reqwest::Url, ApiError> {
    let url = reqwest::Url::parse(&owner.base_url).map_err(|_| denied())?;
    if !(url.scheme() == "https"
        || (cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
        || owner.area == OnboardingArea::Trust
        || owner.owner_instance_id.is_nil()
    {
        return Err(denied());
    }
    Ok(url)
}
pub(super) async fn post<T: Serialize, R: DeserializeOwned>(
    owner: &codifix_config::CompanyOwnerConfig,
    path: &str,
    request: &T,
    max: usize,
) -> Result<R, ApiError> {
    let env = owner.delivery_token_env.as_ref().ok_or_else(denied)?;
    if env.is_empty()
        || env.len() > 128
        || !env.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || env == &owner.token_env
        || owner.delivery_policy_revision.is_none_or(|v| v < 1)
    {
        return Err(denied());
    }
    // One credential snapshot per request. No incoming authorization/cookie is
    // inherited, and no management credential or password fallback exists.
    let token = std::env::var(env).map_err(|_| unavailable())?;
    if !(32..=4096).contains(&token.len())
        || token.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(unavailable());
    }
    let mut url = origin(owner)?;
    url.set_path(path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| unavailable())?;
    let mut response = client
        .post(url)
        .bearer_auth(token)
        .json(request)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::UNAUTHORIZED {
        return Err(denied());
    }
    if response.status() == StatusCode::CONFLICT {
        return Err(changed());
    }
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > max as u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > max {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}

pub(super) async fn subscription(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    area: &str,
    owner: Uuid,
    project: Uuid,
) -> Result<Option<Response>, ApiError> {
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let Some(binding) = company_binding(state, &mut tx, actor, area, owner, project).await? else {
        return Ok(None);
    };
    let before = binding_hash(&binding)?;
    tx.commit().await.map_err(ApiError::sql)?;
    let request = OwnerDeviceSubscriptionRequest {
        approval_id: binding.approval.request.operation_id,
    };
    let response: DeviceProjectSubscription = post(
        &binding.owner,
        "/v1/onboarding/company-delivery/subscription",
        &request,
        64 * 1024,
    )
    .await?;
    response.validate().map_err(|_| changed())?;
    // Public keys are learned only during confirmed enrollment from a pinned
    // control plane. A later HTTP response cannot silently replace that trust.
    if response != binding.bootstrap {
        return Err(changed());
    }
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let after = company_binding(state, &mut tx, actor, area, owner, project)
        .await?
        .ok_or_else(denied)?;
    if binding_hash(&after)? != before {
        return Err(changed());
    }
    let bytes = serde_json::to_vec(&response).map_err(|_| unavailable())?;
    super::onboarding::audit(
        &mut tx,
        actor.id,
        Some(after.human_id),
        "onboarding.company.subscription",
        json!({"device_id":after.device_id,"owner":owner,"project_id":project}),
    )
    .await?;
    company_binding(state, &mut tx, actor, area, owner, project)
        .await?
        .ok_or_else(denied)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Some(
        (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
    ))
}

pub(super) async fn snapshot(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    area: &str,
    owner: Uuid,
    project: Uuid,
    request: OnboardingProjectSnapshotRequest,
) -> Result<Option<Response>, ApiError> {
    if request.nonce.is_nil()
        || !(1..=32).contains(&request.limit)
        || request.after > i64::MAX as u64
        || request
            .through
            .is_some_and(|v| v < request.after || v > i64::MAX as u64)
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "invalid bounded selected project cursor",
        ));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let Some(binding) = company_binding(state, &mut tx, actor, area, owner, project).await? else {
        return Ok(None);
    };
    let before = binding_hash(&binding)?;
    let bound = json!({"area":area,"owner":owner,"project":project,"approval":binding.approval.request.operation_id,"request":request});
    sqlx::query("INSERT INTO onboarding_company_device_delivery_requests(device_id,nonce,request_json) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(binding.device_id).bind(request.nonce).bind(&bound).execute(&mut *tx).await.map_err(ApiError::sql)?;
    let saved: Value = sqlx::query_scalar("SELECT request_json FROM onboarding_company_device_delivery_requests WHERE device_id=$1 AND nonce=$2")
        .bind(binding.device_id).bind(request.nonce).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    if saved != bound {
        return Err(changed());
    }
    tx.commit().await.map_err(ApiError::sql)?;
    let transport = OwnerDeviceSnapshotRequest {
        approval_id: binding.approval.request.operation_id,
        snapshot: request.clone(),
    };
    let signed: SignedProjectPage = post(
        &binding.owner,
        "/v1/onboarding/company-delivery/snapshot",
        &transport,
        4 * 1024 * 1024,
    )
    .await?;
    let expected = signed.claims.page.request.clone();
    if expected.device != binding.device_id
        || expected.scope != binding.bootstrap.scope
        || expected.catalog_revision != binding.bootstrap.catalog_revision
        || expected.nonce != request.nonce
        || expected.after != request.after
        || expected.through != request.through
        || expected.limit != request.limit
    {
        return Err(changed());
    }
    let trust = binding.bootstrap.validate().map_err(|_| changed())?;
    trust
        .verify_project_page(signed.clone(), &expected, binding.bootstrap.authority)
        .map_err(|_| changed())?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let after = company_binding(state, &mut tx, actor, area, owner, project)
        .await?
        .ok_or_else(denied)?;
    if binding_hash(&after)? != before {
        return Err(changed());
    }
    let bytes = serde_json::to_vec(&signed).map_err(|_| unavailable())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(unavailable());
    }
    super::onboarding::audit(&mut tx,actor.id,Some(after.human_id),"onboarding.company.snapshot",json!({"device_id":after.device_id,"owner":owner,"project_id":project,"nonce":request.nonce,"next":signed.claims.page.next})).await?;
    company_binding(state, &mut tx, actor, area, owner, project)
        .await?
        .ok_or_else(denied)?;
    trust
        .verify_project_page(signed, &expected, binding.bootstrap.authority)
        .map_err(|_| changed())?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Some(
        (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
    ))
}
