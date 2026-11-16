//! Human-only private inbox gateway. No agency bearer token crosses this boundary.
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use axum::{
    Extension, Json,
    extract::{Path, Query, RawQuery, State},
    http::StatusCode,
};
use codifix_identity::{BrokerClient, LinkBinding, LinkPoll};
use codifix_inbox::{
    store::{Store, admission, digest, human},
    *,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

fn unavailable() -> ApiError {
    ApiError::status_message(
        StatusCode::NOT_FOUND,
        "Network and Inbox are unavailable on this service",
    )
}
fn error(_: impl std::fmt::Display) -> ApiError {
    ApiError::status_message(
        StatusCode::CONFLICT,
        "Operation could not be completed. Refresh the conversation and check current access, authority, disclosure and budget.",
    )
}
fn config(state: &AppState) -> Result<&codifix_config::InboxConfig, ApiError> {
    let c = state
        .config
        .inbox
        .as_ref()
        .filter(|c| c.enabled && !c.home_instance_id.is_nil())
        .ok_or_else(unavailable)?;
    if !state.is_primary()
        || state.config.evaluation.active()
        || state
            .config
            .area
            .as_ref()
            .is_some_and(|a| a.instance_id != c.home_instance_id)
    {
        return Err(unavailable());
    }
    Ok(c)
}
fn store(state: &AppState) -> Result<Store, ApiError> {
    Ok(Store {
        pool: state.pool()?,
        home_instance_id: config(state)?.home_instance_id,
    })
}
fn broker(state: &AppState) -> Result<BrokerClient, ApiError> {
    BrokerClient::new(
        config(state)?
            .identity_broker_socket
            .as_ref()
            .ok_or_else(unavailable)?,
    )
    .map_err(error)
}
fn binding(
    state: &AppState,
    p: &AuthenticatedPrincipal,
    id: Uuid,
) -> Result<LinkBinding, ApiError> {
    let session = p
        .session_id
        .ok_or_else(|| ApiError::forbidden("human session required"))?;
    let home = config(state)?.home_instance_id;
    Ok(LinkBinding {
        attempt_id: id,
        principal_id: p.id,
        home_instance_id: home,
        session_binding: digest(&(home, p.id, session)).map_err(error)?,
    })
}
async fn current_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &AuthenticatedPrincipal,
) -> Result<(), ApiError> {
    human(tx, p.id).await.map_err(error)?;
    let id = p
        .session_id
        .ok_or_else(|| ApiError::forbidden("current human session required"))?;
    let session = sqlx::query("SELECT id FROM auth_web_sessions WHERE id=$1 AND principal_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() FOR SHARE").bind(id).bind(p.id).fetch_optional(&mut **tx).await.map_err(error)?;
    if session.is_none() {
        return Err(ApiError::forbidden("human session revoked or expired"));
    }
    Ok(())
}
pub(crate) async fn status(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Value>, ApiError> {
    let Ok(c) = config(&s) else {
        return Ok(Json(
            json!({"enabled":false,"home_instance_id":null,"identity":null,"identity_linking_available":false}),
        ));
    };
    let db = store(&s)?;
    let identity=sqlx::query("SELECT did,handle,linked_at FROM inbox_identity_links WHERE home_instance_id=$1 AND principal_id=$2").bind(c.home_instance_id).bind(p.id).fetch_optional(&db.pool).await.map_err(error)?.map(|r|->Result<Value,sqlx::Error>{Ok(json!({"did":r.try_get::<String,_>("did")?,"handle":r.try_get::<Option<String>,_>("handle")?,"linked_at":r.try_get::<chrono::DateTime<chrono::Utc>,_>("linked_at")?}))}).transpose().map_err(error)?;
    Ok(Json(
        json!({"enabled":true,"home_instance_id":c.home_instance_id,"identity":identity,"identity_linking_available":c.identity_broker_socket.is_some()}),
    ))
}
pub(crate) async fn specialists(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Vec<Specialist>>, ApiError> {
    Ok(Json(store(&s)?.specialists(p.id).await.map_err(error)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identifier {
    identifier: String,
}
pub(crate) async fn discover(
    State(s): State<AppState>,
    Extension(_): Extension<AuthenticatedPrincipal>,
    Json(q): Json<Identifier>,
) -> Result<Json<Value>, ApiError> {
    if q.identifier.len() > 2048 {
        return Err(error("identifier too long"));
    }
    Ok(Json(
        json!({"profiles":broker(&s)?.discover(&q.identifier).await.map_err(error)?}),
    ))
}
pub(crate) async fn metadata(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(broker(&s)?.metadata().await.map_err(error)?))
}
pub(crate) async fn callback(
    State(s): State<AppState>,
    RawQuery(q): RawQuery,
) -> Result<&'static str, ApiError> {
    broker(&s)?
        .callback(q.as_deref().unwrap_or_default())
        .await
        .map_err(error)?;
    Ok("Identity verified. Return to Codifix to finish linking your account.")
}
pub(crate) async fn link(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Json(q): Json<Identifier>,
) -> Result<Json<Value>, ApiError> {
    if q.identifier.len() > 2048 {
        return Err(error("identifier too long"));
    }
    let client = broker(&s)?;
    let b = binding(&s, &p, Uuid::new_v4())?;
    let db = store(&s)?;
    let mut tx = db.pool.begin().await.map_err(error)?;
    admission(&mut tx).await.map_err(error)?;
    current_session(&mut tx, &p).await?;
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM inbox_identity_attempts WHERE home_instance_id=$1 AND principal_id=$2 AND status='pending' AND expires_at>clock_timestamp()").bind(b.home_instance_id).bind(p.id).fetch_one(&mut *tx).await.map_err(error)?;
    if count >= 4 {
        return Err(error("too many pending attempts"));
    }
    sqlx::query("INSERT INTO inbox_identity_attempts(id,home_instance_id,principal_id,session_binding,status,expires_at) VALUES($1,$2,$3,$4,'pending',clock_timestamp()+interval '10 minutes')").bind(b.attempt_id).bind(b.home_instance_id).bind(p.id).bind(&b.session_binding).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    let start = match client.begin(&b, &q.identifier).await {
        Ok(start) => start,
        Err(cause) => {
            // The caller has not received an attempt ID and cannot poll this
            // row. Release its quota rather than stranding it for ten minutes.
            sqlx::query("UPDATE inbox_identity_attempts SET status='failed',authorization_url=NULL WHERE id=$1 AND status='pending'")
                .bind(b.attempt_id)
                .execute(&db.pool)
                .await
                .map_err(error)?;
            return Err(error(cause));
        }
    };
    sqlx::query(
        "UPDATE inbox_identity_attempts SET authorization_url=$2,expires_at=$3 WHERE id=$1",
    )
    .bind(b.attempt_id)
    .bind(&start.authorization_url)
    .bind(start.expires_at)
    .execute(&db.pool)
    .await
    .map_err(error)?;
    Ok(Json(
        json!({"attempt_id":b.attempt_id,"status":"pending","authorization_url":start.authorization_url,"expires_at":start.expires_at}),
    ))
}
pub(crate) async fn link_poll(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let b = binding(&s, &p, id)?;
    let db = store(&s)?;
    let row=sqlx::query("SELECT status,authorization_url,expires_at FROM inbox_identity_attempts WHERE id=$1 AND home_instance_id=$2 AND principal_id=$3 AND session_binding=$4").bind(id).bind(b.home_instance_id).bind(p.id).bind(&b.session_binding).fetch_one(&db.pool).await.map_err(error)?;
    let mut status: String = row.try_get("status").map_err(error)?;
    let expiry: chrono::DateTime<chrono::Utc> = row.try_get("expires_at").map_err(error)?;
    if status == "pending" && expiry <= chrono::Utc::now() {
        status = "expired".into();
    }
    if status == "pending" {
        if s.config.service.read_only {
            return Err(ApiError::forbidden(
                "identity linking is disabled in read-only mode",
            ));
        }
        let client = broker(&s)?;
        match client.poll(&b).await.map_err(error)? {
            LinkPoll::Pending => {}
            LinkPoll::Failed { .. } => {
                status = "failed".into();
                sqlx::query("UPDATE inbox_identity_attempts SET status='failed',authorization_url=NULL WHERE id=$1").bind(id).execute(&db.pool).await.map_err(error)?;
            }
            result @ LinkPoll::Complete { .. } => {
                let LinkPoll::Complete {
                    did, handle, pds, ..
                } = &result
                else {
                    unreachable!()
                };
                let mut tx = db.pool.begin().await.map_err(error)?;
                admission(&mut tx).await.map_err(error)?;
                current_session(&mut tx, &p).await?;
                let still_pending:bool=sqlx::query_scalar("SELECT status='pending' AND expires_at>clock_timestamp() FROM inbox_identity_attempts WHERE id=$1 AND session_binding=$2 FOR UPDATE").bind(id).bind(&b.session_binding).fetch_one(&mut *tx).await.map_err(error)?;
                if still_pending {
                    sqlx::query("INSERT INTO inbox_identity_links(home_instance_id,principal_id,did,handle,pds) VALUES($1,$2,$3,$4,$5) ON CONFLICT(home_instance_id,principal_id) DO UPDATE SET did=excluded.did,handle=excluded.handle,pds=excluded.pds,linked_at=clock_timestamp()").bind(b.home_instance_id).bind(p.id).bind(did).bind(handle).bind(pds).execute(&mut *tx).await.map_err(error)?;
                    sqlx::query("UPDATE inbox_identity_attempts SET status='complete',authorization_url=NULL,result=$2 WHERE id=$1").bind(id).bind(serde_json::to_value(&result).map_err(error)?).execute(&mut *tx).await.map_err(error)?;
                }
                status = sqlx::query_scalar("SELECT CASE WHEN status='pending' AND expires_at<=clock_timestamp() THEN 'expired' ELSE status END FROM inbox_identity_attempts WHERE id=$1").bind(id).fetch_one(&mut *tx).await.map_err(error)?;
                tx.commit().await.map_err(error)?;
                if status == "complete" || status == "cancelled" || status == "expired" {
                    let _ = client.acknowledge(&b).await;
                }
            }
        }
    }
    let url = if status == "pending" {
        row.try_get::<Option<String>, _>("authorization_url")
            .map_err(error)?
    } else {
        None
    };
    Ok(Json(
        json!({"attempt_id":id,"status":status,"authorization_url":url,"expires_at":expiry}),
    ))
}
pub(crate) async fn unlink(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Value>, ApiError> {
    let db = store(&s)?;
    let mut tx = db.pool.begin().await.map_err(error)?;
    admission(&mut tx).await.map_err(error)?;
    current_session(&mut tx, &p).await?;
    sqlx::query("DELETE FROM inbox_identity_links WHERE home_instance_id=$1 AND principal_id=$2")
        .bind(db.home_instance_id)
        .bind(p.id)
        .execute(&mut *tx)
        .await
        .map_err(error)?;
    sqlx::query("UPDATE inbox_identity_attempts SET status='cancelled',authorization_url=NULL WHERE home_instance_id=$1 AND principal_id=$2 AND status='pending'").bind(db.home_instance_id).bind(p.id).execute(&mut *tx).await.map_err(error)?;
    tx.commit().await.map_err(error)?;
    Ok(Json(json!({"unlinked":true})))
}
pub(crate) async fn list(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
) -> Result<Json<Vec<Conversation>>, ApiError> {
    Ok(Json(store(&s)?.list(p.id).await.map_err(error)?))
}
pub(crate) async fn create(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Json(q): Json<CreateConversation>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(store(&s)?.create(p.id, &q).await.map_err(error)?))
}
pub(crate) async fn detail(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(store(&s)?.detail(p.id, id).await.map_err(error)?))
}
pub(crate) async fn send(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(q): Json<SendMessage>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(store(&s)?.send(p.id, id, &q).await.map_err(error)?))
}
pub(crate) async fn join(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(q): Json<AddMember>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?.add_member(p.id, id, &q).await.map_err(error)?,
    ))
}
pub(crate) async fn remove(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path((id, member)): Path<(Uuid, Uuid)>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?
            .remove_member(p.id, id, member)
            .await
            .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lead {
    member_id: Uuid,
}
pub(crate) async fn lead(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(q): Json<Lead>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?
            .lead(p.id, id, q.member_id)
            .await
            .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    sequence: i64,
}
pub(crate) async fn read(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(q): Json<Read>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?.read(p.id, id, q.sequence).await.map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pause {
    paused: bool,
}
pub(crate) async fn pause(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Json(q): Json<Pause>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?.pause(p.id, id, q.paused).await.map_err(error)?,
    ))
}
pub(crate) async fn cancel(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path((id, task)): Path<(Uuid, Uuid)>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?.cancel(p.id, id, task).await.map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Answer {
    client_message_id: Uuid,
    answer: String,
}
pub(crate) async fn respond(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path((id, task)): Path<(Uuid, Uuid)>,
    Json(q): Json<Answer>,
) -> Result<Json<Detail>, ApiError> {
    Ok(Json(
        store(&s)?
            .respond(p.id, id, task, q.client_message_id, q.answer)
            .await
            .map_err(error)?,
    ))
}

#[cfg(test)]
mod candidate;
#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryPage {
    before_sequence: i64,
    limit: Option<u32>,
}
pub(crate) async fn history(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
    Query(q): Query<HistoryPage>,
) -> Result<Json<MessagePage>, ApiError> {
    Ok(Json(
        store(&s)?
            .message_page(p.id, id, q.before_sequence, q.limit.unwrap_or(100))
            .await
            .map_err(error)?,
    ))
}
