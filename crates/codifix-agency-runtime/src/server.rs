use crate::{
    config::{Auth, Config, Peer},
    store::{Store, StoredTask, Submit},
};
use axum::{
    Json, Router,
    extract::Request,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use codifix_agency::{
    Mandate,
    a2a::{self, Part},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
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
    let kind = match code {
        "VersionNotSupportedError" => "https://a2a-protocol.org/errors/version-not-supported",
        "PushNotificationNotSupportedError" => {
            "https://a2a-protocol.org/errors/push-notification-not-supported"
        }
        _ => "about:blank",
    };
    let mut body = json!({"type":kind,"title":status.canonical_reason().unwrap_or("Request failed"),"status":status.as_u16(),"detail":code,"error":{"code":code}});
    if code == "VersionNotSupportedError" {
        body["supportedVersions"] = json!(["1.0"]);
    }
    (status, Json(body))
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
        .route("/operator", get(crate::operator::shell))
        .route("/.well-known/agent-card.json", get(card))
        .route("/message:send", post(send_message))
        .route("/tasks", get(a2a_list))
        .route("/tasks/{id}", get(a2a_get).post(a2a_cancel_route))
        .route("/v1/tasks", get(list).post(submit))
        .route("/v1/tasks/{id}", get(detail))
        .route("/v1/tasks/{id}/cancel", post(cancel))
        .route("/v1/tasks/{id}/lease", post(renew_lease))
        .route("/v1/mandates", post(grant))
        .route("/v1/mandates/{id}/{action}", post(control))
        .route("/v1/schedules", post(schedule))
        .route("/v1/improvements", post(improvement))
        .layer(DefaultBodyLimit::max(262144))
        .layer(middleware::from_fn(wire_format))
        .with_state(app)
}
// Normalize extractor failures as well as application failures. Provider errors
// and request bodies are never copied into public protocol errors.
async fn wire_format(request: Request, next: Next) -> Response {
    let a2a = request.uri().path() == "/message:send"
        || request.uri().path() == "/tasks"
        || request.uri().path().starts_with("/tasks/");
    let mut response = next.run(request).await;
    if a2a {
        if !response.status().is_success()
            && response
                .headers()
                .get("content-type")
                .is_none_or(|v| v != "application/json")
        {
            response = failure(response.status(), "invalid_request").into_response();
        }
        let kind = if response.status().is_success() {
            "application/a2a+json"
        } else {
            "application/problem+json"
        };
        response
            .headers_mut()
            .insert("content-type", axum::http::HeaderValue::from_static(kind));
        response
            .headers_mut()
            .insert("a2a-version", axum::http::HeaderValue::from_static("1.0"));
    }
    response
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
    if let Some(trace) = req.payload.get("delegation_trace") {
        let trace: Vec<Uuid> = serde_json::from_value(trace.clone())
            .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_delegation_trace"))?;
        if trace.len() > 16
            || trace.contains(&app.config.instance_id)
            || trace
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != trace.len()
        {
            return Err(failure(StatusCode::BAD_REQUEST, "delegation_cycle"));
        }
    }
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
async fn renew_lease(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    app.store
        .renew_execution_lease(id, &p)
        .await
        .map_err(safe_error)?;
    Ok(Json(json!({"renewed":true})))
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
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListQuery {
    page_size: Option<u32>,
    page_token: Option<String>,
    context_id: Option<Uuid>,
    status: Option<a2a::TaskState>,
    history_length: Option<u32>,
    status_timestamp_after: Option<i64>,
    include_artifacts: Option<bool>,
}
#[derive(Deserialize, Serialize)]
struct Cursor {
    at: chrono::DateTime<chrono::Utc>,
    id: Uuid,
    binding: String,
}
async fn a2a_list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> ApiResult<Value> {
    version(&headers)?;
    let p = peer(&app, &headers)?;
    let page_size = query.page_size.unwrap_or(50);
    if !(1..=100).contains(&page_size) {
        return Err(failure(StatusCode::BAD_REQUEST, "invalid_page_size"));
    }
    let states: Option<Vec<&str>> = query.status.map(|s| match s {
        a2a::TaskState::Submitted => vec!["submitted"],
        a2a::TaskState::Working => vec!["working"],
        a2a::TaskState::Completed => vec!["completed"],
        a2a::TaskState::Failed => vec!["failed"],
        a2a::TaskState::Canceled => vec!["canceled"],
        a2a::TaskState::InputRequired => vec!["input_required", "plan_review_required"],
        a2a::TaskState::Rejected => vec!["rejected"],
        _ => vec![],
    });
    let binding=crate::store::fingerprint(&json!({"peer":p.id,"page_size":page_size,"context":query.context_id,"states":states,"after":query.status_timestamp_after,"artifacts":query.include_artifacts})).map_err(safe_error)?;
    let cursor: Option<Cursor> = query
        .page_token
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|token| app.auth.decode_cursor(token))
        .transpose()
        .map_err(|_| failure(StatusCode::BAD_REQUEST, "invalid_page_token"))?;
    if cursor.as_ref().is_some_and(|c| c.binding != binding) {
        return Err(failure(StatusCode::BAD_REQUEST, "invalid_page_token"));
    }
    let after = query
        .status_timestamp_after
        .map(|ms| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
                .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "invalid_timestamp"))
        })
        .transpose()?;
    let total:i64=sqlx::query_scalar("SELECT count(*) FROM agency_tasks WHERE ($1 OR requester=$2) AND ($3::uuid IS NULL OR id=$3) AND ($4::text[] IS NULL OR contract->>'status'=ANY($4)) AND ($5::timestamptz IS NULL OR (contract->>'updated_at')::timestamptz>$5)")
        .bind(p.operator).bind(&p.id).bind(query.context_id).bind(&states).bind(after).fetch_one(&app.store.pool).await.map_err(|_|failure(StatusCode::INTERNAL_SERVER_ERROR,"storage_unavailable"))?;
    let rows=sqlx::query("SELECT * FROM agency_tasks WHERE ($1 OR requester=$2) AND ($3::uuid IS NULL OR id=$3) AND ($4::text[] IS NULL OR contract->>'status'=ANY($4)) AND ($5::timestamptz IS NULL OR (contract->>'updated_at')::timestamptz>$5) AND ($6::timestamptz IS NULL OR (created_at,id)<($6,$7)) ORDER BY created_at DESC,id DESC LIMIT $8")
        .bind(p.operator).bind(&p.id).bind(query.context_id).bind(&states).bind(after).bind(cursor.as_ref().map(|c|c.at)).bind(cursor.as_ref().map(|c|c.id)).bind(i64::from(page_size)+1).fetch_all(&app.store.pool).await.map_err(|_|failure(StatusCode::INTERNAL_SERVER_ERROR,"storage_unavailable"))?;
    let more = rows.len() > page_size as usize;
    let mut tasks = Vec::new();
    let mut next = String::new();
    for (index, row) in rows.iter().take(page_size as usize).enumerate() {
        let task = crate::store::decode(row).map_err(safe_error)?;
        if more && index + 1 == page_size as usize {
            next = app
                .auth
                .encode_cursor(&Cursor {
                    at: row.try_get("created_at").map_err(|_| {
                        failure(StatusCode::INTERNAL_SERVER_ERROR, "invalid_cursor")
                    })?,
                    id: task.contract.id,
                    binding: binding.clone(),
                })
                .map_err(safe_error)?;
        }
        let mut wire = project_task(&app, task).await?;
        if !query.include_artifacts.unwrap_or(false) {
            wire.artifacts.clear();
        }
        tasks.push(wire);
    }
    Ok(Json(
        json!({"tasks":tasks,"totalSize":total,"pageSize":page_size,"nextPageToken":next}),
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
    let configuration = req.configuration.clone().unwrap_or_default();
    if configuration.task_push_notification_config.is_some() {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "PushNotificationNotSupportedError",
        ));
    }
    if !configuration.accepted_output_modes.is_empty()
        && !configuration
            .accepted_output_modes
            .iter()
            .any(|s| s == "application/json")
    {
        return Err(failure(
            StatusCode::BAD_REQUEST,
            "ContentTypeNotSupportedError",
        ));
    }
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
                data[0]
                    .get("replacement_payload")
                    .filter(|v| !v.is_null())
                    .cloned(),
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
    let mut task = task;
    if !configuration.return_immediately {
        let timeout = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        while matches!(
            task.contract.status,
            codifix_agency::TaskStatus::Submitted | codifix_agency::TaskStatus::Working
        ) {
            if tokio::time::Instant::now() >= timeout {
                let (status, Json(mut problem)) =
                    failure(StatusCode::REQUEST_TIMEOUT, "task_continues_poll_by_id");
                problem["taskId"] = json!(task.contract.id);
                problem["contextId"] = json!(task.contract.id);
                return Err((status, Json(problem)));
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            task = app
                .store
                .get(task.contract.id, &p)
                .await
                .map_err(safe_error)?;
        }
    }
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
