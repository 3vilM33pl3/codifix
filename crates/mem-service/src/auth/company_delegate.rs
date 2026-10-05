// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit configuration-bound operator provisioning, not a general token mint.
use super::AuthenticatedPrincipal;
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use mem_record::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, types::Json as SqlJson};
use uuid::Uuid;
#[cfg(test)]
mod tests;

const PERMISSIONS: [Permission; 2] = [Permission::MemoryRead, Permission::AuthManage];
const EVENT: &str = "onboarding.company.delegate.provision";
#[derive(Clone, Copy)]
pub(super) enum DelegateKind {
    Management,
    Delivery,
}
impl DelegateKind {
    fn event(self) -> &'static str {
        match self {
            Self::Management => EVENT,
            Self::Delivery => "onboarding.company.delivery_delegate.provision",
        }
    }
    fn permissions(self) -> &'static [Permission] {
        match self {
            Self::Management => &PERMISSIONS,
            Self::Delivery => &[Permission::ScopedProjectDelivery],
        }
    }
}
fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "company delegate request, configuration or current identity changed; no credential or permissions restored",
    )
}
fn storage(_: sqlx::Error) -> ApiError {
    ApiError::service_unavailable(
        "company delegate provisioning outcome unconfirmed; retry the same request; one-time credentials are never reissued",
    )
}
fn authority(error: ApiError) -> ApiError {
    if matches!(
        error.status,
        StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
    ) {
        ApiError::forbidden("current global administration and a writable primary are required")
    } else {
        ApiError::service_unavailable("company delegate authority unavailable")
    }
}
fn area(state: &AppState) -> Result<(OnboardingArea, Uuid), ApiError> {
    state
        .config
        .auth
        .validate_company_delegations()
        .map_err(|_| ApiError::forbidden("invalid exact company delegate configuration"))?;
    state
        .config
        .validate_company_delivery_owner()
        .map_err(|_| ApiError::forbidden("invalid exact company delivery configuration"))?;
    let area = state
        .config
        .area
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("explicit owner area identity required"))?;
    area.validate()
        .map_err(|_| ApiError::forbidden("invalid owner area identity"))?;
    let name = match area.name.as_str() {
        "workshop" => OnboardingArea::Workshop,
        "homelab" => OnboardingArea::Homelab,
        "general" => OnboardingArea::General,
        _ => {
            return Err(ApiError::forbidden(
                "Trust and unknown areas cannot provision company delegates",
            ));
        }
    };
    let issuer = &state.config.auth.oidc.issuer_url;
    if !reqwest::Url::parse(issuer).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
    }) {
        return Err(ApiError::forbidden(
            "exact shared HTTPS OIDC issuer required",
        ));
    }
    Ok((name, area.instance_id))
}
fn plan(
    state: &AppState,
    cfg: &mem_config::CompanyDelegationConfig,
) -> Result<CompanyDelegatePlan, ApiError> {
    let (area, owner) = area(state)?;
    if cfg.principal_id == super::local_owner_id() {
        return Err(ApiError::forbidden("reserved delegate identity"));
    }
    let mut projects: Vec<_> = cfg
        .projects
        .iter()
        .map(|p| CompanyDelegateProject {
            project_id: p.project_id,
            role_ceiling: p.role_ceiling,
        })
        .collect();
    projects.sort_by_key(|p| p.project_id);
    Ok(CompanyDelegatePlan {
        company_id: cfg.company_id,
        owner_instance_id: owner,
        area,
        issuer: cfg.issuer.clone(),
        principal_id: cfg.principal_id,
        token_id: cfg.token_id,
        delegation_policy_revision: cfg.policy_revision,
        projects,
        status: "unprovisioned".into(),
        expires_at: None,
    })
}
fn delivery_plan(
    state: &AppState,
    cfg: &mem_config::CompanyDeliveryDelegationConfig,
) -> Result<CompanyDelegatePlan, ApiError> {
    let (area, owner) = area(state)?;
    if state.config.auth.mode != AuthMode::MultiUser
        || cfg.owner_instance_id != owner
        || cfg.area != area
        || cfg.principal_id == super::local_owner_id()
    {
        return Err(ApiError::forbidden(
            "multi-user exact configured local delivery owner required",
        ));
    }
    let mut projects: Vec<_> = cfg
        .projects
        .iter()
        .map(|id| CompanyDelegateProject {
            project_id: *id,
            role_ceiling: AuthRole::Reader,
        })
        .collect();
    projects.sort_by_key(|p| p.project_id);
    // This private common-kernel representation preserves management receipt
    // compatibility. Its reader label is not a delivery grant or role ceiling:
    // every delivery membership has ONLY the explicit custom override.
    Ok(CompanyDelegatePlan {
        company_id: cfg.company_id,
        owner_instance_id: owner,
        area,
        issuer: cfg.issuer.clone(),
        principal_id: cfg.principal_id,
        token_id: cfg.token_id,
        delegation_policy_revision: cfg.policy_revision,
        projects,
        status: "unprovisioned".into(),
        expires_at: None,
    })
}
fn same_configuration(
    request: &CompanyDelegateProvisionRequest,
    plan: &CompanyDelegatePlan,
) -> bool {
    let mut projects = request.projects.clone();
    projects.sort_by_key(|p| p.project_id);
    request.company_id == plan.company_id
        && request.owner_instance_id == plan.owner_instance_id
        && request.area == plan.area
        && request.issuer == plan.issuer
        && request.principal_id == plan.principal_id
        && request.token_id == plan.token_id
        && request.delegation_policy_revision == plan.delegation_policy_revision
        && projects == plan.projects
}
fn membership(request: Uuid, project: Uuid) -> Uuid {
    let mut name = b"company-delegate-project-membership".to_vec();
    name.extend_from_slice(project.as_bytes());
    Uuid::new_v5(&request, &name)
}
fn commitment(hash: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(hash).to_vec()
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    request: CompanyDelegateProvisionRequest,
    expires_at: DateTime<Utc>,
    credential_commitment: Vec<u8>,
}

/// Inspection checks the complete current credential envelope, not merely the
/// existence of the audit receipt. It never restores a disabled/revoked row.
async fn current(
    tx: &mut Transaction<'_, Postgres>,
    plan: &CompanyDelegatePlan,
    receipt: Option<&Receipt>,
    creator: Option<Uuid>,
    kind: DelegateKind,
) -> Result<(String, Option<DateTime<Utc>>), ApiError> {
    let principal = sqlx::query("SELECT * FROM auth_principals WHERE id=$1 FOR SHARE")
        .bind(plan.principal_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?;
    let tokens = sqlx::query(
        "SELECT * FROM auth_service_tokens WHERE principal_id=$1 OR id=$2 ORDER BY id FOR SHARE",
    )
    .bind(plan.principal_id)
    .bind(plan.token_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(storage)?;
    if principal.is_none() && tokens.is_empty() && receipt.is_none() {
        return Ok(("unprovisioned".into(), None));
    }
    let Some(principal) = principal else {
        return Ok(("conflict".into(), None));
    };
    let Some(receipt) = receipt else {
        return Ok(("conflict".into(), None));
    };
    if !same_configuration(&receipt.request, plan) || tokens.len() != 1 {
        return Ok(("conflict".into(), None));
    }
    let token = &tokens[0];
    let expiry: Option<DateTime<Utc>> = token.try_get("expires_at").map_err(storage)?;
    let expected = json!(kind.permissions());
    let identity = principal.try_get::<String, _>("kind").map_err(storage)? == "service_token"
        && principal
            .try_get::<String, _>("display_name")
            .map_err(storage)?
            == receipt.request.name
        && ["issuer", "subject", "email", "global_role"]
            .iter()
            .all(|key| principal.try_get::<Option<String>, _>(*key).ok() == Some(None))
        && principal
            .try_get::<Value, _>("groups_json")
            .map_err(storage)?
            == json!([])
        && principal
            .try_get::<Option<Value>, _>("global_permissions_json")
            .map_err(storage)?
            == Some(json!([]))
        && token.try_get::<Uuid, _>("id").map_err(storage)? == plan.token_id
        && token.try_get::<Uuid, _>("principal_id").map_err(storage)? == plan.principal_id
        && token.try_get::<String, _>("name").map_err(storage)? == receipt.request.name
        && token
            .try_get::<Option<Uuid>, _>("created_by")
            .map_err(storage)?
            == creator
        && expiry == Some(receipt.expires_at)
        && commitment(&token.try_get::<Vec<u8>, _>("token_hash").map_err(storage)?)
            == receipt.credential_commitment;
    if !identity {
        return Ok(("conflict".into(), expiry));
    }
    let rows=sqlx::query("SELECT * FROM auth_project_memberships WHERE principal_id=$1 ORDER BY project_id FOR SHARE")
        .bind(plan.principal_id).fetch_all(&mut **tx).await.map_err(storage)?;
    if rows.len() != plan.projects.len() {
        return Ok(("conflict".into(), expiry));
    }
    for (row, project) in rows.iter().zip(&plan.projects) {
        if row.try_get::<Uuid, _>("id").map_err(storage)?
            != membership(receipt.request.request_id, project.project_id)
            || row.try_get::<Uuid, _>("project_id").map_err(storage)? != project.project_id
            || row.try_get::<String, _>("role").map_err(storage)? != "reader"
            || row.try_get::<String, _>("source").map_err(storage)? != "direct"
            || row
                .try_get::<Option<Uuid>, _>("created_by")
                .map_err(storage)?
                != creator
            || row
                .try_get::<Option<Value>, _>("permissions_json")
                .map_err(storage)?
                != Some(expected.clone())
        {
            return Ok(("conflict".into(), expiry));
        }
    }
    let other:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_web_sessions WHERE principal_id=$1) OR EXISTS(SELECT 1 FROM onboarding_devices WHERE machine_principal_id=$1 OR service_token_id=$2)")
        .bind(plan.principal_id).bind(plan.token_id).fetch_one(&mut **tx).await.map_err(storage)?;
    if other {
        return Ok(("conflict".into(), expiry));
    }
    let still_live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_service_tokens WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp())")
        .bind(plan.token_id).bind(plan.principal_id).fetch_one(&mut **tx).await.map_err(storage)?;
    let disabled = principal
        .try_get::<Option<DateTime<Utc>>, _>("disabled_at")
        .map_err(storage)?
        .is_some()
        || token
            .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
            .map_err(storage)?
            .is_some()
        || !still_live;
    Ok((if disabled { "inactive" } else { "active" }.into(), expiry))
}

pub(crate) async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
) -> Result<impl IntoResponse, ApiError> {
    let plans = list_plans(&state, &actor, DelegateKind::Management).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"delegates":plans})),
    ))
}
pub(super) async fn list_plans(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    kind: DelegateKind,
) -> Result<Vec<CompanyDelegatePlan>, ApiError> {
    state
        .config
        .validate_company_delivery_owner()
        .map_err(|_| ApiError::forbidden("invalid exact company delivery configuration"))?;
    if matches!(kind, DelegateKind::Delivery) && state.config.auth.mode != AuthMode::MultiUser {
        return Err(ApiError::forbidden(
            "multi-user company delivery provisioning required",
        ));
    }
    state
        .config
        .auth
        .validate_company_delegations()
        .map_err(|_| ApiError::forbidden("invalid exact company delegate configuration"))?;
    let mut tx = state.pool()?.begin().await.map_err(storage)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    super::lock_global_administrator(state, &mut tx, actor)
        .await
        .map_err(authority)?;
    let mut plans = match kind {
        DelegateKind::Management => state
            .config
            .auth
            .company_delegations
            .iter()
            .map(|cfg| plan(state, cfg))
            .collect::<Result<Vec<_>, _>>()?,
        DelegateKind::Delivery => state
            .config
            .auth
            .company_delivery_delegations
            .iter()
            .map(|cfg| delivery_plan(state, cfg))
            .collect::<Result<Vec<_>, _>>()?,
    };
    for view in &mut plans {
        let row=sqlx::query("SELECT actor_principal_id,metadata_json FROM auth_audit_events WHERE event_type=$1 AND outcome='success' AND target_principal_id=$2 AND target_token_id=$3 ORDER BY recorded_at DESC,id LIMIT 1")
            .bind(kind.event()).bind(view.principal_id).bind(view.token_id).fetch_optional(&mut *tx).await.map_err(storage)?;
        let receipt = row
            .as_ref()
            .map(|r| {
                r.try_get::<SqlJson<Receipt>, _>("metadata_json")
                    .map(|v| v.0)
                    .map_err(storage)
            })
            .transpose()?;
        let creator = row
            .as_ref()
            .map(|r| {
                r.try_get::<Option<Uuid>, _>("actor_principal_id")
                    .map_err(storage)
            })
            .transpose()?
            .flatten();
        let (status, expiry) = current(&mut tx, view, receipt.as_ref(), creator, kind).await?;
        view.status = status;
        view.expires_at = expiry;
    }
    tx.commit().await.map_err(storage)?;
    Ok(plans)
}
pub(crate) async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<CompanyDelegateProvisionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let response = provision(&state, &actor, &request).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(response)))
}
pub(crate) async fn provision(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    request: &CompanyDelegateProvisionRequest,
) -> Result<CompanyDelegateProvisionResponse, ApiError> {
    provision_with(state, actor, request, DelegateKind::Management).await
}
pub(super) async fn provision_with(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
    request: &CompanyDelegateProvisionRequest,
    kind: DelegateKind,
) -> Result<CompanyDelegateProvisionResponse, ApiError> {
    if !request.confirmed
        || request.request_id.is_nil()
        || request.principal_id == actor.id
        || request.principal_id == super::local_owner_id()
        || request.name.trim().is_empty()
        || request.name.trim() != request.name
        || request.name.len() > 120
        || request.name.chars().any(char::is_control)
        || !(1..=90).contains(&request.expires_in_days)
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "explicit confirmation, stable identity and bounded delegate credential lifetime required",
        ));
    }
    let view = match kind {
        DelegateKind::Management => plan(
            state,
            state
                .config
                .auth
                .company_delegations
                .iter()
                .find(|c| c.company_id == request.company_id)
                .ok_or_else(|| {
                    ApiError::forbidden("company management is not configured on this owner")
                })?,
        )?,
        DelegateKind::Delivery => delivery_plan(
            state,
            state
                .config
                .auth
                .company_delivery_delegations
                .iter()
                .find(|c| c.company_id == request.company_id)
                .ok_or_else(|| {
                    ApiError::forbidden("company delivery is not configured on this owner")
                })?,
        )?,
    };
    if !same_configuration(request, &view) {
        return Err(conflict());
    }
    let mut tx = state.pool()?.begin().await.map_err(storage)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
    super::lock_global_administrator(state, &mut tx, actor)
        .await
        .map_err(authority)?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:company-delegate-provision',0))",
    )
    .execute(&mut *tx)
    .await
    .map_err(storage)?;
    let prior=sqlx::query("SELECT actor_principal_id,target_principal_id,target_token_id,event_type,outcome,metadata_json FROM auth_audit_events WHERE id=$1")
        .bind(request.request_id).fetch_optional(&mut *tx).await.map_err(storage)?;
    let replayed = prior.is_some();
    let (expiry, token) = if let Some(row) = prior {
        if row
            .try_get::<Option<Uuid>, _>("actor_principal_id")
            .map_err(storage)?
            != Some(actor.id)
            || row
                .try_get::<Option<Uuid>, _>("target_principal_id")
                .map_err(storage)?
                != Some(request.principal_id)
            || row
                .try_get::<Option<Uuid>, _>("target_token_id")
                .map_err(storage)?
                != Some(request.token_id)
            || row.try_get::<String, _>("event_type").map_err(storage)? != kind.event()
            || row.try_get::<String, _>("outcome").map_err(storage)? != "success"
        {
            return Err(conflict());
        }
        let receipt = row
            .try_get::<SqlJson<Receipt>, _>("metadata_json")
            .map_err(storage)?
            .0;
        if receipt.request != *request
            || current(&mut tx, &view, Some(&receipt), Some(actor.id), kind)
                .await?
                .0
                != "active"
        {
            return Err(conflict());
        }
        (receipt.expires_at, None)
    } else {
        // Claim only absent identities. Never adopt, overwrite or reactivate an
        // existing human, machine, device credential or manually granted row.
        let inserted=sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json,groups_json) VALUES($1,'service_token',$2,'[]','[]') ON CONFLICT DO NOTHING")
            .bind(request.principal_id).bind(&request.name).execute(&mut *tx).await.map_err(storage)?;
        if inserted.rows_affected() != 1 {
            return Err(conflict());
        }
        for project in &view.projects {
            sqlx::query("SELECT id FROM projects WHERE id=$1 FOR SHARE")
                .bind(project.project_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?
                .ok_or_else(|| ApiError::not_found("configured local project UUID not found"))?;
        }
        let expiry: DateTime<Utc> =
            sqlx::query_scalar("SELECT clock_timestamp()+($1::integer * interval '1 day')")
                .bind(i32::try_from(request.expires_in_days).map_err(|_| conflict())?)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        let raw = format!("mlt_{}", super::random_secret(32));
        let hashed = super::hash_secret(&raw);
        let inserted=sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,created_by,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(request.token_id).bind(request.principal_id).bind(&request.name).bind(&hashed).bind(raw.chars().take(12).collect::<String>()).bind(actor.id).bind(expiry).execute(&mut *tx).await.map_err(storage)?;
        if inserted.rows_affected() != 1 {
            return Err(conflict());
        }
        for project in &view.projects {
            // The display preset is reader, but the explicit override alone is
            // authoritative. Delivery receives no reader/management capability.
            sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,source,permissions_json,created_by) VALUES($1,$2,$3,'reader','direct',$4,$5)")
                .bind(membership(request.request_id,project.project_id)).bind(request.principal_id).bind(project.project_id).bind(json!(kind.permissions())).bind(actor.id).execute(&mut *tx).await.map_err(storage)?;
        }
        let receipt = Receipt {
            request: request.clone(),
            expires_at: expiry,
            credential_commitment: commitment(&hashed),
        };
        sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,target_principal_id,target_token_id,event_type,outcome,metadata_json,recorded_at) VALUES($1,$2,$3,$4,$5,'success',$6,clock_timestamp())")
            .bind(request.request_id).bind(actor.id).bind(request.principal_id).bind(request.token_id).bind(kind.event()).bind(SqlJson(receipt)).execute(&mut *tx).await.map_err(storage)?;
        (expiry, Some(raw))
    };
    super::lock_global_administrator(state, &mut tx, actor)
        .await
        .map_err(authority)?;
    tx.commit().await.map_err(storage)?;
    Ok(CompanyDelegateProvisionResponse {
        request_id: request.request_id,
        company_id: view.company_id,
        owner_instance_id: view.owner_instance_id,
        area: view.area,
        issuer: view.issuer,
        principal_id: view.principal_id,
        token_id: view.token_id,
        delegation_policy_revision: view.delegation_policy_revision,
        expires_at: expiry,
        projects: view
            .projects
            .into_iter()
            .map(|p| CompanyDelegateProjectGrant {
                project_id: p.project_id,
                role_ceiling: p.role_ceiling,
                permissions: kind
                    .permissions()
                    .iter()
                    .map(|p| p.as_str().into())
                    .collect(),
            })
            .collect(),
        replayed,
        credential_issued: token.is_some(),
        token,
    })
}
