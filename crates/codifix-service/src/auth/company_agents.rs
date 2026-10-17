// SPDX-License-Identifier: AGPL-3.0-or-later
//! Independent, zero-permission child credentials; no MCP/content grant.
use super::{AuthenticatedPrincipal, CredentialSource, company_catalog, company_devices, device};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use codifix_record::*;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow, types::Json as SqlJson};
use std::collections::BTreeSet;
use uuid::Uuid;

#[cfg(test)]
mod tests;
fn denied() -> ApiError {
    ApiError::forbidden(
        "exact current agent, parent human, device, scope and reviewed destination required",
    )
}
fn changed() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "agent identity, immutable request, reviewed scope or parent policy changed; no credential restored",
    )
}
fn hash(value: &str) -> Result<Vec<u8>, ApiError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(denied());
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| denied()))
        .collect()
}
fn validate(
    request: &CompanyAgentApproveRequest,
    device: Uuid,
    human: Uuid,
) -> Result<(), ApiError> {
    let ids = [
        request.request_id,
        request.agent_id,
        request.principal_id,
        request.token_id,
    ];
    if !request.confirmed
        || ids
            .iter()
            .any(|id| id.is_nil() || *id == device || *id == human)
        || ids.into_iter().collect::<BTreeSet<_>>().len() != 4
        || request.provider_id != request.caller.provider_id()
        || request.projects.is_empty()
        || request.projects.len() > 32
    {
        return Err(denied());
    }
    hash(&request.credential_sha256)?;
    let mut selected = BTreeSet::new();
    for s in &request.projects {
        if !s.offline_confirmed
            || s.area == OnboardingArea::Trust
            || s.owner_instance_id.is_nil()
            || s.project_id.is_nil()
            || s.manifest_revision <= 0
            || hash(&s.manifest_sha256).is_err()
            || !selected.insert((s.area.as_str(), s.owner_instance_id, s.project_id))
        {
            return Err(denied());
        }
    }
    Ok(())
}
async fn begin(state: &AppState) -> Result<Transaction<'_, Postgres>, ApiError> {
    let mut tx = state.pool()?.begin().await.map_err(ApiError::sql)?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    Ok(tx)
}
async fn browser_parent(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    device_id: Uuid,
) -> Result<(), ApiError> {
    device::lock_human(state, tx, actor).await?;
    let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_devices d JOIN onboarding_company_device_intents i ON i.device_id=d.id WHERE d.id=$1 AND d.principal_id=$2 AND i.human_id=$2 FOR SHARE OF d,i)").bind(device_id).bind(actor.id).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !owned {
        return Err(denied());
    }
    Ok(())
}
fn receipt(row: &PgRow) -> Result<CompanyAgentReceipt, ApiError> {
    let mut receipt = row
        .try_get::<SqlJson<CompanyAgentReceipt>, _>("receipt_json")
        .map_err(ApiError::sql)?
        .0;
    receipt.revoked_at = row.try_get("revoked_at").map_err(ApiError::sql)?;
    Ok(receipt)
}
async fn find(tx: &mut Transaction<'_, Postgres>, agent: Uuid) -> Result<PgRow, ApiError> {
    sqlx::query("SELECT a.*,r.revoked_at FROM onboarding_company_agents a LEFT JOIN onboarding_company_agent_revocations r USING(agent_id) WHERE a.agent_id=$1").bind(agent).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)
}
fn policy(
    request: &CompanyAgentApproveRequest,
    bindings: &[company_devices::CompanyDeviceBinding],
) -> Result<Vec<CompanyAgentScope>, ApiError> {
    if bindings.len() != request.projects.len() {
        return Err(denied());
    }
    request
        .projects
        .iter()
        .zip(bindings)
        .map(|(selection, binding)| {
            let manifest = &binding.manifest;
            if manifest.entry.area != selection.area
                || manifest.entry.owner_instance_id != selection.owner_instance_id
                || manifest.project_id != selection.project_id
                || manifest.revision != selection.manifest_revision
                || company_catalog::digest(manifest)? != selection.manifest_sha256
                || !manifest.entry.offline_allowed
                || manifest.entry.area == OnboardingArea::Trust
            {
                return Err(changed());
            }
            let destination = manifest
                .entry
                .destinations
                .iter()
                .find(|d| {
                    d.name == request.caller.as_str()
                        && d.provider_id == request.caller.provider_id()
                        && d.destination_id == request.caller.provider_id()
                })
                .cloned()
                .ok_or_else(denied)?;
            Ok(CompanyAgentScope {
                selection: selection.clone(),
                destination,
            })
        })
        .collect()
}
async fn parents(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    device_id: Uuid,
    human_id: Uuid,
    request: &CompanyAgentApproveRequest,
) -> Result<Vec<company_devices::CompanyDeviceBinding>, ApiError> {
    let mut bindings = Vec::new();
    // One parent request/device/token lock order is reused for every immutable scope.
    for s in &request.projects {
        bindings.push(
            company_devices::parent_binding(
                state,
                tx,
                device_id,
                human_id,
                s.area.as_str(),
                s.owner_instance_id,
                s.project_id,
            )
            .await?,
        );
    }
    policy(request, &bindings)?;
    Ok(bindings)
}
pub(super) struct CompanyAgentBinding {
    pub receipt: CompanyAgentReceipt,
    pub parents: Vec<company_devices::CompanyDeviceBinding>,
}
async fn locked_relation(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    row: &PgRow,
) -> Result<CompanyAgentBinding, ApiError> {
    let saved = receipt(row)?;
    let request = row
        .try_get::<SqlJson<CompanyAgentApproveRequest>, _>("request_json")
        .map_err(ApiError::sql)?
        .0;
    validate(&request, saved.device_id, saved.human_id)?;
    if company_catalog::digest(&request)?
        != row
            .try_get::<String, _>("request_sha256")
            .map_err(ApiError::sql)?
        || row.try_get::<Uuid, _>("device_id").map_err(ApiError::sql)? != saved.device_id
        || row.try_get::<Uuid, _>("human_id").map_err(ApiError::sql)? != saved.human_id
        || row
            .try_get::<Uuid, _>("principal_id")
            .map_err(ApiError::sql)?
            != saved.principal_id
        || row.try_get::<Uuid, _>("token_id").map_err(ApiError::sql)? != saved.token_id
        || saved.request_id != request.request_id
        || saved.agent_id != request.agent_id
        || saved.principal_id != request.principal_id
        || saved.token_id != request.token_id
        || saved.caller != request.caller
        || saved.replication_mode != request.replication_mode
        || saved.provider_id != request.provider_id
        || saved.credential_sha256 != request.credential_sha256
        || saved.expires_at != request.expires_at
        || saved.schema_version != 1
        || saved.revoked_at.is_some()
    {
        return Err(denied());
    }
    let bindings = parents(state, tx, saved.device_id, saved.human_id, &request).await?;
    let scopes = policy(&request, &bindings)?;
    if company_catalog::digest(&saved.projects)? != company_catalog::digest(&scopes)? {
        return Err(changed());
    }
    for binding in &bindings {
        if binding.company_id != saved.company_id
            || binding.issuer != saved.issuer
            || binding.subject != saved.subject
            || binding.registry_sha256
                != row
                    .try_get::<String, _>("registry_sha256")
                    .map_err(ApiError::sql)?
        {
            return Err(changed());
        }
    }
    // Parent locks are acquired BEFORE the independently revocable child locks.
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_company_agents a WHERE agent_id=$1 AND expires_at>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM onboarding_company_agent_revocations r WHERE r.agent_id=a.agent_id) FOR SHARE OF a)").bind(saved.agent_id).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !live {
        return Err(denied());
    }
    // SQLx encodes TIMESTAMPTZ as whole microseconds. Preserve the immutable
    // original JSON/request digest, but require BOTH database expiry columns
    // to equal that exact representation (no tolerance and no TTL extension).
    let database_expiry =
        DateTime::from_timestamp_micros(saved.expires_at.timestamp_micros()).ok_or_else(denied)?;
    if row
        .try_get::<DateTime<Utc>, _>("expires_at")
        .map_err(ApiError::sql)?
        != database_expiry
    {
        return Err(denied());
    }
    let token=sqlx::query("SELECT t.token_hash,t.expires_at,p.global_role,p.global_permissions_json,p.groups_json FROM auth_principals p JOIN auth_service_tokens t ON t.principal_id=p.id WHERE p.id=$1 AND p.kind='service_token' AND p.disabled_at IS NULL AND t.id=$2 AND t.revoked_at IS NULL AND t.expires_at>clock_timestamp() FOR SHARE OF p,t").bind(saved.principal_id).bind(saved.token_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(denied)?;
    if token
        .try_get::<Vec<u8>, _>("token_hash")
        .map_err(ApiError::sql)?
        != hash(&request.credential_sha256)?
        || token
            .try_get::<Vec<u8>, _>("token_hash")
            .map_err(ApiError::sql)?
            != row
                .try_get::<Vec<u8>, _>("credential_hash")
                .map_err(ApiError::sql)?
        || token
            .try_get::<Option<DateTime<Utc>>, _>("expires_at")
            .map_err(ApiError::sql)?
            != Some(database_expiry)
        || token
            .try_get::<Option<String>, _>("global_role")
            .map_err(ApiError::sql)?
            .is_some()
        || token
            .try_get::<Option<serde_json::Value>, _>("global_permissions_json")
            .map_err(ApiError::sql)?
            != Some(json!([]))
        || token
            .try_get::<serde_json::Value, _>("groups_json")
            .map_err(ApiError::sql)?
            != json!([])
    {
        return Err(denied());
    }
    let ordinary: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM auth_project_memberships WHERE principal_id=$1)",
    )
    .bind(saved.principal_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    if ordinary {
        return Err(denied());
    }
    let earliest = bindings
        .iter()
        .map(|b| b.approval.request.expires_at)
        .chain(std::iter::once(saved.expires_at))
        .min()
        .ok_or_else(denied)?;
    let current: bool = sqlx::query_scalar("SELECT $1::timestamptz>clock_timestamp()")
        .bind(earliest)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    if !current {
        return Err(denied());
    }
    Ok(CompanyAgentBinding {
        receipt: saved,
        parents: bindings,
    })
}
pub(super) async fn agent_binding(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthenticatedPrincipal,
    agent_id: Uuid,
) -> Result<CompanyAgentBinding, ApiError> {
    if actor.kind != AuthPrincipalKind::ServiceToken
        || actor.credential_source != CredentialSource::ServiceToken
    {
        return Err(denied());
    }
    let row = find(tx, agent_id).await?;
    if row
        .try_get::<Uuid, _>("principal_id")
        .map_err(ApiError::sql)?
        != actor.id
        || Some(row.try_get::<Uuid, _>("token_id").map_err(ApiError::sql)?) != actor.token_id
    {
        return Err(denied());
    }
    locked_relation(state, tx, &row).await
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    agent: Uuid,
    event: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,metadata_json) VALUES($1,$2,$3,'success',$4)").bind(Uuid::new_v4()).bind(actor).bind(event).bind(json!({"agent_id":agent})).execute(&mut **tx).await.map_err(ApiError::sql)?;
    Ok(())
}
async fn current(binding: &CompanyAgentBinding) -> Result<(), ApiError> {
    for parent in &binding.parents {
        company_devices::current_owner(parent).await?;
    }
    Ok(())
}
fn bounded(receipt: &CompanyAgentReceipt) -> Result<(), ApiError> {
    if serde_json::to_vec(receipt).map_err(|_| denied())?.len() > 32768 {
        return Err(denied());
    }
    Ok(())
}
pub(crate) async fn inspect(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(device_id): Path<Uuid>,
) -> Result<Json<CompanyAgentList>, ApiError> {
    let mut tx = begin(&state).await?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    let rows=sqlx::query("SELECT a.*,r.revoked_at FROM onboarding_company_agents a LEFT JOIN onboarding_company_agent_revocations r USING(agent_id) WHERE a.device_id=$1 AND a.human_id=$2 ORDER BY a.approved_at,a.agent_id LIMIT 65").bind(device_id).bind(actor.id).fetch_all(&mut *tx).await.map_err(ApiError::sql)?;
    if rows.len() > 64 {
        return Err(denied());
    }
    let agents = rows.iter().map(receipt).collect::<Result<Vec<_>, _>>()?;
    for receipt in &agents {
        bounded(receipt)?;
    }
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(CompanyAgentList {
        schema_version: 1,
        agents,
    }))
}
pub(crate) async fn child_receipt(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<CompanyAgentReceipt>, ApiError> {
    let mut tx = begin(&state).await?;
    let before = agent_binding(&state, &mut tx, &actor, agent_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    current(&before).await?;
    let mut tx = begin(&state).await?;
    let after = agent_binding(&state, &mut tx, &actor, agent_id).await?;
    if company_catalog::digest(&before.receipt)? != company_catalog::digest(&after.receipt)? {
        return Err(changed());
    }
    bounded(&after.receipt)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(after.receipt))
}
pub(super) async fn cap_authenticated_agent(
    state: &AppState,
    pool: &sqlx::PgPool,
    token: Uuid,
    principal: &mut AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    let agent: Option<Uuid> = sqlx::query_scalar(
        "SELECT agent_id FROM onboarding_company_agents WHERE token_id=$1 OR principal_id=$2",
    )
    .bind(token)
    .bind(principal.id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::sql)?;
    let Some(agent) = agent else { return Ok(()) };
    let mut tx = begin(state).await?;
    agent_binding(state, &mut tx, principal, agent)
        .await
        .map_err(|_| {
            ApiError::unauthorized("agent credential or current parent binding inactive")
        })?;
    principal.global = PermissionSet::EMPTY;
    principal.global_role = None;
    principal.project_roles.clear();
    principal.groups.clear();
    tx.commit().await.map_err(ApiError::sql)
}

async fn existing(
    tx: &mut Transaction<'_, Postgres>,
    request: &CompanyAgentApproveRequest,
) -> Result<Option<PgRow>, ApiError> {
    sqlx::query("SELECT a.*,r.revoked_at FROM onboarding_company_agents a LEFT JOIN onboarding_company_agent_revocations r USING(agent_id) WHERE a.request_id=$1 OR a.agent_id=$2 OR a.principal_id=$3 OR a.token_id=$4").bind(request.request_id).bind(request.agent_id).bind(request.principal_id).bind(request.token_id).fetch_optional(&mut **tx).await.map_err(ApiError::sql)
}
fn exact(
    row: &PgRow,
    request: &CompanyAgentApproveRequest,
    device_id: Uuid,
    human_id: Uuid,
) -> Result<(), ApiError> {
    if row.try_get::<Uuid, _>("device_id").map_err(ApiError::sql)? != device_id
        || row.try_get::<Uuid, _>("human_id").map_err(ApiError::sql)? != human_id
        || row
            .try_get::<String, _>("request_sha256")
            .map_err(ApiError::sql)?
            != company_catalog::digest(request)?
        || row
            .try_get::<SqlJson<CompanyAgentApproveRequest>, _>("request_json")
            .map_err(ApiError::sql)?
            .0
            != *request
    {
        return Err(changed());
    }
    Ok(())
}
async fn expiry(
    tx: &mut Transaction<'_, Postgres>,
    expires: DateTime<Utc>,
) -> Result<(), ApiError> {
    let valid:bool=sqlx::query_scalar("SELECT $1::timestamptz>clock_timestamp() AND $1::timestamptz<=clock_timestamp()+interval '24 hours'").bind(expires).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !valid {
        return Err(denied());
    }
    Ok(())
}
pub(crate) async fn approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path(device_id): Path<Uuid>,
    Json(request): Json<CompanyAgentApproveRequest>,
) -> Result<Json<CompanyAgentReceipt>, ApiError> {
    validate(&request, device_id, actor.id)?;
    let mut tx = begin(&state).await?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    let before = if let Some(row) = existing(&mut tx, &request).await? {
        exact(&row, &request, device_id, actor.id)?;
        Some(locked_relation(&state, &mut tx, &row).await?)
    } else {
        None
    };
    expiry(&mut tx, request.expires_at).await?;
    let before_parents = parents(&state, &mut tx, device_id, actor.id, &request).await?;
    // Parent/device bearer fingerprints cannot be reused as child credentials.
    let parent_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT r.credential_hash FROM onboarding_device_requests r WHERE r.device_id=$1",
    )
    .bind(device_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if hash(&request.credential_sha256)? == parent_hash {
        return Err(denied());
    }
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    for parent in &before_parents {
        company_devices::current_owner(parent).await?;
    }
    let mut tx = begin(&state).await?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    let after_parents = parents(&state, &mut tx, device_id, actor.id, &request).await?;
    for (before, after) in before_parents.iter().zip(&after_parents) {
        if before.approval != after.approval
            || before.bootstrap != after.bootstrap
            || before.registry_sha256 != after.registry_sha256
            || company_catalog::digest(&before.manifest)?
                != company_catalog::digest(&after.manifest)?
        {
            return Err(changed());
        }
    }
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:company-agent-provision',0))",
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    let result = if let Some(row) = existing(&mut tx, &request).await? {
        exact(&row, &request, device_id, actor.id)?;
        let current = locked_relation(&state, &mut tx, &row).await?;
        if before.as_ref().is_some_and(|old| {
            company_catalog::digest(&old.receipt).ok()
                != company_catalog::digest(&current.receipt).ok()
        }) {
            return Err(changed());
        }
        current.receipt
    } else {
        if before.is_some() {
            return Err(changed());
        }
        let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1) OR EXISTS(SELECT 1 FROM auth_service_tokens WHERE id=$2 OR token_hash=$3)").bind(request.principal_id).bind(request.token_id).bind(hash(&request.credential_sha256)?).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        if occupied {
            return Err(changed());
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM onboarding_company_agents WHERE device_id=$1")
                .bind(device_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(ApiError::sql)?;
        if count >= 64 {
            return Err(ApiError::status_message(
                StatusCode::TOO_MANY_REQUESTS,
                "at most 64 retained agent identities per enrolled device",
            ));
        }
        expiry(&mut tx, request.expires_at).await?;
        let scopes = policy(&request, &after_parents)?;
        let parent = after_parents.first().ok_or_else(denied)?;
        if request.expires_at > parent.approval.request.expires_at {
            return Err(denied());
        }
        let approved_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::sql)?;
        let receipt = CompanyAgentReceipt {
            replication_mode: request.replication_mode,
            schema_version: 1,
            request_id: request.request_id,
            agent_id: request.agent_id,
            device_id,
            human_id: actor.id,
            issuer: parent.issuer.clone(),
            subject: parent.subject.clone(),
            company_id: parent.company_id,
            principal_id: request.principal_id,
            token_id: request.token_id,
            caller: request.caller,
            provider_id: request.provider_id.clone(),
            credential_sha256: request.credential_sha256.clone(),
            expires_at: request.expires_at,
            projects: scopes,
            approved_at,
            revoked_at: None,
        };
        bounded(&receipt)?;
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,'service_token',$2,'[]'::jsonb)").bind(request.principal_id).bind(format!("{} company agent",request.caller.as_str())).execute(&mut *tx).await.map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,created_by,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(request.token_id).bind(request.principal_id).bind(format!("{} independent agent",request.caller.as_str())).bind(hash(&request.credential_sha256)?).bind(format!("agent_{}",&request.agent_id.simple().to_string()[..8])).bind(actor.id).bind(request.expires_at).execute(&mut *tx).await.map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO onboarding_company_agents(agent_id,request_id,device_id,human_id,principal_id,token_id,company_id,request_sha256,request_json,receipt_json,registry_sha256,credential_hash,expires_at,approved_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(request.agent_id).bind(request.request_id).bind(device_id).bind(actor.id).bind(request.principal_id).bind(request.token_id).bind(parent.company_id).bind(company_catalog::digest(&request)?).bind(SqlJson(&request)).bind(SqlJson(&receipt)).bind(&parent.registry_sha256).bind(hash(&request.credential_sha256)?).bind(request.expires_at).bind(approved_at).execute(&mut *tx).await.map_err(ApiError::sql)?;
        audit(
            &mut tx,
            actor.id,
            request.agent_id,
            "onboarding.company_agent.approve",
        )
        .await?;
        receipt
    };
    expiry(&mut tx, result.expires_at).await?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    bounded(&result)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(result))
}
pub(crate) async fn revoke(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Path((device_id, agent_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CompanyAgentReceipt>, ApiError> {
    let mut tx = begin(&state).await?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    let row = find(&mut tx, agent_id).await?;
    let saved = receipt(&row)?;
    if saved.device_id != device_id || saved.human_id != actor.id {
        return Err(denied());
    }
    sqlx::query("SELECT agent_id FROM onboarding_company_agents WHERE agent_id=$1 FOR UPDATE")
        .bind(agent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    // Tombstone + token fence + protected audit share one transaction. Replays
    // acknowledge the original revocation without restoring or rotating IDs.
    if saved.revoked_at.is_none() {
        sqlx::query("INSERT INTO onboarding_company_agent_revocations(agent_id,revoked_by) VALUES($1,$2) ON CONFLICT(agent_id) DO NOTHING").bind(agent_id).bind(actor.id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        sqlx::query("UPDATE auth_service_tokens SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE id=$1 AND principal_id=$2").bind(saved.token_id).bind(saved.principal_id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        audit(
            &mut tx,
            actor.id,
            agent_id,
            "onboarding.company_agent.revoke",
        )
        .await?;
    }
    let result = receipt(&find(&mut tx, agent_id).await?)?;
    bounded(&result)?;
    browser_parent(&state, &mut tx, &actor, device_id).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(result))
}
