// SPDX-License-Identifier: AGPL-3.0-or-later
//! Global administration only. Catalog metadata is not an authorization grant.
use super::{AuthenticatedPrincipal, lock_global_administrator};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use mem_record::*;
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

pub(crate) mod company;
mod provider;
#[cfg(test)]
mod tests;

fn bad(message: &str) -> ApiError {
    ApiError::status_message(StatusCode::BAD_REQUEST, message)
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn safe_https(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

pub(super) async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Uuid,
    target: Option<Uuid>,
    event: &str,
    metadata: Value,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,target_principal_id,event_type,outcome,metadata_json) VALUES($1,$2,$3,$4,'success',$5)")
        .bind(Uuid::new_v4()).bind(actor).bind(target).bind(event).bind(metadata)
        .execute(&mut **tx).await.map_err(ApiError::sql)?;
    Ok(())
}

pub(crate) async fn capabilities(State(state): State<AppState>) -> Json<OnboardingCapabilities> {
    let cfg = &state.config.auth.authentik_management;
    Json(OnboardingCapabilities {
        schema_version: 1,
        idp_management_enabled: cfg.enabled,
        invitation_enabled: cfg.enabled && cfg.invitation_flow.is_some(),
        idp_provider: cfg.enabled.then(|| "authentik".into()),
    })
}

async fn user(state: &AppState, id: Uuid) -> Result<OnboardingUser, ApiError> {
    let pool = state.pool()?;
    let row = sqlx::query("SELECT * FROM auth_principals WHERE id=$1 AND kind='human_oidc'")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::not_found("human identity not found"))?;
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("disabled_at").map_err(ApiError::sql)?;
    let mut principal = super::store::principal_from_row(&pool, state, &row)
        .await?
        .to_response();
    if disabled_at.is_some() {
        principal.global_permissions.clear();
        for grant in &mut principal.projects {
            grant.permissions.clear();
        }
    }
    Ok(OnboardingUser {
        principal,
        disabled_at,
        created_at: row.try_get("created_at").map_err(ApiError::sql)?,
    })
}

pub(crate) async fn users(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM auth_principals WHERE kind='human_oidc' ORDER BY display_name,id LIMIT 500",
    )
    .fetch_all(&state.pool()?)
    .await
    .map_err(ApiError::sql)?;
    let mut users = Vec::with_capacity(ids.len());
    for id in ids {
        users.push(user(&state, id).await?);
    }
    Ok(Json(json!({"users":users})))
}

pub(crate) async fn register_user(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OnboardingUserRequest>,
) -> Result<Json<OnboardingUser>, ApiError> {
    let issuer = mem_config::oidc_principal_namespace(&state.config.auth.oidc.issuer_url);
    if issuer.is_empty()
        || mem_config::oidc_principal_namespace(&request.issuer) != issuer
        || !safe_https(issuer)
        || !text(&request.subject, 512)
        || !text(&request.display_name, 256)
        || request.email.as_ref().is_some_and(|v| !text(v, 320))
    {
        return Err(bad(
            "verified configured HTTPS issuer, stable subject and bounded identity fields are required",
        ));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_global_administrator(&state, &mut tx, &actor).await?;
    let id = Uuid::new_v4();
    // Existing identities (including disabled ones) are never reassigned by email.
    let created: Option<Uuid> = sqlx::query_scalar("INSERT INTO auth_principals(id,kind,issuer,subject,display_name,email) VALUES($1,'human_oidc',$2,$3,$4,$5) ON CONFLICT (issuer,subject) WHERE issuer IS NOT NULL AND subject IS NOT NULL DO NOTHING RETURNING id")
        .bind(id).bind(issuer).bind(&request.subject).bind(&request.display_name).bind(&request.email)
        .fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    let Some(id) = created else {
        return Err(ApiError::status_message(
            StatusCode::CONFLICT,
            "stable identity already registered; inspect its access and disabled status",
        ));
    };
    audit(
        &mut tx,
        actor.id,
        Some(id),
        "onboarding.user.register",
        json!({}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(user(&state, id).await?))
}

pub(crate) async fn disable_user(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<OnboardingUser>, ApiError> {
    if id == actor.id {
        return Err(bad("self-disable is not allowed"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_global_administrator(&state, &mut tx, &actor).await?;
    let target: Option<Uuid> = sqlx::query_scalar("UPDATE auth_principals SET disabled_at=COALESCE(disabled_at,now()),updated_at=now() WHERE id=$1 AND kind='human_oidc' RETURNING id")
        .bind(id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    if target.is_none() {
        return Err(ApiError::not_found("human identity not found"));
    }
    for query in [
        "UPDATE auth_web_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE principal_id=$1",
        "UPDATE auth_service_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE principal_id=$1",
        "UPDATE onboarding_devices SET revoked_at=COALESCE(revoked_at,now()) WHERE principal_id=$1",
    ] {
        sqlx::query(query)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::sql)?;
    }
    // Device credentials can belong to a machine principal; explicit links fence them too.
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE id IN (SELECT service_token_id FROM onboarding_devices WHERE principal_id=$1)")
        .bind(id).execute(&mut *tx).await.map_err(ApiError::sql)?;
    sqlx::query("UPDATE auth_web_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE id IN (SELECT web_session_id FROM onboarding_devices WHERE principal_id=$1)")
        .bind(id).execute(&mut *tx).await.map_err(ApiError::sql)?;
    audit(
        &mut tx,
        actor.id,
        Some(id),
        "onboarding.user.disable",
        json!({}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(user(&state, id).await?))
}

pub(crate) async fn access(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OnboardingEffectiveAccess>, ApiError> {
    let user = user(&state, id).await?;
    let disabled = user.disabled_at.is_some();
    let pool = state.pool()?;
    let explicit = super::list_memberships(&pool).await?;
    let projects: Vec<String> =
        sqlx::query_scalar("SELECT slug FROM projects ORDER BY slug LIMIT 1000")
            .fetch_all(&pool)
            .await
            .map_err(ApiError::sql)?;
    let principal = user.principal;
    let group_rules: Vec<_> = state
        .config
        .auth
        .group_mappings
        .rules
        .iter()
        .map(|r| OnboardingGroupRule {
            group: r.group.clone(),
            role: r.role,
            global: r.global,
            project: r.project.clone(),
            matched: principal.groups.contains(&r.group),
        })
        .collect();
    let mut entries = Vec::new();
    for project in projects {
        let mut grants: Vec<_> = explicit
            .iter()
            .filter(|m| m.principal_id == id && m.project == project)
            .map(|m| OnboardingAccessGrant {
                source: m.source.clone(),
                role: m.role,
                group: None,
            })
            .collect();
        for rule in &group_rules {
            if rule.matched && (rule.global || rule.project.as_deref() == Some(&project)) {
                grants.push(OnboardingAccessGrant {
                    source: "oidc_group".into(),
                    role: rule.role,
                    group: Some(rule.group.clone()),
                });
            }
        }
        let mut names = principal.global_permissions.clone();
        if let Some(entry) = principal.projects.iter().find(|p| p.project == project) {
            names.extend(entry.permissions.clone());
        }
        names.sort();
        names.dedup();
        if disabled {
            names.clear();
        }
        if !names.is_empty() || !grants.is_empty() {
            entries.push(OnboardingProjectAccess {
                project,
                permissions: names,
                grants,
            });
        }
    }
    Ok(Json(OnboardingEffectiveAccess {
        principal_id: id,
        disabled,
        global_permissions: principal.global_permissions,
        projects: entries,
        group_rules,
    }))
}

pub(super) fn validate_entry(entry: &OnboardingProjectEntry) -> Result<(), ApiError> {
    if entry.owner_instance_id.is_nil()
        || (entry.area == OnboardingArea::Trust && entry.offline_allowed)
        || !safe_https(&entry.repository_url)
        || !text(&entry.default_branch, 256)
        || entry.default_branch.starts_with('-')
        || entry.default_branch.contains(' ')
        || entry.tools.len() > 32
        || entry.destinations.len() > 32
    {
        return Err(bad(
            "catalog requires a non-nil owner, credential-free HTTPS repository, pinned branch, and Trust remote-only policy",
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for tool in &entry.tools {
        if !text(&tool.name, 128)
            || !text(&tool.version, 128)
            || matches!(tool.version.as_str(), "latest" | "main" | "master")
            || !safe_https(&tool.url)
            || tool.sha256.len() != 64
            || !tool.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || !names.insert(&tool.name)
        {
            return Err(bad(
                "tools require unique names, pinned versions, credential-free HTTPS URLs and SHA-256 hashes",
            ));
        }
    }
    names.clear();
    for destination in &entry.destinations {
        if !text(&destination.name, 128)
            || !text(&destination.provider_id, 128)
            || !text(&destination.destination_id, 256)
            || !destination
                .provider_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || !destination
                .destination_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            || !names.insert(&destination.name)
        {
            return Err(bad(
                "destinations require unique names and reviewed provider/destination identifiers",
            ));
        }
    }
    Ok(())
}
pub(super) fn project_row(row: &PgRow) -> Result<OnboardingProject, ApiError> {
    Ok(OnboardingProject {
        project_id: row.try_get("project_id").map_err(ApiError::sql)?,
        project: row.try_get("slug").map_err(ApiError::sql)?,
        entry: serde_json::from_value(row.try_get("entry_json").map_err(ApiError::sql)?)
            .map_err(|_| ApiError::internal("invalid reviewed catalog entry"))?,
        revision: row.try_get("revision").map_err(ApiError::sql)?,
        reviewed_by: row.try_get("reviewed_by").map_err(ApiError::sql)?,
        reviewed_at: row.try_get("reviewed_at").map_err(ApiError::sql)?,
    })
}
pub(crate) async fn projects(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows=sqlx::query("SELECT DISTINCT ON(v.project_id) v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id ORDER BY v.project_id,v.revision DESC LIMIT 1000")
        .fetch_all(&state.pool()?).await.map_err(ApiError::sql)?;
    let projects: Vec<_> = rows.iter().map(project_row).collect::<Result<_, _>>()?;
    Ok(Json(json!({"schema_version":1,"projects":projects})))
}
pub(crate) async fn review_project(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(request): Json<OnboardingProjectReviewRequest>,
) -> Result<Json<OnboardingProject>, ApiError> {
    validate_entry(&request.entry)?;
    if request.expected_revision < 0 {
        return Err(bad("expected_revision must be nonnegative"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_global_administrator(&state, &mut tx, &actor).await?;
    let slug: Option<String> =
        sqlx::query_scalar("SELECT slug FROM projects WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::sql)?;
    let slug = slug.ok_or_else(|| ApiError::not_found("local project UUID not found"))?;
    let revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM onboarding_project_versions WHERE project_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if revision != request.expected_revision {
        return Err(ApiError::status_message(
            StatusCode::CONFLICT,
            "catalog revision changed; refresh and review again",
        ));
    }
    let next = revision
        .checked_add(1)
        .ok_or_else(|| bad("catalog revision exhausted"))?;
    let row=sqlx::query("INSERT INTO onboarding_project_versions(project_id,revision,area,owner_instance_id,offline_allowed,entry_json,reviewed_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *, $8::text AS slug")
        .bind(id).bind(next).bind(request.entry.area.as_str()).bind(request.entry.owner_instance_id).bind(request.entry.offline_allowed)
        .bind(json!(request.entry)).bind(actor.id).bind(slug).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    audit(
        &mut tx,
        actor.id,
        None,
        "onboarding.project.review",
        json!({"project_id":id,"revision":next}),
    )
    .await?;
    let result = project_row(&row)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(result))
}
fn device_row(row: &PgRow) -> Result<OnboardingDevice, ApiError> {
    Ok(OnboardingDevice {
        id: row.try_get("id").map_err(ApiError::sql)?,
        principal_id: row.try_get("principal_id").map_err(ApiError::sql)?,
        name: row.try_get("name").map_err(ApiError::sql)?,
        platform: row.try_get("platform").map_err(ApiError::sql)?,
        created_at: row.try_get("created_at").map_err(ApiError::sql)?,
        last_seen_at: row.try_get("last_seen_at").map_err(ApiError::sql)?,
        revoked_at: row.try_get("revoked_at").map_err(ApiError::sql)?,
    })
}
pub(crate) async fn devices(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows =
        sqlx::query("SELECT * FROM onboarding_devices ORDER BY created_at DESC,id LIMIT 500")
            .fetch_all(&state.pool()?)
            .await
            .map_err(ApiError::sql)?;
    let devices: Vec<_> = rows.iter().map(device_row).collect::<Result<_, _>>()?;
    Ok(Json(json!({"devices":devices})))
}
pub(crate) async fn revoke_device(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<OnboardingDevice>, ApiError> {
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_global_administrator(&state, &mut tx, &actor).await?;
    let row=sqlx::query("UPDATE onboarding_devices SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1 RETURNING *")
        .bind(id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("device not found"))?;
    let token: Option<Uuid> = row.try_get("service_token_id").map_err(ApiError::sql)?;
    let session: Option<Uuid> = row.try_get("web_session_id").map_err(ApiError::sql)?;
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1")
        .bind(token)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    sqlx::query("UPDATE auth_web_sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1")
        .bind(session)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    let device = device_row(&row)?;
    audit(
        &mut tx,
        actor.id,
        Some(device.principal_id),
        "onboarding.device.revoke",
        json!({"device_id":id}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(device))
}
pub(crate) async fn invite(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OnboardingIdpUserRequest>,
) -> Result<Json<OnboardingIdpOperation>, ApiError> {
    Ok(Json(
        provider::operate(&state, &actor, "invite", Some(request), None).await?,
    ))
}
pub(crate) async fn create_idp_user(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(request): Json<OnboardingIdpUserRequest>,
) -> Result<Json<OnboardingIdpOperation>, ApiError> {
    Ok(Json(
        provider::operate(&state, &actor, "create", Some(request), None).await?,
    ))
}
pub(crate) async fn disable_idp_user(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(id): Path<i64>,
) -> Result<Json<OnboardingIdpOperation>, ApiError> {
    if id <= 0 {
        return Err(bad("provider user ID must be positive"));
    }
    Ok(Json(
        provider::operate(&state, &actor, "disable", None, Some(id)).await?,
    ))
}

fn operation_row(row: &PgRow) -> Result<OnboardingIdpOperation, ApiError> {
    Ok(OnboardingIdpOperation {
        id: row.try_get("id").map_err(ApiError::sql)?,
        operation: row.try_get("operation").map_err(ApiError::sql)?,
        outcome: row.try_get("outcome").map_err(ApiError::sql)?,
        provider_origin: row.try_get("provider_origin").map_err(ApiError::sql)?,
        provider_correlation: row.try_get("provider_correlation").map_err(ApiError::sql)?,
        request_sha256: row.try_get("request_sha256").map_err(ApiError::sql)?,
        provider_user_id: row.try_get("provider_user_id").map_err(ApiError::sql)?,
        invitation_id: row.try_get("invitation_id").map_err(ApiError::sql)?,
    })
}
pub(crate) async fn operations(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows=sqlx::query("SELECT i.id,i.operation,i.provider_origin,i.provider_correlation,i.request_sha256,COALESCE(o.outcome,'pending') AS outcome,COALESCE(o.provider_user_id,i.provider_user_id) AS provider_user_id,o.invitation_id FROM onboarding_idp_intents i LEFT JOIN onboarding_idp_outcomes o ON o.intent_id=i.id ORDER BY i.recorded_at DESC,i.id LIMIT 100")
        .fetch_all(&state.pool()?).await.map_err(ApiError::sql)?;
    let operations: Vec<_> = rows.iter().map(operation_row).collect::<Result<_, _>>()?;
    Ok(Json(json!({"operations":operations})))
}
pub(crate) async fn operation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OnboardingIdpOperation>, ApiError> {
    let row=sqlx::query("SELECT i.id,i.operation,i.provider_origin,i.provider_correlation,i.request_sha256,COALESCE(o.outcome,'pending') AS outcome,COALESCE(o.provider_user_id,i.provider_user_id) AS provider_user_id,o.invitation_id FROM onboarding_idp_intents i LEFT JOIN onboarding_idp_outcomes o ON o.intent_id=i.id WHERE i.id=$1")
        .bind(id).fetch_optional(&state.pool()?).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("provider operation not found"))?;
    Ok(Json(operation_row(&row)?))
}
