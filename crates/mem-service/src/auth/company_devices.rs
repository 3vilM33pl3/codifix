// SPDX-License-Identifier: AGPL-3.0-or-later
//! Company enrollment journals owner effects before locally activating a token.
use super::{AuthenticatedPrincipal, CredentialSource, company_catalog, device};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use mem_federation_runtime::DeviceProjectSubscription;
use mem_record::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow, types::Json as SqlJson};
use uuid::Uuid;

mod enrollment;
#[cfg(test)]
pub(crate) mod tests;
mod transport;
pub(crate) use enrollment::approve;

fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "company enrollment request, identity, selections or registry changed; no device authority restored",
    )
}
fn denied() -> ApiError {
    ApiError::forbidden("current exact company device enrollment and human identity required")
}
fn uncertain(request: Uuid) -> ApiError {
    ApiError::service_unavailable(&format!(
        "company enrollment confirmation uncertain; retry the exact same approval; request {request}"
    ))
}
fn expired() -> ApiError {
    ApiError::status_message(
        StatusCode::GONE,
        "company device challenge is expired or cancelled",
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Identity {
    id: Uuid,
    issuer: String,
    subject: String,
}
async fn human(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
) -> Result<Identity, ApiError> {
    device::configuration(state)?;
    device::lock_human(state, tx, actor).await?;
    let row=sqlx::query("SELECT issuer,subject FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE").bind(actor.id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    let issuer: String = row
        .try_get::<Option<String>, _>("issuer")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let subject: String = row
        .try_get::<Option<String>, _>("subject")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    if issuer != mem_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
        || subject.is_empty()
        || subject.len() > 512
        || subject.chars().any(char::is_control)
    {
        return Err(denied());
    }
    Ok(Identity {
        id: actor.id,
        issuer,
        subject,
    })
}
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
struct Source {
    request_id: Uuid,
    device_id: Uuid,
    user_code: String,
    name: String,
    platform: String,
    credential_sha256: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}
fn source(row: &PgRow) -> Result<Source, ApiError> {
    Ok(Source {
        request_id: row.try_get("request_id").map_err(ApiError::sql)?,
        device_id: row.try_get("device_id").map_err(ApiError::sql)?,
        user_code: row.try_get("user_code").map_err(ApiError::sql)?,
        name: row.try_get("name").map_err(ApiError::sql)?,
        platform: row.try_get("platform").map_err(ApiError::sql)?,
        credential_sha256: hex(&row
            .try_get::<Vec<u8>, _>("credential_hash")
            .map_err(ApiError::sql)?),
        created_at: row.try_get("created_at").map_err(ApiError::sql)?,
        expires_at: row.try_get("expires_at").map_err(ApiError::sql)?,
    })
}
async fn request_row(tx: &mut Transaction<'_, Postgres>, code: &str) -> Result<PgRow, ApiError> {
    sqlx::query("SELECT *,expires_at>clock_timestamp() AS active FROM onboarding_device_requests WHERE user_code=$1 FOR UPDATE").bind(code).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("device confirmation code not found"))
}
#[derive(Serialize, Deserialize, Clone)]
struct Binding {
    manifest: OnboardingProject,
    request: OwnerDeviceApprovalRequest,
}
struct Intent {
    request: CompanyDeviceApproveRequest,
    source_sha256: String,
    identity: Identity,
    company_id: Uuid,
    device_id: Uuid,
    registry_sha256: String,
    grant_until: DateTime<Utc>,
    bindings: Vec<Binding>,
}
async fn load_intent(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<Option<Intent>, ApiError> {
    let row = sqlx::query(
        "SELECT * FROM onboarding_company_device_intents WHERE request_id=$1 FOR SHARE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    row.map(|r| {
        let request = r
            .try_get::<SqlJson<CompanyDeviceApproveRequest>, _>("approve_json")
            .map_err(ApiError::sql)?
            .0;
        if company_catalog::digest(&request)?
            != r.try_get::<String, _>("approve_sha256")
                .map_err(ApiError::sql)?
        {
            return Err(changed());
        }
        Ok(Intent {
            request,
            source_sha256: r.try_get("source_sha256").map_err(ApiError::sql)?,
            identity: Identity {
                id: r.try_get("human_id").map_err(ApiError::sql)?,
                issuer: r.try_get("issuer").map_err(ApiError::sql)?,
                subject: r.try_get("subject").map_err(ApiError::sql)?,
            },
            company_id: r.try_get("company_id").map_err(ApiError::sql)?,
            device_id: r.try_get("device_id").map_err(ApiError::sql)?,
            registry_sha256: r.try_get("registry_sha256").map_err(ApiError::sql)?,
            grant_until: r.try_get("grant_until").map_err(ApiError::sql)?,
            bindings: r
                .try_get::<SqlJson<Vec<Binding>>, _>("bindings_json")
                .map_err(ApiError::sql)?
                .0,
        })
    })
    .transpose()
}
fn match_intent(
    state: &AppState,
    intent: &Intent,
    request: &CompanyDeviceApproveRequest,
    source: &Source,
    identity: &Identity,
) -> Result<(), ApiError> {
    if intent.request != *request
        || intent.identity != *identity
        || intent.device_id != source.device_id
        || intent.source_sha256 != company_catalog::digest(source)?
        || intent.registry_sha256 != company_catalog::registry_sha256(state)?
    {
        return Err(changed());
    }
    Ok(())
}

pub(crate) async fn review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Query(query): Query<device::ReviewQuery>,
) -> Result<Json<CompanyDeviceReview>, ApiError> {
    device::code(&query.code)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let before = human(&state, &mut tx, &actor).await?;
    let row = request_row(&mut tx, &query.code).await?;
    let saved = source(&row)?;
    if row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending"
        || !row.try_get::<bool, _>("active").map_err(ApiError::sql)?
    {
        return Err(expired());
    }
    let registry = company_catalog::registry_sha256(&state)?;
    tx.commit().await.map_err(ApiError::sql)?;
    let response = company_catalog::catalog(State(state.clone()), Extension(actor.clone())).await?;
    let bytes = axum::body::to_bytes(response.into_body(), 3 * 512 * 1024)
        .await
        .map_err(|_| uncertain(saved.request_id))?;
    let catalog: CompanyDeviceCatalogResponse =
        serde_json::from_slice(&bytes).map_err(|_| uncertain(saved.request_id))?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    if human(&state, &mut tx, &actor).await? != before
        || company_catalog::registry_sha256(&state)? != registry
    {
        return Err(changed());
    }
    let row = request_row(&mut tx, &query.code).await?;
    if source(&row)? != saved
        || row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending"
        || !row.try_get::<bool, _>("active").map_err(ApiError::sql)?
    {
        return Err(expired());
    }
    human(&state, &mut tx, &actor).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(CompanyDeviceReview {
        request_id: saved.request_id,
        device_id: saved.device_id,
        user_code: saved.user_code,
        name: saved.name,
        platform: saved.platform,
        credential_sha256: saved.credential_sha256,
        expires_at: saved.expires_at,
        company_id: catalog.company_id,
        observed_at: catalog.observed_at,
        projects: catalog.projects,
    }))
}

/// Internal server binding only; no credential or owner endpoint is sent to a
/// laptop. The caller must retain these locks through its final delivery commit.
pub(super) struct CompanyDeviceBinding {
    pub device_id: Uuid,
    pub human_id: Uuid,
    pub issuer: String,
    pub subject: String,
    pub company_id: Uuid,
    pub manifest: OnboardingProject,
    pub approval: OwnerDeviceApprovalReceipt,
    pub bootstrap: DeviceProjectSubscription,
    pub registry_sha256: String,
    pub owner: mem_config::CompanyOwnerConfig,
    pub machine_principal_id: Uuid,
    pub service_token_id: Uuid,
}
pub(super) async fn current_owner(binding: &CompanyDeviceBinding) -> Result<(), ApiError> {
    transport::current_bootstrap(&binding.owner, &binding.approval, &binding.bootstrap).await
}
pub(super) async fn company_binding(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    area: &str,
    owner: Uuid,
    project: Uuid,
) -> Result<Option<CompanyDeviceBinding>, ApiError> {
    if actor.kind != AuthPrincipalKind::ServiceToken
        || actor.credential_source != CredentialSource::ServiceToken
        || actor.token_id.is_none()
    {
        return Err(denied());
    }
    // Probe without locks solely to choose the fixed human-first lock order.
    let probe=sqlx::query("SELECT d.id,d.principal_id,r.request_id,i.human_id FROM onboarding_devices d JOIN onboarding_device_requests r ON r.device_id=d.id LEFT JOIN onboarding_company_device_intents i ON i.request_id=r.request_id WHERE d.machine_principal_id=$1 OR d.service_token_id=$2")
        .bind(actor.id).bind(actor.token_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    let Some(probe) = probe else { return Ok(None) };
    let Some(human_id) = probe
        .try_get::<Option<Uuid>, _>("human_id")
        .map_err(ApiError::sql)?
    else {
        return Ok(None);
    };
    let binding = parent_binding(
        state,
        tx,
        probe.try_get("id").map_err(ApiError::sql)?,
        human_id,
        area,
        owner,
        project,
    )
    .await?;
    if binding.machine_principal_id != actor.id || Some(binding.service_token_id) != actor.token_id
    {
        return Err(denied());
    }
    Ok(Some(binding))
}

/// Current persisted parent relation, independent of possession of its bearer.
/// Only internal child/browser paths may supply these IDs after verifying the
/// immutable child relation or the fresh owning human. Never synthesize an actor.
pub(super) async fn parent_binding(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    device_id: Uuid,
    human_id: Uuid,
    area: &str,
    owner: Uuid,
    project: Uuid,
) -> Result<CompanyDeviceBinding, ApiError> {
    let human=sqlx::query("SELECT issuer,subject FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE").bind(human_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    let row=sqlx::query("SELECT d.*,r.request_id,r.status,r.approved_by,r.machine_principal_id AS decided_machine,r.selection_json,r.credential_hash FROM onboarding_devices d JOIN onboarding_device_requests r ON r.device_id=d.id WHERE d.id=$1 FOR SHARE OF d,r")
        .bind(device_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    let machine_principal_id = row
        .try_get::<Option<Uuid>, _>("machine_principal_id")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let service_token_id = row
        .try_get::<Option<Uuid>, _>("service_token_id")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let device_id: Uuid = row.try_get("id").map_err(ApiError::sql)?;
    let request_id: Uuid = row.try_get("request_id").map_err(ApiError::sql)?;
    let intent = load_intent(tx, request_id).await?.ok_or_else(denied)?;
    let issuer = human
        .try_get::<Option<String>, _>("issuer")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let subject = human
        .try_get::<Option<String>, _>("subject")
        .map_err(ApiError::sql)?
        .ok_or_else(denied)?;
    let registry_sha256 = company_catalog::registry_sha256(state)?;
    if intent.identity.id != human_id
        || intent.identity.issuer != issuer
        || intent.identity.subject != subject
        || issuer != mem_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
        || intent.registry_sha256 != registry_sha256
        || intent.device_id != device_id
        || row
            .try_get::<Uuid, _>("principal_id")
            .map_err(ApiError::sql)?
            != human_id
        || row
            .try_get::<Option<Uuid>, _>("approved_by")
            .map_err(ApiError::sql)?
            != Some(human_id)
        || row
            .try_get::<Option<Uuid>, _>("decided_machine")
            .map_err(ApiError::sql)?
            != Some(machine_principal_id)
        || row
            .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
            .map_err(ApiError::sql)?
            .is_some()
        || row.try_get::<String, _>("status").map_err(ApiError::sql)? != "approved"
        || row
            .try_get::<serde_json::Value, _>("selection_json")
            .map_err(ApiError::sql)?
            != json!([])
    {
        return Err(denied());
    }
    let live=sqlx::query("SELECT t.token_hash,t.expires_at,p.global_role,p.global_permissions_json,p.groups_json FROM auth_principals p JOIN auth_service_tokens t ON t.principal_id=p.id WHERE p.id=$1 AND p.kind='service_token' AND p.disabled_at IS NULL AND t.id=$2 AND t.revoked_at IS NULL AND t.expires_at>clock_timestamp() FOR SHARE OF p,t")
        .bind(machine_principal_id).bind(service_token_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    if live
        .try_get::<Vec<u8>, _>("token_hash")
        .map_err(ApiError::sql)?
        != row
            .try_get::<Vec<u8>, _>("credential_hash")
            .map_err(ApiError::sql)?
        || live
            .try_get::<Option<DateTime<Utc>>, _>("expires_at")
            .map_err(ApiError::sql)?
            != Some(intent.grant_until)
        || live
            .try_get::<Option<String>, _>("global_role")
            .map_err(ApiError::sql)?
            .is_some()
        || live
            .try_get::<Option<serde_json::Value>, _>("global_permissions_json")
            .map_err(ApiError::sql)?
            != Some(json!([]))
        || live
            .try_get::<serde_json::Value, _>("groups_json")
            .map_err(ApiError::sql)?
            != json!([])
    {
        return Err(denied());
    }
    let ordinary: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth_project_memberships WHERE principal_id=$1)",
    )
    .bind(machine_principal_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    if ordinary {
        return Err(denied());
    }
    let selection=sqlx::query("SELECT * FROM onboarding_company_device_selections WHERE device_id=$1 AND area=$2 AND owner_instance_id=$3 AND project_id=$4 FOR SHARE").bind(device_id).bind(area).bind(owner).bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    let manifest = selection
        .try_get::<SqlJson<OnboardingProject>, _>("manifest_json")
        .map_err(ApiError::sql)?
        .0;
    let approval = selection
        .try_get::<SqlJson<OwnerDeviceApprovalReceipt>, _>("approval_json")
        .map_err(ApiError::sql)?
        .0;
    let bootstrap = selection
        .try_get::<SqlJson<DeviceProjectSubscription>, _>("bootstrap_json")
        .map_err(ApiError::sql)?
        .0;
    let cfg = company_catalog::delivery_owner(state, area, owner)?;
    let selected = intent
        .bindings
        .iter()
        .find(|b| {
            b.request.area.as_str() == area
                && b.request.owner_instance_id == owner
                && b.request.project_id == project
        })
        .ok_or_else(denied)?;
    if selection
        .try_get::<Uuid, _>("request_id")
        .map_err(ApiError::sql)?
        != request_id
        || selection
            .try_get::<String, _>("registry_sha256")
            .map_err(ApiError::sql)?
            != registry_sha256
        || selection
            .try_get::<DateTime<Utc>, _>("grant_until")
            .map_err(ApiError::sql)?
            != intent.grant_until
        || company_catalog::digest(&manifest)? != selected.request.manifest_sha256
        || approval.request != selected.request
    {
        return Err(denied());
    }
    transport::validate_receipt(&selected.request, &approval)?;
    transport::validate_bootstrap(&selected.request, &bootstrap)?;
    let still_live: bool = sqlx::query_scalar("SELECT $1::timestamptz>clock_timestamp()")
        .bind(intent.grant_until)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    if !still_live {
        return Err(denied());
    }
    Ok(CompanyDeviceBinding {
        device_id,
        human_id,
        issuer,
        subject,
        company_id: intent.company_id,
        manifest,
        approval,
        bootstrap,
        registry_sha256,
        owner: cfg,
        machine_principal_id,
        service_token_id,
    })
}

/// None is the legacy local-device path only. An enrolled company device never
/// falls back to a local UUID/slug catalog, including an empty or denied scope.
pub(super) async fn catalog_for_device(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
) -> Result<Option<Vec<OnboardingDeviceReviewProject>>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let enrolled:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_devices d JOIN onboarding_company_device_intents i ON i.device_id=d.id WHERE d.machine_principal_id=$1 OR d.service_token_id=$2)").bind(actor.id).bind(actor.token_id).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    if !enrolled {
        return Ok(None);
    }
    let rows=sqlx::query("SELECT s.area,s.owner_instance_id,s.project_id FROM onboarding_company_device_selections s JOIN onboarding_devices d ON d.id=s.device_id WHERE d.machine_principal_id=$1 OR d.service_token_id=$2 ORDER BY s.area,s.owner_instance_id,s.project_id LIMIT 33").bind(actor.id).bind(actor.token_id).fetch_all(&mut *tx).await.map_err(ApiError::sql)?;
    if rows.is_empty() || rows.len() > 32 {
        return Err(denied());
    }
    let mut bindings = Vec::new();
    for row in rows {
        let binding = company_binding(
            state,
            &mut tx,
            actor,
            &row.try_get::<String, _>("area").map_err(ApiError::sql)?,
            row.try_get("owner_instance_id").map_err(ApiError::sql)?,
            row.try_get("project_id").map_err(ApiError::sql)?,
        )
        .await?
        .ok_or_else(denied)?;
        bindings.push(binding);
    }
    tx.commit().await.map_err(ApiError::sql)?;
    for binding in &bindings {
        transport::current_bootstrap(&binding.owner, &binding.approval, &binding.bootstrap).await?;
    }
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let mut projects = Vec::new();
    for saved in bindings {
        let current = company_binding(
            state,
            &mut tx,
            actor,
            saved.manifest.entry.area.as_str(),
            saved.manifest.entry.owner_instance_id,
            saved.manifest.project_id,
        )
        .await?
        .ok_or_else(denied)?;
        if current.approval != saved.approval
            || current.bootstrap != saved.bootstrap
            || current.registry_sha256 != saved.registry_sha256
            || company_catalog::digest(&current.manifest)?
                != company_catalog::digest(&saved.manifest)?
        {
            return Err(changed());
        }
        projects.push(OnboardingDeviceReviewProject {
            curated_contributions_allowed: current.approval.request.curated_contributions_allowed,
            manifest: current.manifest,
            allowed_roles: vec![AuthRole::Reader],
            offline_available: true,
        });
    }
    // Serialize bounded metadata before releasing authorization locks.
    if serde_json::to_vec(&projects).map_err(|_| denied())?.len() > 3 * 512 * 1024 {
        return Err(denied());
    }
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Some(projects))
}
