// SPDX-License-Identifier: AGPL-3.0-or-later
//! Company metadata aggregation uses only server-side delivery delegates.
//! No local grant, remote device enrollment or content proxy is created.
use super::{AuthenticatedPrincipal, CredentialSource};
use crate::{ApiError, AppState};
use axum::{
    Extension,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use codifix_federation::CanonicalJsonCodec;
use codifix_record::*;
use reqwest::{Client, Url};
use serde::Serialize;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use std::{collections::BTreeSet, time::Duration};
use uuid::Uuid;

#[cfg(test)]
mod tests;
const OWNER_BYTES: usize = 512 * 1024;
const AGGREGATE_BYTES: usize = 3 * OWNER_BYTES;

fn denied() -> ApiError {
    ApiError::forbidden(
        "current human browser identity and exact company delivery registry required",
    )
}
fn unavailable() -> ApiError {
    ApiError::service_unavailable(
        "authorized company catalog unavailable; no partial owner catalog returned",
    )
}
fn invalid() -> ApiError {
    ApiError::service_unavailable(
        "owner catalog identity or reviewed manifest could not be verified; no partial catalog returned",
    )
}
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "company catalog human identity or owner registry changed; refresh before retrying",
    )
}
pub(super) fn digest<T: Serialize>(value: &T) -> Result<String, ApiError> {
    use sha2::{Digest, Sha256};
    CanonicalJsonCodec::encode(value)
        .map(|v| {
            Sha256::digest(v)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        })
        .map_err(|_| invalid())
}
fn bounded_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn issuer(value: &str) -> bool {
    !value.ends_with('/')
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        && Url::parse(value).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
        })
}
pub(super) fn origin(value: &str) -> Result<Url, ApiError> {
    let url = Url::parse(value).map_err(|_| denied())?;
    if !(url.scheme() == "https"
        || (cfg!(test)
            && url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost"))))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(denied());
    }
    Ok(url)
}

#[derive(Serialize)]
struct Owner {
    instance: Uuid,
    area: OnboardingArea,
    issuer: String,
    policy: i64,
    origin: String,
    credential_env: String,
}
struct Registry {
    company: Uuid,
    owners: Vec<Owner>,
    fingerprint: String,
}
fn registry(state: &AppState) -> Result<Registry, ApiError> {
    if state.config.auth.mode != AuthMode::MultiUser
        || state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
    {
        return Err(denied());
    }
    state
        .config
        .auth
        .validate_company_delivery_delegations()
        .map_err(|_| denied())?;
    let cfg = state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .ok_or_else(denied)?;
    if cfg.company_id.is_nil()
        || cfg.owners.len() > 16
        || !issuer(codifix_config::oidc_principal_namespace(
            &state.config.auth.oidc.issuer_url,
        ))
    {
        return Err(denied());
    }
    let mut identities = BTreeSet::new();
    let mut owners = Vec::new();
    for o in &cfg.owners {
        if o.owner_instance_id.is_nil()
            || !identities.insert(o.owner_instance_id)
            || o.issuer
                != codifix_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
        {
            return Err(denied());
        }
        if o.area == OnboardingArea::Trust {
            continue;
        }
        let (Some(env), Some(policy)) = (&o.delivery_token_env, o.delivery_policy_revision) else {
            continue;
        };
        let url = origin(&o.base_url)?;
        owners.push(Owner {
            instance: o.owner_instance_id,
            area: o.area,
            issuer: o.issuer.clone(),
            policy,
            origin: url.as_str().into(),
            credential_env: env.clone(),
        });
    }
    if owners.len() > 3 {
        return Err(denied());
    }
    owners.sort_by_key(|o| o.instance);
    let fingerprint = digest(&json!({"company":cfg.company_id,"owners":owners}))?;
    Ok(Registry {
        company: cfg.company_id,
        owners,
        fingerprint,
    })
}
pub(super) fn registry_sha256(state: &AppState) -> Result<String, ApiError> {
    Ok(registry(state)?.fingerprint)
}
pub(super) fn delivery_owner(
    state: &AppState,
    area: &str,
    owner: Uuid,
) -> Result<codifix_config::CompanyOwnerConfig, ApiError> {
    let registry = registry(state)?;
    if !registry
        .owners
        .iter()
        .any(|o| o.instance == owner && o.area.as_str() == area)
    {
        return Err(denied());
    }
    state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .and_then(|c| {
            c.owners
                .iter()
                .find(|o| o.owner_instance_id == owner && o.area.as_str() == area)
        })
        .cloned()
        .ok_or_else(denied)
}

#[derive(PartialEq, Eq)]
struct Human {
    issuer: String,
    subject: String,
}
async fn human(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
) -> Result<Human, ApiError> {
    if actor.kind != AuthPrincipalKind::HumanOidc
        || actor.credential_source != CredentialSource::BrowserSession
        || actor.session_id.is_none()
    {
        return Err(denied());
    }
    let row=sqlx::query("SELECT issuer,subject FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE").bind(actor.id).fetch_optional(&mut **tx).await.map_err(|_|unavailable())?.ok_or_else(||ApiError::unauthorized("human identity is disabled or missing"))?;
    let issuer: String = row
        .try_get::<Option<String>, _>("issuer")
        .map_err(|_| unavailable())?
        .ok_or_else(denied)?;
    let subject: String = row
        .try_get::<Option<String>, _>("subject")
        .map_err(|_| unavailable())?
        .ok_or_else(denied)?;
    if issuer != codifix_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
        || !bounded_text(&subject, 512)
    {
        return Err(denied());
    }
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_web_sessions WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR SHARE)").bind(actor.session_id).bind(actor.id).fetch_one(&mut **tx).await.map_err(|_|unavailable())?;
    if !live {
        return Err(ApiError::unauthorized(
            "human browser session is expired or revoked",
        ));
    }
    Ok(Human { issuer, subject })
}
fn credential(owner: &Owner) -> Result<String, ApiError> {
    let secret = std::env::var(&owner.credential_env).map_err(|_| unavailable())?;
    if !(32..=4096).contains(&secret.len())
        || secret.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(unavailable());
    }
    Ok(secret)
}
async fn fetch(
    client: &Client,
    owner: &Owner,
    request: &OwnerDeviceCatalogRequest,
) -> Result<OwnerDeviceCatalogResponse, ApiError> {
    let mut url = origin(&owner.origin)?;
    url.set_path("/v1/onboarding/company-delivery/catalog");
    // New dedicated client: no browser/device/management header inheritance.
    let mut response = client
        .post(url)
        .bearer_auth(credential(owner)?)
        .json(request)
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > OWNER_BYTES as u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > OWNER_BYTES {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}
fn validate(
    owner: &Owner,
    request: &OwnerDeviceCatalogRequest,
    response: &OwnerDeviceCatalogResponse,
    started: DateTime<Utc>,
) -> Result<(), ApiError> {
    if response.request_sha256 != digest(request)?
        || response.projects.len() > 256
        || response.observed_at < started - chrono::Duration::minutes(5)
        || response.observed_at > Utc::now() + chrono::Duration::minutes(5)
    {
        return Err(invalid());
    }
    let mut projects = BTreeSet::new();
    for item in &response.projects {
        let manifest = &item.project.manifest;
        if manifest.project_id.is_nil()
            || !projects.insert(manifest.project_id)
            || manifest.entry.owner_instance_id != owner.instance
            || manifest.entry.area != owner.area
            || manifest.entry.area == OnboardingArea::Trust
            || manifest.revision < 1
            || manifest.reviewed_by.is_nil()
            || !bounded_text(&manifest.project, 128)
            || manifest.reviewed_at > response.observed_at + chrono::Duration::minutes(5)
            || item.project.allowed_roles != vec![AuthRole::Reader]
            || (item.project.offline_available && !manifest.entry.offline_allowed)
            || item.manifest_sha256 != digest(manifest)?
        {
            return Err(invalid());
        }
        super::onboarding::validate_entry(&manifest.entry).map_err(|_| invalid())?;
    }
    Ok(())
}
pub(crate) async fn catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
) -> Result<Response, ApiError> {
    let selected = registry(&state)?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(|_| unavailable())?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
    let before = human(&state, &mut tx, &actor).await?;
    tx.commit().await.map_err(|_| unavailable())?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| unavailable())?;
    let started = Utc::now();
    // At most three bounded concurrent requests, with no SQL lock across HTTP.
    let batches = futures_util::future::try_join_all(selected.owners.iter().map(|owner| {
        let client = &client;
        let identity = &before;
        let company = selected.company;
        async move {
            let request = OwnerDeviceCatalogRequest {
                company_id: company,
                owner_instance_id: owner.instance,
                area: owner.area,
                issuer: identity.issuer.clone(),
                subject: identity.subject.clone(),
                delivery_policy_revision: owner.policy,
            };
            let response = fetch(client, owner, &request).await?;
            validate(owner, &request, &response, started)?;
            Ok::<_, ApiError>(response.projects)
        }
    }))
    .await?;
    let projects = batches.into_iter().flatten().collect();
    let mut tx = pool.begin().await.map_err(|_| unavailable())?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
    if human(&state, &mut tx, &actor).await? != before
        || registry(&state)?.fingerprint != selected.fingerprint
    {
        return Err(changed());
    }
    let observed_at = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| unavailable())?;
    let response = CompanyDeviceCatalogResponse {
        company_id: selected.company,
        projects,
        observed_at,
    };
    let bytes = serde_json::to_vec(&response).map_err(|_| invalid())?;
    if bytes.len() > AGGREGATE_BYTES {
        return Err(ApiError::status_message(
            StatusCode::PAYLOAD_TOO_LARGE,
            "authorized company catalog exceeds bounded aggregate size",
        ));
    }
    super::onboarding::audit(&mut tx,actor.id,None,"onboarding.company.catalog",json!({"company_id":selected.company,"owners":selected.owners.len(),"projects":response.projects.len()})).await.map_err(|_|unavailable())?;
    // Final wall-clock check catches session expiry during lock/audit waits.
    human(&state, &mut tx, &actor).await?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
