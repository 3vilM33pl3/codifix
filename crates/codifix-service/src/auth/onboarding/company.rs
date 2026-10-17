// SPDX-License-Identifier: AGPL-3.0-or-later
//! Owner-local grants plus a journaled, explicitly configured company bridge.
use crate::{
    ApiError, AppState,
    auth::{AuthenticatedPrincipal, CredentialSource},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use codifix_record::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

mod coordinator;
pub(crate) use coordinator::*;
#[cfg(test)]
mod tests;

fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "company request, ownership or delegation revision changed",
    )
}
fn writable(state: &AppState) -> Result<(), ApiError> {
    if state.config.auth.mode != AuthMode::MultiUser
        || state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
    {
        return Err(ApiError::forbidden(
            "company delegation requires a writable multi-user primary",
        ));
    }
    Ok(())
}
fn role_level(role: AuthRole) -> Option<u8> {
    match role {
        AuthRole::Reader => Some(1),
        AuthRole::Writer => Some(2),
        AuthRole::Operator => Some(3),
        _ => None,
    }
}
fn request_digest(request: &CompanyGrantRequest) -> Result<String, ApiError> {
    let bytes =
        serde_json::to_vec(request).map_err(|_| ApiError::internal("invalid company request"))?;
    Ok(crate::auth::hash_secret(
        &String::from_utf8(bytes).map_err(|_| ApiError::internal("invalid company request"))?,
    )
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect())
}
fn validate_request(request: &CompanyGrantRequest) -> Result<(), ApiError> {
    if request.company_id.is_nil()
        || request.operation_id.is_nil()
        || request.owner_instance_id.is_nil()
        || request.project_id.is_nil()
        || request.expected_grant_revision < 0
        || request.expected_grant_revision == i64::MAX
        || request.delegation_policy_revision < 1
        || role_level(request.role).is_none()
        || !super::text(&request.subject, 512)
        || !super::safe_https(&request.issuer)
        || request.area == OnboardingArea::Trust
    {
        return Err(ApiError::status_message(
            StatusCode::BAD_REQUEST,
            "bounded exact company identity, grant revision and non-Trust reader/writer/operator role required",
        ));
    }
    Ok(())
}

pub(crate) async fn lock_grant_key(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Uuid,
    principal: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("memory:membership:{project}:{principal}"))
        .execute(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    Ok(())
}

fn delegation<'a>(
    state: &'a AppState,
    actor: &AuthenticatedPrincipal,
    company: Uuid,
) -> Result<&'a codifix_config::CompanyDelegationConfig, ApiError> {
    writable(state)?;
    if actor.kind != AuthPrincipalKind::ServiceToken
        || actor.credential_source != CredentialSource::ServiceToken
    {
        return Err(ApiError::forbidden(
            "current owner-enrolled company service credential required",
        ));
    }
    let matches: Vec<_> = state
        .config
        .auth
        .company_delegations
        .iter()
        .filter(|c| c.company_id == company)
        .collect();
    if matches.len() != 1 {
        return Err(ApiError::forbidden(
            "company is not uniquely enrolled by this owner",
        ));
    }
    let cfg = matches[0];
    if cfg.company_id.is_nil()
        || cfg.principal_id != actor.id
        || Some(cfg.token_id) != actor.token_id
        || cfg.policy_revision < 1
        || cfg.projects.len() > 256
        || cfg.issuer != codifix_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url)
        // Existing local OIDC storage uses a no-trailing-slash namespace. Do
        // not silently alias a forwarded namespace or create unreachable users.
        || cfg.issuer.ends_with('/')
        || !super::safe_https(&cfg.issuer)
    {
        return Err(ApiError::forbidden(
            "exact company delegate credential and shared issuer required",
        ));
    }
    Ok(cfg)
}
async fn authorize_delegate(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &AuthenticatedPrincipal,
    cfg: &codifix_config::CompanyDelegationConfig,
    project: Uuid,
) -> Result<(), ApiError> {
    if cfg
        .projects
        .iter()
        .filter(|p| p.project_id == project)
        .count()
        != 1
    {
        return Err(ApiError::forbidden(
            "project is outside delegated authority",
        ));
    }
    let permissions = crate::auth::locked_project_permissions(
        state,
        tx,
        &codifix_store::jobs::JobActor::Credential {
            principal: actor.id,
            token: actor.token_id,
            session: None,
        },
        project,
    )
    .await?;
    let current_kind: String =
        sqlx::query_scalar("SELECT kind FROM auth_principals WHERE id=$1 FOR SHARE")
            .bind(actor.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::sql)?;
    // A global permission, OIDC group rule or application-admin preset is not
    // an owner-enrolled project delegate. Require an explicit project row too.
    let explicit: Option<Value> = sqlx::query_scalar("SELECT permissions_json FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2 FOR SHARE")
        .bind(actor.id).bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.flatten();
    let explicit: Vec<String> = explicit
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| ApiError::forbidden("invalid explicit delegate permissions"))?
        .unwrap_or_default();
    if current_kind != "service_token"
        || !permissions.contains(Permission::AuthManage)
        || !explicit
            .iter()
            .any(|p| p == Permission::AuthManage.as_str())
    {
        return Err(ApiError::forbidden(
            "current exact-project authentication management permission required",
        ));
    }
    Ok(())
}

async fn current_access(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal: Uuid,
    project: Uuid,
) -> Result<CompanyCurrentAccess, ApiError> {
    let row=sqlx::query("SELECT disabled_at,groups_json FROM auth_principals WHERE id=$1 AND kind='human_oidc' FOR SHARE")
        .bind(principal).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("registered human identity no longer exists"))?;
    let disabled = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("disabled_at")
        .map_err(ApiError::sql)?
        .is_some();
    lock_grant_key(tx, project, principal).await?;
    // Same ownership -> direct row ordering as mutations; otherwise a status
    // read could hold the direct row while waiting on a mutator's ownership row.
    let ownership=sqlx::query("SELECT company_id,revision FROM onboarding_company_grants WHERE project_id=$1 AND principal_id=$2 FOR SHARE")
        .bind(project).bind(principal).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM projects WHERE id=$1 FOR SHARE")
        .bind(project)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    let direct:Option<String>=sqlx::query_scalar("SELECT role FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2 FOR SHARE")
        .bind(principal).bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    let groups: Vec<String> =
        serde_json::from_value(row.try_get("groups_json").map_err(ApiError::sql)?)
            .map_err(|_| ApiError::internal("invalid local human groups"))?;
    let mut group_roles = Vec::new();
    for rule in &state.config.auth.group_mappings.rules {
        if (rule.global || rule.project.as_deref() == Some(&slug))
            && groups.iter().any(|g| g == &rule.group)
            && !group_roles.contains(&rule.role)
        {
            group_roles.push(rule.role);
        }
    }
    if group_roles.len() > 16 {
        return Err(ApiError::internal(
            "effective group grants exceed bounded inspection",
        ));
    }
    let permission = if disabled {
        PermissionSet::EMPTY
    } else {
        crate::auth::locked_project_permissions(
            state,
            tx,
            &codifix_store::jobs::JobActor::Principal(principal),
            project,
        )
        .await?
    };
    let direct_role = direct
        .map(|v| serde_json::from_value(json!(v)))
        .transpose()
        .map_err(|_| ApiError::internal("invalid direct role"))?;
    Ok(CompanyCurrentAccess {
        observed_at: chrono::Utc::now(),
        principal_id: principal,
        disabled,
        project: slug,
        direct_role,
        grant_company_id: ownership
            .as_ref()
            .map(|r| r.try_get("company_id"))
            .transpose()
            .map_err(ApiError::sql)?,
        owner_grant_revision: ownership
            .as_ref()
            .map(|r| r.try_get("revision"))
            .transpose()
            .map_err(ApiError::sql)?,
        group_roles,
        permissions: permission.names(),
    })
}

pub(crate) async fn delegated_grant(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<CompanyGrantRequest>,
) -> Result<Json<CompanyGrantResult>, ApiError> {
    validate_request(&request)?;
    let cfg = delegation(&state, &actor, request.company_id)?.clone();
    let area = state
        .config
        .area
        .as_ref()
        .ok_or_else(|| ApiError::forbidden("explicit owner area identity required"))?;
    if area.instance_id != request.owner_instance_id
        || area.name != request.area.as_str()
        || area.name == "trust"
        || request.issuer != cfg.issuer
        || request.delegation_policy_revision != cfg.policy_revision
    {
        return Err(conflict());
    }
    let ceilings: Vec<_> = cfg
        .projects
        .iter()
        .filter(|p| p.project_id == request.project_id)
        .collect();
    if ceilings.len() != 1
        || role_level(ceilings[0].role_ceiling)
            .is_none_or(|ceiling| role_level(request.role).is_none_or(|r| r > ceiling))
    {
        return Err(ApiError::forbidden(
            "requested role exceeds explicit owner ceiling",
        ));
    }
    let digest = request_digest(&request)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "memory:company-operation:{}:{}",
            request.company_id, request.operation_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    // Current delegate authority is required even when returning a historical receipt.
    authorize_delegate(&state, &mut tx, &actor, &cfg, request.project_id).await?;
    let old=sqlx::query("SELECT request_sha256,receipt_json FROM onboarding_company_receipts WHERE company_id=$1 AND operation_id=$2")
        .bind(request.company_id).bind(request.operation_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    if let Some(old) = old {
        if old
            .try_get::<String, _>("request_sha256")
            .map_err(ApiError::sql)?
            != digest
        {
            return Err(conflict());
        }
        let receipt: CompanyGrantReceipt =
            serde_json::from_value(old.try_get("receipt_json").map_err(ApiError::sql)?)
                .map_err(|_| ApiError::internal("invalid owner receipt"))?;
        let current_access =
            current_access(&state, &mut tx, receipt.principal_id, request.project_id).await?;
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(CompanyGrantResult {
            receipt,
            current_access,
        }));
    }
    // The forwarded issuer+subject is trusted ONLY because this exact delegate
    // was explicitly enrolled; no email aliases or automatic reactivation.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "memory:company-human:{}:{}",
            request.issuer, request.subject
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    sqlx::query("INSERT INTO auth_principals(id,kind,issuer,subject,display_name) VALUES($1,'human_oidc',$2,$3,$4) ON CONFLICT(issuer,subject) WHERE issuer IS NOT NULL AND subject IS NOT NULL DO NOTHING")
        .bind(Uuid::new_v4()).bind(&request.issuer).bind(&request.subject).bind("Company pre-login identity").execute(&mut *tx).await.map_err(ApiError::sql)?;
    let human = sqlx::query(
        "SELECT id,kind,disabled_at FROM auth_principals WHERE issuer=$1 AND subject=$2 FOR SHARE",
    )
    .bind(&request.issuer)
    .bind(&request.subject)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if human.try_get::<String, _>("kind").map_err(ApiError::sql)? != "human_oidc"
        || human
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("disabled_at")
            .map_err(ApiError::sql)?
            .is_some()
    {
        return Err(ApiError::forbidden(
            "existing stable human identity is disabled or incompatible",
        ));
    }
    let principal: Uuid = human.try_get("id").map_err(ApiError::sql)?;
    lock_grant_key(&mut tx, request.project_id, principal).await?;
    let owned=sqlx::query("SELECT * FROM onboarding_company_grants WHERE project_id=$1 AND principal_id=$2 FOR UPDATE")
        .bind(request.project_id).bind(principal).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    let member=sqlx::query("SELECT id,role,source,permissions_json FROM auth_project_memberships WHERE project_id=$1 AND principal_id=$2 FOR UPDATE")
        .bind(request.project_id).bind(principal).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    let revision = owned
        .as_ref()
        .map(|r| r.try_get::<i64, _>("revision"))
        .transpose()
        .map_err(ApiError::sql)?
        .unwrap_or(0);
    if revision != request.expected_grant_revision {
        return Err(conflict());
    }
    if let Some(owned) = &owned {
        if owned
            .try_get::<Uuid, _>("company_id")
            .map_err(ApiError::sql)?
            != request.company_id
        {
            return Err(conflict());
        }
        let binding: Option<Uuid> = owned.try_get("membership_id").map_err(ApiError::sql)?;
        if binding
            != member
                .as_ref()
                .map(|m| m.try_get::<Uuid, _>("id"))
                .transpose()
                .map_err(ApiError::sql)?
        {
            return Err(conflict());
        }
        if let Some(member) = &member
            && (member.try_get::<String, _>("role").map_err(ApiError::sql)?
                != owned.try_get::<String, _>("role").map_err(ApiError::sql)?
                || member
                    .try_get::<String, _>("source")
                    .map_err(ApiError::sql)?
                    != "direct"
                || member
                    .try_get::<Option<Value>, _>("permissions_json")
                    .map_err(ApiError::sql)?
                    .is_some())
        {
            return Err(conflict());
        }
    } else if member.is_some() || request.operation == CompanyGrantOperation::Revoke {
        return Err(conflict());
    }
    if request.operation == CompanyGrantOperation::Revoke
        && owned.as_ref().is_some_and(|o| {
            o.try_get::<String, _>("role").ok().as_deref() != Some(request.role.as_str())
        })
    {
        return Err(conflict());
    }
    let active = request.operation == CompanyGrantOperation::Grant;
    let membership = if active {
        let id = member
            .as_ref()
            .map(|m| m.try_get::<Uuid, _>("id"))
            .transpose()
            .map_err(ApiError::sql)?
            .unwrap_or_else(Uuid::new_v4);
        sqlx::query("INSERT INTO auth_project_memberships(id,project_id,principal_id,role,source,created_by) VALUES($1,$2,$3,$4,'direct',$5) ON CONFLICT(project_id,principal_id) DO UPDATE SET role=EXCLUDED.role,updated_at=now()")
            .bind(id).bind(request.project_id).bind(principal).bind(request.role.as_str()).bind(actor.id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        Some(id)
    } else {
        if let Some(member) = &member {
            sqlx::query("DELETE FROM auth_project_memberships WHERE id=$1")
                .bind(member.try_get::<Uuid, _>("id").map_err(ApiError::sql)?)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::sql)?;
        }
        None
    };
    let next = revision.checked_add(1).ok_or_else(conflict)?;
    sqlx::query("INSERT INTO onboarding_company_grants(project_id,principal_id,company_id,revision,role,membership_id,active) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(project_id,principal_id) DO UPDATE SET revision=EXCLUDED.revision,role=EXCLUDED.role,membership_id=EXCLUDED.membership_id,active=EXCLUDED.active")
        .bind(request.project_id).bind(principal).bind(request.company_id).bind(next).bind(request.role.as_str()).bind(membership).bind(active).execute(&mut *tx).await.map_err(ApiError::sql)?;
    let receipt = CompanyGrantReceipt {
        request: request.clone(),
        request_sha256: digest.clone(),
        principal_id: principal,
        grant_revision: next,
        direct_grant_active: active,
        committed_at: chrono::Utc::now(),
    };
    sqlx::query("INSERT INTO onboarding_company_receipts(company_id,operation_id,request_sha256,request_json,receipt_json) VALUES($1,$2,$3,$4,$5)")
        .bind(request.company_id).bind(request.operation_id).bind(digest).bind(json!(request)).bind(json!(receipt)).execute(&mut *tx).await.map_err(ApiError::sql)?;
    super::audit(&mut tx,actor.id,Some(principal),"onboarding.company.grant",json!({"company_id":request.company_id,"operation_id":request.operation_id,"project_id":request.project_id,"revision":next,"active":active})).await?;
    let current_access = current_access(&state, &mut tx, principal, request.project_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(CompanyGrantResult {
        receipt,
        current_access,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompanyQuery {
    pub company_id: Uuid,
}
pub(crate) async fn delegated_status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(operation): Path<Uuid>,
    Query(query): Query<CompanyQuery>,
) -> Result<Json<CompanyGrantResult>, ApiError> {
    let cfg = delegation(&state, &actor, query.company_id)?;
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    let stored:Option<Value>=sqlx::query_scalar("SELECT receipt_json FROM onboarding_company_receipts WHERE company_id=$1 AND operation_id=$2").bind(query.company_id).bind(operation).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    let receipt: CompanyGrantReceipt = serde_json::from_value(
        stored.ok_or_else(|| ApiError::not_found("owner operation not found"))?,
    )
    .map_err(|_| ApiError::internal("invalid owner receipt"))?;
    authorize_delegate(&state, &mut tx, &actor, cfg, receipt.request.project_id).await?;
    let current_access = current_access(
        &state,
        &mut tx,
        receipt.principal_id,
        receipt.request.project_id,
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(CompanyGrantResult {
        receipt,
        current_access,
    }))
}
