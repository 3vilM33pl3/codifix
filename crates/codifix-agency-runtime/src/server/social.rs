use super::*;
use chrono::{DateTime, Utc};

pub(super) fn routes() -> Router<App> {
    Router::new()
        .route("/xrpc/dev.codifix.agency.checkUpgrade", post(check_upgrade))
        .route("/xrpc/dev.codifix.agency.sendMessage", post(send))
        .route("/xrpc/dev.codifix.agency.deliverNotice", post(notice))
        .route(
            "/xrpc/dev.codifix.agency.deliverIncidentNotice",
            post(incident_notice),
        )
        .route("/xrpc/dev.codifix.agency.getTask", get(status))
        .route("/xrpc/dev.codifix.agency.findRequest", get(lookup))
        .route("/xrpc/dev.codifix.agency.renewLease", post(renew))
        .route("/xrpc/dev.codifix.agency.cancelTask", post(stop))
        .route("/v1/social/moderation", post(moderate))
}

async fn authenticate(
    app: &App,
    headers: &HeaderMap,
    method: &str,
) -> Result<Peer, (StatusCode, Json<Value>)> {
    let social = app
        .config
        .social
        .as_ref()
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "disabled"))?;
    let unauthorized = || failure(StatusCode::UNAUTHORIZED, "invalid_service_token");
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    let (peer, claims) = social
        .verify(&app.config.peers, token, method, Utc::now().timestamp())
        .map_err(|_| unauthorized())?;
    let unavailable = |_| failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable");
    let mut tx = app.store.pool.begin().await.map_err(unavailable)?;
    let blocked:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agency_social_moderation WHERE instance_id=$1 AND peer_id=$2 AND (banned_until IS NULL OR banned_until>clock_timestamp()))")
        .bind(app.config.instance_id).bind(&peer.id).fetch_one(&mut *tx).await.map_err(unavailable)?;
    if blocked {
        return Err(failure(StatusCode::FORBIDDEN, "peer_blocked"));
    }
    sqlx::query("DELETE FROM agency_social_used_tokens WHERE expires_at <= clock_timestamp()")
        .execute(&mut *tx)
        .await
        .map_err(unavailable)?;
    let accepted = sqlx::query("INSERT INTO agency_social_used_tokens(instance_id,peer_id,jti,expires_at) VALUES($1,$2,$3,to_timestamp($4)) ON CONFLICT DO NOTHING")
        .bind(app.config.instance_id).bind(&peer.id).bind(claims.jti).bind(claims.exp as f64).execute(&mut *tx).await.map_err(unavailable)?;
    if accepted.rows_affected() == 0 {
        return Err(unauthorized());
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(peer.clone())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id<T> {
    id: T,
}

async fn send(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<a2a::SendMessageRequest>,
) -> ApiResult<a2a::SendMessageResponse> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.sendMessage").await?;
    receive(&app, peer, req).await
}
async fn status(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<Id<Uuid>>,
) -> ApiResult<a2a::Task> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.getTask").await?;
    let task = app.store.get(q.id, &peer).await.map_err(safe_error)?;
    Ok(Json(project_task(&app, task).await?))
}
async fn lookup(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<Id<String>>,
) -> ApiResult<StoredTask> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.findRequest").await?;
    lookup_for(&app, &peer, &q.id).await
}
async fn renew(
    State(app): State<App>,
    headers: HeaderMap,
    Json(q): Json<Id<Uuid>>,
) -> ApiResult<Value> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.renewLease").await?;
    app.store
        .renew_execution_lease(q.id, &peer)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"renewed":true})))
}
async fn stop(
    State(app): State<App>,
    headers: HeaderMap,
    Json(q): Json<Id<Uuid>>,
) -> ApiResult<a2a::Task> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.cancelTask").await?;
    let task = app.store.cancel(q.id, &peer).await.map_err(safe_error)?;
    Ok(Json(project_task(&app, task).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Moderation {
    peer_id: String,
    action: String,
    until: Option<DateTime<Utc>>,
}

async fn moderate(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Moderation>,
) -> ApiResult<Value> {
    let actor = peer(&app, &headers)?;
    if !actor.operator && actor.instance_id != app.config.instance_id {
        return Err(failure(StatusCode::FORBIDDEN, "local_agent_required"));
    }
    if !app
        .config
        .social
        .as_ref()
        .is_some_and(|social| social.peers.contains_key(&input.peer_id))
    {
        return Err(failure(StatusCode::NOT_FOUND, "unknown_social_peer"));
    }
    let unavailable = |_| failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable");
    let mut tx = app.store.pool.begin().await.map_err(unavailable)?;
    crate::store::admission(&mut tx)
        .await
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"))?;
    match (input.action.as_str(), input.until) {
        ("allow", None) => {
            sqlx::query("DELETE FROM agency_social_moderation WHERE instance_id=$1 AND peer_id=$2")
                .bind(app.config.instance_id)
                .bind(&input.peer_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"))?;
        }
        ("remove", None) | ("ban", Some(_)) if input.until.is_none_or(|t| t > Utc::now()) => {
            sqlx::query("INSERT INTO agency_social_moderation(instance_id,peer_id,banned_until) VALUES($1,$2,$3) ON CONFLICT(instance_id,peer_id) DO UPDATE SET banned_until=excluded.banned_until").bind(app.config.instance_id).bind(&input.peer_id).bind(input.until).execute(&mut *tx).await.map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE,"storage_unavailable"))?;
        }
        _ => return Err(failure(StatusCode::BAD_REQUEST, "invalid_moderation")),
    }
    tx.commit().await.map_err(unavailable)?;
    Ok(Json(json!({"peer_id":input.peer_id,"action":input.action})))
}

async fn incident_notice(
    State(app): State<App>,
    headers: HeaderMap,
    Json(notice): Json<crate::notice::IncidentNotice>,
) -> ApiResult<Value> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.deliverIncidentNotice").await?;
    notice
        .validate()
        .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_incident_notice"))?;
    let sender = app
        .config
        .social
        .as_ref()
        .and_then(|s| s.peers.get(&peer.id))
        .ok_or_else(|| failure(StatusCode::FORBIDDEN, "unenrolled_peer"))?;
    let chat = sender
        .incident_conversation
        .filter(|_| app.config.inbox_enabled)
        .ok_or_else(|| failure(StatusCode::FORBIDDEN, "incident_inbox_not_enrolled"))?;
    notice
        .receive(&app.store, app.config.instance_id, chat, &sender.did)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"received":true})))
}

async fn notice(
    State(app): State<App>,
    headers: HeaderMap,
    Json(notice): Json<crate::notice::ScenarioNotice>,
) -> ApiResult<Value> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.deliverNotice").await?;
    notice
        .validate()
        .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_notice"))?;
    let sender = app
        .config
        .social
        .as_ref()
        .and_then(|s| s.peers.get(&peer.id))
        .ok_or_else(|| failure(StatusCode::FORBIDDEN, "unenrolled_peer"))?;
    let inbox = sender
        .notice_inbox
        .as_ref()
        .filter(|_| app.config.inbox_enabled)
        .ok_or_else(|| failure(StatusCode::FORBIDDEN, "notice_inbox_not_enrolled"))?;
    let mut body = format!(
        "{} — {}. From {}. Attempt {}.",
        notice.event.text(),
        notice.subject,
        sender.did,
        notice.attempt
    );
    if let Some(person) = &notice.participant {
        body.push_str(&format!(" Participant {person}."));
    }
    let handoff = inbox
        .unseal_url
        .as_deref()
        .filter(|_| notice.event == crate::notice::NoticeEvent::AwaitingOperator);
    if let Some(link) = handoff {
        body.push_str(&format!(" Protected handoff: {link}"));
    }
    let id = Uuid::new_v5(
        &inbox.conversation_id,
        &serde_json::to_vec(&(&sender.did, &notice))
            .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_notice"))?,
    );
    let store = codifix_inbox::store::Store {
        pool: app.store.pool.clone(),
        home_instance_id: app.config.instance_id,
    };
    store
        .receive_notice(inbox.conversation_id, id, &body, handoff)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"received":true})))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpgradeCheck {
    publisher_task: Uuid,
    ca_task: Uuid,
    nonce: Uuid,
    version: String,
}

async fn check_upgrade(
    State(app): State<App>,
    headers: HeaderMap,
    Json(q): Json<UpgradeCheck>,
) -> ApiResult<UpgradeCheck> {
    let peer = authenticate(&app, &headers, "dev.codifix.agency.checkUpgrade").await?;
    let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agency_remote_tasks r JOIN agency_tasks t ON t.id=r.task_id WHERE r.task_id=$1 AND r.remote_id=$2 AND r.peer_id=$3 AND t.payload#>>'{payload,request,capability}'='upgrade_candidate' AND t.payload#>>'{payload,request,payload,release,version}'=$4)")
        .bind(q.publisher_task).bind(q.ca_task.to_string()).bind(&peer.id).bind(&q.version).fetch_one(&app.store.pool).await.map_err(|err| safe_error(err.into()))?;
    if !known || q.nonce.is_nil() {
        return Err(failure(StatusCode::FORBIDDEN, "unrecognized_rollout"));
    }
    Ok(Json(q))
}
