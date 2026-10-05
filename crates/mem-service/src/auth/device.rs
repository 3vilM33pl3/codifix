// SPDX-License-Identifier: AGPL-3.0-or-later
//! Human browser approval of a client-generated, bounded machine credential.
use super::{AuthenticatedPrincipal, CredentialSource};
use crate::{ApiError, AppState};
use axum::{
    Extension, Json,
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
};
use mem_record::*;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use uuid::Uuid;
#[cfg(test)]
pub(crate) mod tests;

fn bad(message: &str) -> ApiError {
    ApiError::status_message(StatusCode::BAD_REQUEST, message)
}
fn conflict() -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "device request, decision or reviewed project revision changed",
    )
}
pub(super) fn configuration(state: &AppState) -> Result<&mem_config::AreaIdentityConfig, ApiError> {
    let url = state
        .config
        .auth
        .public_base_url
        .as_deref()
        .and_then(|v| reqwest::Url::parse(v).ok());
    let valid = url.is_some_and(|u| {
        (u.scheme() == "https"
            || (cfg!(test)
                && u.scheme() == "http"
                && u.host_str()
                    .is_some_and(|h| h == "localhost" || h == "127.0.0.1")))
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
    });
    if state.config.auth.mode != AuthMode::MultiUser
        || !valid
        || state.config.service.read_only
        || state.config.evaluation.active()
        || !state.is_primary()
    {
        return Err(ApiError::forbidden(
            "device onboarding requires a writable multi-user company instance with a configured HTTPS browser origin",
        ));
    }
    let area = state.config.area.as_ref().ok_or_else(|| {
        ApiError::forbidden(
            "device onboarding requires explicit local area and owner instance identity",
        )
    })?;
    area.validate()
        .map_err(|_| ApiError::forbidden("invalid local area identity"))?;
    if !matches!(
        area.name.as_str(),
        "workshop" | "homelab" | "trust" | "general"
    ) {
        return Err(ApiError::forbidden("unsupported local area identity"));
    }
    Ok(area)
}
fn digest(value: &str) -> Result<Vec<u8>, ApiError> {
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad(
            "SHA-256 fingerprints must contain exactly 64 hexadecimal characters",
        ));
    }
    (0..64)
        .step_by(2)
        .map(|n| u8::from_str_radix(&value[n..n + 2], 16).map_err(|_| bad("invalid fingerprint")))
        .collect()
}
fn fingerprint(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn limit_start(peer: IpAddr) -> Result<(), ApiError> {
    static RATES: OnceLock<Mutex<BTreeMap<IpAddr, VecDeque<Instant>>>> = OnceLock::new();
    let mut rates = RATES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| ApiError::service_unavailable("device enrollment rate limiter unavailable"))?;
    let now = Instant::now();
    for events in rates.values_mut() {
        while events
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            events.pop_front();
        }
    }
    rates.retain(|_, events| !events.is_empty());
    if rates.len() >= 10_000 && !rates.contains_key(&peer) {
        return Err(ApiError::status_message(
            StatusCode::TOO_MANY_REQUESTS,
            "device enrollment rate limit reached",
        ));
    }
    let events = rates.entry(peer).or_default();
    if events.len() >= 10 {
        return Err(ApiError::status_message(
            StatusCode::TOO_MANY_REQUESTS,
            "at most ten device starts per peer per minute",
        ));
    }
    events.push_back(now);
    Ok(())
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: Option<Uuid>,
    event: &str,
    metadata: Value,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO auth_audit_events(id,actor_principal_id,event_type,outcome,metadata_json) VALUES($1,$2,$3,'success',$4)")
        .bind(Uuid::new_v4()).bind(actor).bind(event).bind(metadata).execute(&mut **tx).await.map_err(ApiError::sql)?;
    Ok(())
}
pub(crate) async fn start(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<OnboardingDeviceStartRequest>,
) -> Result<Json<OnboardingDeviceStartResponse>, ApiError> {
    configuration(&state)?;
    limit_start(peer.ip())?;
    if request.request_id.is_nil()
        || request.device_id.is_nil()
        || request.name.trim().is_empty()
        || request.name.len() > 128
        || request.name.chars().any(char::is_control)
        || !matches!(request.platform.as_str(), "macos" | "linux" | "windows")
    {
        return Err(bad("invalid bounded device identity"));
    }
    let credential = digest(&request.credential_sha256)?;
    let poll = digest(&request.poll_sha256)?;
    if credential == poll {
        return Err(bad("credential and polling secret must be distinct"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('memory:device-enrollment-start',0))",
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    let existing =
        sqlx::query("SELECT * FROM onboarding_device_requests WHERE request_id=$1 FOR UPDATE")
            .bind(request.request_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::sql)?;
    let row = if let Some(row) = existing {
        if row.try_get::<Uuid, _>("device_id").map_err(ApiError::sql)? != request.device_id
            || row.try_get::<String, _>("name").map_err(ApiError::sql)? != request.name
            || row
                .try_get::<String, _>("platform")
                .map_err(ApiError::sql)?
                != request.platform
            || row
                .try_get::<Vec<u8>, _>("credential_hash")
                .map_err(ApiError::sql)?
                != credential
            || row
                .try_get::<Vec<u8>, _>("poll_hash")
                .map_err(ApiError::sql)?
                != poll
        {
            return Err(conflict());
        }
        row
    } else {
        let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_devices WHERE id=$1) OR EXISTS(SELECT 1 FROM onboarding_device_requests WHERE device_id=$1)")
            .bind(request.device_id).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        if occupied {
            return Err(conflict());
        }
        let outstanding:i64=sqlx::query_scalar("SELECT count(*) FROM onboarding_device_requests WHERE status='pending' AND expires_at>clock_timestamp()")
            .fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        if outstanding >= 1000 {
            return Err(ApiError::status_message(
                StatusCode::TOO_MANY_REQUESTS,
                "outstanding device request capacity reached",
            ));
        }
        let code = {
            use rand::Rng;
            const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            let mut rng = rand::rng();
            let chars: String = (0..8)
                .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
                .collect();
            format!("{}-{}", &chars[..4], &chars[4..8])
        };
        let row=sqlx::query("INSERT INTO onboarding_device_requests(request_id,device_id,name,platform,credential_hash,poll_hash,user_code) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *")
            .bind(request.request_id).bind(request.device_id).bind(&request.name).bind(&request.platform).bind(credential).bind(poll).bind(code).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
        audit(
            &mut tx,
            None,
            "onboarding.device.start",
            json!({"request_id":request.request_id,"device_id":request.device_id}),
        )
        .await?;
        row
    };
    let user_code: String = row.try_get("user_code").map_err(ApiError::sql)?;
    let mut uri = reqwest::Url::parse(
        state
            .config
            .auth
            .public_base_url
            .as_deref()
            .unwrap_or_default(),
    )
    .map_err(|_| ApiError::forbidden("invalid company browser origin"))?;
    uri.set_path("/");
    uri.set_query(Some(&format!("device_code={user_code}")));
    if state
        .config
        .auth
        .company_onboarding
        .as_ref()
        .is_some_and(|c| {
            c.owners.iter().any(|o| {
                o.area != OnboardingArea::Trust
                    && o.delivery_token_env.is_some()
                    && o.delivery_policy_revision.is_some()
            })
        })
    {
        uri.query_pairs_mut().append_pair("company_device", "1");
    }
    let response = OnboardingDeviceStartResponse {
        request_id: request.request_id,
        user_code,
        expires_at: row.try_get("expires_at").map_err(ApiError::sql)?,
        poll_interval_seconds: 2,
        verification_uri_complete: uri.to_string(),
    };
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
}
fn empty_status(status: &str) -> OnboardingDevicePollResponse {
    OnboardingDevicePollResponse {
        status: status.into(),
        device_id: None,
        principal_id: None,
        projects: None,
    }
}
pub(super) async fn status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    row: &PgRow,
) -> Result<OnboardingDevicePollResponse, ApiError> {
    let state: String = row.try_get("status").map_err(ApiError::sql)?;
    let expires: chrono::DateTime<chrono::Utc> =
        row.try_get("expires_at").map_err(ApiError::sql)?;
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    if expires <= now {
        return Ok(empty_status("expired"));
    }
    if state != "approved" {
        return Ok(empty_status(&state));
    }
    let device: Uuid = row.try_get("device_id").map_err(ApiError::sql)?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_devices d JOIN auth_principals h ON h.id=d.principal_id JOIN auth_service_tokens t ON t.id=d.service_token_id WHERE d.id=$1 AND d.revoked_at IS NULL AND h.disabled_at IS NULL AND t.revoked_at IS NULL AND t.expires_at>clock_timestamp())")
        .bind(device).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !active {
        return Ok(empty_status("denied"));
    }
    let grants: Value = row.try_get("grants_json").map_err(ApiError::sql)?;
    Ok(OnboardingDevicePollResponse {
        status: "approved".into(),
        device_id: Some(device),
        principal_id: row.try_get("machine_principal_id").map_err(ApiError::sql)?,
        projects: Some(
            serde_json::from_value(grants)
                .map_err(|_| ApiError::internal("invalid device decision metadata"))?,
        ),
    })
}
pub(crate) async fn poll(
    State(state): State<AppState>,
    Json(request): Json<OnboardingDevicePollRequest>,
) -> Result<Json<OnboardingDevicePollResponse>, ApiError> {
    configuration(&state)?;
    if request.poll_secret.len() < 32
        || request.poll_secret.len() > 256
        || request.poll_secret.chars().any(char::is_control)
    {
        return Err(ApiError::unauthorized("invalid device poll proof"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let row=sqlx::query("SELECT *,last_poll_at>clock_timestamp()-interval '2 seconds' AS too_soon FROM onboarding_device_requests WHERE request_id=$1 FOR UPDATE")
        .bind(request.request_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::unauthorized("invalid device poll proof"))?;
    if row
        .try_get::<Vec<u8>, _>("poll_hash")
        .map_err(ApiError::sql)?
        != super::hash_secret(&request.poll_secret)
    {
        return Err(ApiError::unauthorized("invalid device poll proof"));
    }
    if row
        .try_get::<Option<bool>, _>("too_soon")
        .map_err(ApiError::sql)?
        == Some(true)
    {
        return Err(ApiError::status_message(
            StatusCode::TOO_MANY_REQUESTS,
            "wait two seconds before polling again",
        ));
    }
    sqlx::query(
        "UPDATE onboarding_device_requests SET last_poll_at=clock_timestamp() WHERE request_id=$1",
    )
    .bind(request.request_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    let result = status(&mut tx, &row).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(result))
}

pub(crate) async fn cancel(
    State(state): State<AppState>,
    Json(request): Json<OnboardingDevicePollRequest>,
) -> Result<Json<Value>, ApiError> {
    configuration(&state)?;
    if request.poll_secret.len() < 32
        || request.poll_secret.len() > 256
        || request.poll_secret.chars().any(char::is_control)
    {
        return Err(ApiError::unauthorized("invalid device poll proof"));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let row =
        sqlx::query("SELECT * FROM onboarding_device_requests WHERE request_id=$1 FOR UPDATE")
            .bind(request.request_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::sql)?
            .ok_or_else(|| ApiError::unauthorized("invalid device poll proof"))?;
    if row
        .try_get::<Vec<u8>, _>("poll_hash")
        .map_err(ApiError::sql)?
        != super::hash_secret(&request.poll_secret)
    {
        return Err(ApiError::unauthorized("invalid device poll proof"));
    }
    let current: String = row.try_get("status").map_err(ApiError::sql)?;
    if current == "approved" {
        return Err(ApiError::status_message(
            StatusCode::CONFLICT,
            "device credential may already be issued; revoke it using device logout",
        ));
    }
    if current == "pending" {
        sqlx::query("UPDATE onboarding_device_requests SET status='denied',selection_json='[]'::jsonb,grants_json='[]'::jsonb,decided_at=now() WHERE request_id=$1")
            .bind(request.request_id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        audit(&mut tx,None,"onboarding.device.cancel",json!({"request_id":request.request_id,"device_id":row.try_get::<Uuid,_>("device_id").map_err(ApiError::sql)?})).await?;
    }
    let device: Uuid = row.try_get("device_id").map_err(ApiError::sql)?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(
        json!({"status":"denied","request_id":request.request_id,"device_id":device}),
    ))
}
pub(super) async fn lock_human(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    configuration(state)?;
    if actor.kind != AuthPrincipalKind::HumanOidc
        || actor.credential_source != CredentialSource::BrowserSession
    {
        return Err(ApiError::forbidden(
            "a current human OIDC browser session is required",
        ));
    }
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE)")
        .bind(actor.id).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !live {
        return Err(ApiError::unauthorized("human identity is disabled"));
    }
    let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_web_sessions WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR SHARE)")
        .bind(actor.session_id).bind(actor.id).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if !live {
        return Err(ApiError::unauthorized(
            "human browser session is expired or revoked",
        ));
    }
    Ok(())
}
pub(super) fn code(value: &str) -> Result<(), ApiError> {
    if value.len() != 9
        || value.as_bytes()[4] != b'-'
        || !value
            .bytes()
            .enumerate()
            .all(|(n, c)| n == 4 || c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(bad("invalid device confirmation code"));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewQuery {
    pub code: String,
}

async fn manifests(
    state: &AppState,
    actor: &AuthenticatedPrincipal,
) -> Result<Vec<OnboardingDeviceReviewProject>, ApiError> {
    let area = configuration(state)?;
    let pool = state.pool()?;
    let rows=sqlx::query("SELECT DISTINCT ON(v.project_id) v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id ORDER BY v.project_id,v.revision DESC LIMIT 1000")
        .fetch_all(&pool).await.map_err(ApiError::sql)?;
    let mut result = Vec::new();
    for row in rows {
        let manifest = super::onboarding::project_row(&row)?;
        let permissions = actor.permissions_for_project(&manifest.project);
        if manifest.entry.owner_instance_id != area.instance_id
            || manifest.entry.area.as_str() != area.name
            || !permissions.contains(Permission::MemoryRead)
        {
            continue;
        }
        let mut allowed_roles = vec![AuthRole::Reader];
        if permissions.contains(Permission::ActivityCapture) {
            allowed_roles.push(AuthRole::Writer);
        }
        let offline_available = manifest.entry.area != OnboardingArea::Trust
            && manifest.entry.offline_allowed
            && permissions.contains(Permission::BundleExport);
        result.push(OnboardingDeviceReviewProject {
            manifest,
            allowed_roles,
            offline_available,
        });
    }
    Ok(result)
}
pub(crate) async fn review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Query(query): Query<ReviewQuery>,
) -> Result<Json<OnboardingDeviceReview>, ApiError> {
    code(&query.code)?;
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_human(&state, &mut tx, &actor).await?;
    let row=sqlx::query("SELECT *,expires_at>clock_timestamp() AS active FROM onboarding_device_requests WHERE user_code=$1 FOR SHARE").bind(&query.code).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("device confirmation code not found"))?;
    if !row.try_get::<bool, _>("active").map_err(ApiError::sql)?
        || row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending"
    {
        return Err(ApiError::status_message(
            StatusCode::GONE,
            "device request is expired or already decided",
        ));
    }
    let response = OnboardingDeviceReview {
        request_id: row.try_get("request_id").map_err(ApiError::sql)?,
        device_id: row.try_get("device_id").map_err(ApiError::sql)?,
        user_code: query.code,
        name: row.try_get("name").map_err(ApiError::sql)?,
        platform: row.try_get("platform").map_err(ApiError::sql)?,
        credential_sha256: fingerprint(
            &row.try_get::<Vec<u8>, _>("credential_hash")
                .map_err(ApiError::sql)?,
        ),
        expires_at: row.try_get("expires_at").map_err(ApiError::sql)?,
        projects: manifests(&state, &actor).await?,
    };
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
}
pub(crate) async fn approve(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
    Json(mut request): Json<OnboardingDeviceApproveRequest>,
) -> Result<Json<OnboardingDevicePollResponse>, ApiError> {
    code(&request.user_code)?;
    if !request.confirmed
        || request.projects.len() > 32
        || request.projects.iter().any(|s| {
            s.project_id.is_nil()
                || s.revision < 1
                || !matches!(s.role, AuthRole::Reader | AuthRole::Writer)
        })
    {
        return Err(bad(
            "explicit confirmation and at most 32 reviewed reader/writer projects are required",
        ));
    }
    request.projects.sort_by_key(|s| s.project_id);
    if request
        .projects
        .windows(2)
        .any(|w| w[0].project_id == w[1].project_id)
    {
        return Err(bad("duplicate project selection"));
    }
    let area = configuration(&state)?.clone();
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    lock_human(&state, &mut tx, &actor).await?;
    let row=sqlx::query("SELECT *,expires_at>clock_timestamp() AS active FROM onboarding_device_requests WHERE user_code=$1 FOR UPDATE")
        .bind(&request.user_code).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::not_found("device confirmation code not found"))?;
    let company: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM onboarding_company_device_intents WHERE request_id=$1)",
    )
    .bind(
        row.try_get::<Uuid, _>("request_id")
            .map_err(ApiError::sql)?,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::sql)?;
    if company {
        return Err(ApiError::status_message(
            StatusCode::CONFLICT,
            "company enrollment must use its exact company approval path",
        ));
    }
    let device: Uuid = row.try_get("device_id").map_err(ApiError::sql)?;
    if row.try_get::<String, _>("status").map_err(ApiError::sql)? != "pending" {
        if row
            .try_get::<Option<Uuid>, _>("approved_by")
            .map_err(ApiError::sql)?
            != Some(actor.id)
            || row
                .try_get::<Option<Value>, _>("selection_json")
                .map_err(ApiError::sql)?
                != Some(json!(request.projects))
        {
            return Err(conflict());
        }
        let result = status(&mut tx, &row).await?;
        tx.commit().await.map_err(ApiError::sql)?;
        return Ok(Json(result));
    }
    if !row.try_get::<bool, _>("active").map_err(ApiError::sql)? {
        return Err(ApiError::status_message(
            StatusCode::GONE,
            "device request expired",
        ));
    }
    let mut grants = Vec::new();
    for selection in &request.projects {
        sqlx::query("SELECT id FROM projects WHERE id=$1 FOR SHARE")
            .bind(selection.project_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::sql)?
            .ok_or_else(|| ApiError::not_found("local project not found"))?;
        let catalog=sqlx::query("SELECT v.*,p.slug FROM onboarding_project_versions v JOIN projects p ON p.id=v.project_id WHERE v.project_id=$1 ORDER BY v.revision DESC LIMIT 1 FOR SHARE OF v,p")
            .bind(selection.project_id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::forbidden("project has no reviewed local manifest"))?;
        let manifest = super::onboarding::project_row(&catalog)?;
        if manifest.revision != selection.revision
            || manifest.entry.owner_instance_id != area.instance_id
            || manifest.entry.area.as_str() != area.name
        {
            return Err(conflict());
        }
        let permission = super::locked_project_permissions(
            &state,
            &mut tx,
            &mem_store::jobs::JobActor::Credential {
                principal: actor.id,
                token: None,
                session: actor.session_id,
            },
            selection.project_id,
        )
        .await?;
        if !permission.contains(Permission::MemoryRead)
            || (selection.role == AuthRole::Writer
                && !permission.contains(Permission::ActivityCapture))
        {
            return Err(ApiError::forbidden(
                "selected device role exceeds current human project access",
            ));
        }
        grants.push(OnboardingDeviceGrant {
            manifest,
            role: selection.role,
        });
    }
    let machine = Uuid::new_v4();
    let token = Uuid::new_v4();
    let name: String = row.try_get("name").map_err(ApiError::sql)?;
    let decided = if request.projects.is_empty() {
        "denied"
    } else {
        "approved"
    };
    if decided == "approved" {
        sqlx::query("INSERT INTO auth_principals(id,kind,display_name,global_permissions_json) VALUES($1,'service_token',$2,'[]'::jsonb)").bind(machine).bind(format!("Device {name}")).execute(&mut *tx).await.map_err(ApiError::sql)?;
        let credential: Vec<u8> = row.try_get("credential_hash").map_err(ApiError::sql)?;
        sqlx::query("INSERT INTO auth_service_tokens(id,principal_id,name,token_hash,token_prefix,created_by,expires_at) VALUES($1,$2,$3,$4,$5,$6,now()+interval '30 days')")
            .bind(token).bind(machine).bind(&name).bind(credential).bind(format!("device_{}",&device.simple().to_string()[..8])).bind(actor.id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        for selection in &request.projects {
            sqlx::query("INSERT INTO auth_project_memberships(id,principal_id,project_id,role,created_by) VALUES($1,$2,$3,$4,$5)")
                .bind(Uuid::new_v4()).bind(machine).bind(selection.project_id).bind(selection.role.as_str()).bind(actor.id).execute(&mut *tx).await.map_err(ApiError::sql)?;
        }
        sqlx::query("INSERT INTO onboarding_devices(id,principal_id,machine_principal_id,name,platform,service_token_id) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(device).bind(actor.id).bind(machine).bind(name).bind(row.try_get::<String,_>("platform").map_err(ApiError::sql)?).bind(token).execute(&mut *tx).await.map_err(ApiError::sql)?;
    }
    let updated=sqlx::query("UPDATE onboarding_device_requests SET status=$2,approved_by=$3,machine_principal_id=$4,selection_json=$5,grants_json=$6,decided_at=now() WHERE request_id=$1 RETURNING *")
        .bind(row.try_get::<Uuid,_>("request_id").map_err(ApiError::sql)?).bind(decided).bind(actor.id).bind((decided=="approved").then_some(machine)).bind(json!(request.projects)).bind(json!(grants)).fetch_one(&mut *tx).await.map_err(ApiError::sql)?;
    audit(&mut tx,Some(actor.id),"onboarding.device.decide",json!({"request_id":row.try_get::<Uuid,_>("request_id").map_err(ApiError::sql)?,"device_id":device,"decision":decided})).await?;
    let response = status(&mut tx, &updated).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(response))
}
pub(crate) async fn catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Value>, ApiError> {
    configuration(&state)?;
    if actor.kind == AuthPrincipalKind::HumanOidc
        && actor.credential_source == CredentialSource::BrowserSession
    {
    } else if actor.credential_source == CredentialSource::ServiceToken {
        if let Some(projects) = super::company_devices::catalog_for_device(&state, &actor).await? {
            return Ok(Json(json!({"schema_version":1,"projects":projects})));
        }
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM onboarding_devices WHERE service_token_id=$1 AND machine_principal_id=$2 AND revoked_at IS NULL)")
            .bind(actor.token_id).bind(actor.id).fetch_one(&state.pool()?).await.map_err(ApiError::sql)?;
        if !exists {
            return Err(ApiError::forbidden(
                "enrolled device or human browser identity required",
            ));
        }
    } else {
        return Err(ApiError::forbidden(
            "enrolled device or human browser identity required",
        ));
    }
    let projects = manifests(&state, &actor).await?;
    Ok(Json(json!({"schema_version":1,"projects":projects})))
}
pub(crate) async fn logout(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Value>, ApiError> {
    configuration(&state)?;
    if actor.credential_source != CredentialSource::ServiceToken {
        return Err(ApiError::forbidden(
            "current enrolled device credential required",
        ));
    }
    let pool = state.pool()?;
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let device:Option<Uuid>=sqlx::query_scalar("UPDATE onboarding_devices SET revoked_at=COALESCE(revoked_at,now()) WHERE service_token_id=$1 AND machine_principal_id=$2 RETURNING id")
        .bind(actor.token_id).bind(actor.id).fetch_optional(&mut *tx).await.map_err(ApiError::sql)?;
    let device =
        device.ok_or_else(|| ApiError::forbidden("current enrolled device credential required"))?;
    sqlx::query("UPDATE auth_service_tokens SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1")
        .bind(actor.token_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::sql)?;
    audit(
        &mut tx,
        Some(actor.id),
        "onboarding.device.logout",
        json!({"device_id":device}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(Json(json!({"logged_out":true,"device_id":device})))
}

/// Only exact POST device/logout may acknowledge/revoke this device using
/// an expired/revoked hash proof. This identity carries no other permission.
pub(super) async fn logout_proof(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<Option<AuthenticatedPrincipal>, ApiError> {
    let proof = super::credential_from_headers(headers, super::CookiePolicy::Allow)?;
    let super::Credential::ApiToken(proof) = proof else {
        return Err(ApiError::unauthorized(
            "exact enrolled device credential required",
        ));
    };
    if proof.is_empty() || proof.len() > 4096 {
        return Err(ApiError::unauthorized(
            "exact enrolled device credential required",
        ));
    }
    let row=sqlx::query("SELECT t.id AS token_id,t.principal_id,p.display_name FROM auth_service_tokens t JOIN onboarding_devices d ON d.service_token_id=t.id AND d.machine_principal_id=t.principal_id JOIN auth_principals p ON p.id=t.principal_id WHERE t.token_hash=$1 AND p.kind='service_token'")
        .bind(super::hash_secret(&proof)).fetch_optional(&state.pool()?).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::unauthorized("exact enrolled device credential required"))?;
    Ok(Some(AuthenticatedPrincipal {
        id: row.try_get("principal_id").map_err(ApiError::sql)?,
        kind: AuthPrincipalKind::ServiceToken,
        display_name: row.try_get("display_name").map_err(ApiError::sql)?,
        email: None,
        issuer: None,
        subject: None,
        groups: vec![],
        global_role: None,
        global: PermissionSet::EMPTY,
        project_roles: BTreeMap::new(),
        credential_source: CredentialSource::ServiceToken,
        token_id: Some(row.try_get("token_id").map_err(ApiError::sql)?),
        session_id: None,
        session_csrf_hash: None,
    }))
}

pub(super) async fn cap_authenticated_device(
    state: &AppState,
    pool: &sqlx::PgPool,
    token: Uuid,
    principal: &mut AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    let row=sqlx::query("SELECT d.principal_id,d.revoked_at,r.selection_json,r.status FROM onboarding_devices d LEFT JOIN onboarding_device_requests r ON r.device_id=d.id WHERE d.service_token_id=$1")
        .bind(token).fetch_optional(pool).await.map_err(ApiError::sql)?;
    let Some(row) = row else { return Ok(()) };
    if row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
        .map_err(ApiError::sql)?
        .is_some()
        || row
            .try_get::<Option<String>, _>("status")
            .map_err(ApiError::sql)?
            .as_deref()
            != Some("approved")
    {
        return Err(ApiError::unauthorized(
            "device enrollment is revoked or missing",
        ));
    }
    let human: Uuid = row.try_get("principal_id").map_err(ApiError::sql)?;
    let human_row = sqlx::query(
        "SELECT * FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL",
    )
    .bind(human)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::sql)?
    .ok_or_else(|| ApiError::unauthorized("device human identity is disabled"))?;
    let human = super::store::principal_from_row(pool, state, &human_row).await?;
    let selections: Vec<OnboardingDeviceSelection> =
        serde_json::from_value(row.try_get("selection_json").map_err(ApiError::sql)?)
            .map_err(|_| ApiError::unauthorized("invalid device approval binding"))?;
    let mut caps = BTreeMap::new();
    for selected in selections {
        if !matches!(selected.role, AuthRole::Reader | AuthRole::Writer) {
            return Err(ApiError::unauthorized("invalid device approval cap"));
        }
        let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM projects WHERE id=$1")
            .bind(selected.project_id)
            .fetch_optional(pool)
            .await
            .map_err(ApiError::sql)?;
        if let Some(slug) = slug {
            caps.insert(
                slug.clone(),
                selected
                    .role
                    .permissions()
                    .intersection(human.permissions_for_project(&slug)),
            );
        }
    }
    principal.global = PermissionSet::EMPTY;
    principal.global_role = None;
    principal.project_roles.retain(|slug, grant| {
        grant.permissions = grant
            .permissions
            .intersection(caps.get(slug).copied().unwrap_or(PermissionSet::EMPTY));
        !grant.permissions.is_empty()
    });
    Ok(())
}

pub(super) async fn locked_device_cap(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    machine: Uuid,
    token: Option<Uuid>,
    project: Uuid,
) -> Result<Option<PermissionSet>, ApiError> {
    let human:Option<Uuid>=sqlx::query_scalar("SELECT principal_id FROM onboarding_devices WHERE machine_principal_id=$1 OR service_token_id=$2")
        .bind(machine).bind(token).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    let Some(human) = human else { return Ok(None) };
    let live=sqlx::query("SELECT id FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE")
        .bind(human).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    if live.is_none() {
        return Err(ApiError::forbidden("device human identity is disabled"));
    }
    let row=sqlx::query("SELECT d.revoked_at,r.selection_json,r.status FROM onboarding_devices d JOIN onboarding_device_requests r ON r.device_id=d.id WHERE d.principal_id=$1 AND (d.machine_principal_id=$2 OR d.service_token_id=$3) FOR SHARE OF d,r")
        .bind(human).bind(machine).bind(token).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?.ok_or_else(||ApiError::forbidden("device enrollment binding missing"))?;
    if row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
        .map_err(ApiError::sql)?
        .is_some()
        || row.try_get::<String, _>("status").map_err(ApiError::sql)? != "approved"
    {
        return Err(ApiError::forbidden("device revoked"));
    }
    let selections: Vec<OnboardingDeviceSelection> =
        serde_json::from_value(row.try_get("selection_json").map_err(ApiError::sql)?)
            .map_err(|_| ApiError::forbidden("invalid device approval binding"))?;
    let cap = selections
        .iter()
        .find(|s| s.project_id == project)
        .map(|s| s.role)
        .filter(|r| matches!(r, AuthRole::Reader | AuthRole::Writer));
    let Some(cap) = cap else {
        return Ok(Some(PermissionSet::EMPTY));
    };
    // Recursion terminates at a verified human principal with no device token.
    let current = Box::pin(super::locked_project_permissions(
        state,
        tx,
        &mem_store::jobs::JobActor::Principal(human),
        project,
    ))
    .await?;
    Ok(Some(cap.permissions().intersection(current)))
}
