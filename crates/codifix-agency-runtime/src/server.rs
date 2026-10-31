use crate::{
    config::{Auth, Config, Peer},
    store::{Store, StoredTask, Submit},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use codifix_agency::{
    Mandate,
    a2a::{self, Part},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct App {
    pub store: Store,
    pub config: Arc<Config>,
    pub auth: Auth,
}
type ApiResult<T> = Result<Json<T>, (StatusCode, Json<Value>)>;
fn failure(status: StatusCode, code: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error":{"code":code}})))
}
fn peer(app: &App, headers: &HeaderMap) -> Result<Peer, (StatusCode, Json<Value>)> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| failure(StatusCode::UNAUTHORIZED, "unauthorized"))?;
    app.auth
        .authenticate(token)
        .ok_or_else(|| failure(StatusCode::UNAUTHORIZED, "unauthorized"))
}
fn version(headers: &HeaderMap) -> Result<(), (StatusCode, Json<Value>)> {
    if headers.get("a2a-version").and_then(|v| v.to_str().ok()) != Some("1.0") {
        return Err(failure(StatusCode::BAD_REQUEST, "VersionNotSupportedError"));
    }
    Ok(())
}
fn safe_error(_: anyhow::Error) -> (StatusCode, Json<Value>) {
    failure(StatusCode::CONFLICT, "operation_denied_or_conflict")
}
pub fn router(app: App) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/.well-known/agent-card.json", get(card))
        .route("/message:send", post(send_message))
        .route("/tasks", get(a2a_list))
        .route("/tasks/{id}", get(a2a_get).post(a2a_cancel_route))
        .route("/v1/tasks", get(list).post(submit))
        .route("/v1/tasks/{id}", get(detail))
        .route("/v1/tasks/{id}/cancel", post(cancel))
        .route("/v1/mandates", post(grant))
        .route("/v1/mandates/{id}/{action}", post(control))
        .route("/v1/schedules", post(schedule))
        .route("/v1/improvements", post(improvement))
        .layer(DefaultBodyLimit::max(262144))
        .with_state(app)
}
async fn card(State(app): State<App>) -> ApiResult<a2a::AgentCard> {
    let value = app
        .config
        .public_profile
        .clone()
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "profile_not_published"))?;
    let profile: codifix_agency::discovery::PublicCapabilityProfile = serde_json::from_value(value)
        .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "invalid_public_profile"))?;
    let mut card = a2a::AgentCard::from_profile(
        &profile,
        app.config.public_url.clone(),
        env!("CARGO_PKG_VERSION").into(),
        "HTTP+JSON",
    )
    .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "invalid_public_profile"))?;
    card.require_bearer();
    Ok(Json(card))
}
fn check_specialist(app: &App, req: &Submit) -> Result<(), (StatusCode, Json<Value>)> {
    if !app
        .config
        .specialists
        .iter()
        .any(|s| s.specialist_id == req.specialist_id && s.capabilities.contains(&req.capability))
    {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "unknown_specialist_or_capability",
        ));
    }
    Ok(())
}
async fn submit(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<Submit>,
) -> ApiResult<StoredTask> {
    let peer = peer(&app, &headers)?;
    check_specialist(&app, &req)?;
    Ok(Json(
        app.store.submit(&req, &peer).await.map_err(safe_error)?,
    ))
}
async fn list(State(app): State<App>, headers: HeaderMap) -> ApiResult<Vec<StoredTask>> {
    let p = peer(&app, &headers)?;
    Ok(Json(app.store.list(&p).await.map_err(safe_error)?))
}
async fn detail(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<StoredTask> {
    let p = peer(&app, &headers)?;
    Ok(Json(app.store.get(id, &p).await.map_err(safe_error)?))
}
async fn cancel(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<StoredTask> {
    let p = peer(&app, &headers)?;
    Ok(Json(app.store.cancel(id, &p).await.map_err(safe_error)?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    project_id: Uuid,
    mandate: Mandate,
}
async fn grant(
    State(app): State<App>,
    headers: HeaderMap,
    Json(grant): Json<Grant>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    app.store
        .install_mandate(grant.project_id, &grant.mandate, &p, app.config.instance_id)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"id":grant.mandate.id})))
}
async fn control(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, action)): Path<(Uuid, String)>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    app.store
        .control_mandate(id, &action, &p)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"id":id,"action":action})))
}
async fn project_task(app: &App, task: StoredTask) -> Result<a2a::Task, (StatusCode, Json<Value>)> {
    let mut wire = a2a::Task::from_contract(&task.contract, task.contract.id.to_string())
        .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "invalid_task"))?;
    if let Some(c) = &task.contract.clarification {
        wire.status.message = Some(a2a::Message {
            message_id: format!("{}:{}", task.contract.id, task.revision),
            role: a2a::Role::Agent,
            parts: vec![Part::Text(c.question.clone())],
            context_id: Some(wire.context_id.clone()),
            task_id: Some(wire.id.clone()),
        });
    }
    let evidence: Option<Value> =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_optional(&app.store.pool)
            .await
            .map_err(|_| failure(StatusCode::INTERNAL_SERVER_ERROR, "storage_unavailable"))?;
    if let Some(evidence) = evidence {
        wire.artifacts.push(a2a::Artifact {
            artifact_id: format!("{}:evidence", task.contract.id),
            parts: vec![Part::Data(evidence)],
        });
    }
    Ok(wire)
}
async fn a2a_get(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<a2a::Task> {
    version(&headers)?;
    let p = peer(&app, &headers)?;
    let task = app.store.get(id, &p).await.map_err(safe_error)?;
    Ok(Json(project_task(&app, task).await?))
}
async fn a2a_list(State(app): State<App>, headers: HeaderMap) -> ApiResult<Value> {
    version(&headers)?;
    let p = peer(&app, &headers)?;
    let tasks = app.store.list(&p).await.map_err(safe_error)?;
    let mut out = Vec::new();
    for task in tasks {
        out.push(project_task(&app, task).await?);
    }
    Ok(Json(
        json!({"tasks":out,"pageSize":out.len(),"nextPageToken":""}),
    ))
}
async fn a2a_cancel_route(
    State(app): State<App>,
    headers: HeaderMap,
    Path(action): Path<String>,
) -> ApiResult<a2a::Task> {
    let id = action
        .strip_suffix(":cancel")
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "unknown_task_operation"))?;
    a2a_cancel(State(app), headers, Path(id)).await
}
async fn a2a_cancel(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<a2a::Task> {
    version(&headers)?;
    let p = peer(&app, &headers)?;
    let task = app.store.cancel(id, &p).await.map_err(safe_error)?;
    Ok(Json(project_task(&app, task).await?))
}
async fn send_message(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<a2a::SendMessageRequest>,
) -> ApiResult<a2a::SendMessageResponse> {
    version(&headers)?;
    let p = peer(&app, &headers)?;
    let interface = a2a::AgentInterface {
        url: app.config.public_url.clone(),
        protocol_binding: "HTTP+JSON".into(),
        protocol_version: "1.0".into(),
        tenant: None,
    };
    let data: Vec<_> = req
        .message
        .parts
        .iter()
        .filter_map(|p| if let Part::Data(v) = p { Some(v) } else { None })
        .collect();
    if data.len() != 1 {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "one_codifix_data_part_required",
        ));
    }
    let task = if let Some(id) = &req.message.task_id {
        let id =
            Uuid::parse_str(id).map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_task_id"))?;
        let current = app.store.get(id, &p).await.map_err(safe_error)?;
        let wire = project_task(&app, current).await?;
        req.validate(&interface, Some(&wire))
            .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_context"))?;
        let answer = data[0]["answer"]
            .as_str()
            .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "answer_required"))?;
        app.store
            .respond(
                id,
                &req.message.message_id,
                answer,
                data[0].get("replacement_payload").cloned(),
                &p,
            )
            .await
            .map_err(safe_error)?
    } else {
        req.validate(&interface, None)
            .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_message"))?;
        // Each task owns a fresh context. Unbound caller context IDs are not accepted.
        if req.message.context_id.is_some() {
            return Err(failure(
                StatusCode::BAD_REQUEST,
                "new_task_context_is_server_assigned",
            ));
        }
        let mut submit: Submit = serde_json::from_value(data[0].clone())
            .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_task_contract"))?;
        submit.request_id = req.message.message_id.clone();
        check_specialist(&app, &submit)?;
        app.store.submit(&submit, &p).await.map_err(safe_error)?
    };
    Ok(Json(a2a::SendMessageResponse::Task(
        project_task(&app, task).await?,
    )))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleRequest {
    id: Uuid,
    interval_seconds: u32,
    template: Submit,
}
async fn schedule(
    State(app): State<App>,
    headers: HeaderMap,
    Json(req): Json<ScheduleRequest>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    if !p.operator {
        return Err(failure(StatusCode::FORBIDDEN, "operator_required"));
    }
    if !(30..=604800).contains(&req.interval_seconds) {
        return Err(failure(StatusCode::BAD_REQUEST, "invalid_interval"));
    }
    check_specialist(&app, &req.template)?;
    sqlx::query("INSERT INTO agency_schedules(id,mandate_id,requester,interval_seconds,next_at,template,enabled) VALUES($1,$2,$3,$4,clock_timestamp(),$5,true)")
        .bind(req.id).bind(req.template.mandate_id).bind(&p.id).bind(i64::from(req.interval_seconds)).bind(serde_json::to_value(&req.template).map_err(|_|failure(StatusCode::BAD_REQUEST,"invalid_template"))?).execute(&app.store.pool).await.map_err(|_|failure(StatusCode::CONFLICT,"schedule_conflict"))?;
    Ok(Json(json!({"id":req.id})))
}
async fn improvement(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    if !p.operator {
        return Err(failure(StatusCode::FORBIDDEN, "operator_required"));
    }
    crate::worker::improve(&app.store, &app.config, input)
        .await
        .map(Json)
        .map_err(safe_error)
}
