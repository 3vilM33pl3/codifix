// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only area/project/private-peer observations. Central auth remains authoritative.
use crate::{ApiError, AppState, AuthenticatedPrincipal};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use codifix_config::agency_observation::AgencyObservationConfig;
use codifix_record::{Permission, agency_observation::*};
use serde::Deserialize;
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

fn unavailable() -> ApiError {
    ApiError::not_found("Companion observations are unavailable for this area")
}
fn database_error(_: sqlx::Error) -> ApiError {
    ApiError::service_unavailable("Companion observation schema or database is unavailable")
}
fn config(s: &AppState) -> Result<&AgencyObservationConfig, ApiError> {
    let c = s
        .config
        .agency_observation
        .as_ref()
        .filter(|c| c.enabled)
        .ok_or_else(unavailable)?;
    let a = s.config.area.as_ref().ok_or_else(unavailable)?;
    if !s.is_primary()
        || s.config.evaluation.active()
        || a.validate().is_err()
        || c.area != a.name
        || c.instance_id != a.instance_id
    {
        return Err(unavailable());
    }
    Ok(c)
}
fn exchange_access(c: &AgencyObservationConfig, p: &AuthenticatedPrincipal, project: &str) -> bool {
    p.has_for_project(project, Permission::MemoryCurate)
        && c.exchange_projects
            .iter()
            .any(|g| g.project == project && g.principal_ids.contains(&p.id))
}
async fn provenance(pool: &PgPool, c: &AgencyObservationConfig) -> Result<(), ApiError> {
    // A conflicting import receipt cannot be overridden by a heartbeat.
    let receipt =
        sqlx::query("SELECT area,target_instance FROM area_import_receipts WHERE singleton=true")
            .fetch_optional(pool)
            .await
            .map_err(database_error)?;
    if let Some(r) = receipt {
        if r.try_get::<String, _>("area").map_err(database_error)? == c.area
            && r.try_get::<Uuid, _>("target_instance")
                .map_err(database_error)?
                == c.instance_id
        {
            return Ok(());
        }
        return Err(unavailable());
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agency_runtime_heartbeats WHERE instance_id=$1)",
    )
    .bind(c.instance_id)
    .fetch_one(pool)
    .await
    .map_err(database_error)?;
    if exists { Ok(()) } else { Err(unavailable()) }
}
pub(crate) async fn companion(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(project): Path<String>,
) -> Result<Json<CompanionStatus>, ApiError> {
    let mut response = CompanionStatus {
        enabled: false,
        area: None,
        instance_id: None,
        state: "disabled".into(),
        last_heartbeat_at: None,
        version: None,
        exchange_access: false,
        enabled_specialists: vec![],
        enrolled_peers: vec![],
        last_exchange_at: None,
    };
    let Some(_) = s.config.agency_observation.as_ref().filter(|c| c.enabled) else {
        return Ok(Json(response));
    };
    let c = config(&s)?;
    let pool = s.pool()?;
    provenance(&pool, c).await?;
    response.enabled = true;
    response.area = Some(c.area.clone());
    response.instance_id = Some(c.instance_id);
    response.exchange_access = exchange_access(c, &p, &project);
    let heartbeat = sqlx::query("SELECT observed_at,version,enabled_specialists,enrolled_peers,observed_at BETWEEN clock_timestamp()-interval '90 seconds' AND clock_timestamp()+interval '5 seconds' AS fresh FROM agency_runtime_heartbeats WHERE instance_id=$1")
        .bind(c.instance_id).fetch_optional(&pool).await.map_err(database_error)?;
    response.state = "unknown".into();
    if let Some(r) = heartbeat {
        response.enabled_specialists = r
            .try_get::<Vec<String>, _>("enabled_specialists")
            .map_err(database_error)?
            .iter()
            .take(100)
            .map(|s| label(s, 128))
            .collect();
        response.enrolled_peers = r
            .try_get::<Vec<String>, _>("enrolled_peers")
            .map_err(database_error)?
            .iter()
            .take(100)
            .map(|s| label(s, 128))
            .collect();
        response.last_heartbeat_at = Some(r.try_get("observed_at").map_err(database_error)?);
        response.version = Some(label(
            &r.try_get::<String, _>("version").map_err(database_error)?,
            128,
        ));
        response.state = if r.try_get::<bool, _>("fresh").map_err(database_error)? {
            "fresh"
        } else {
            "stale"
        }
        .into();
    }
    if response.exchange_access {
        response.last_exchange_at =
            sqlx::query_scalar(&format!("SELECT max(updated_at) FROM ({SELECT}) eligible"))
                .bind(&project)
                .bind(&c.peer_ids)
                .bind(serde_json::to_value(&c.legacy_exchanges).map_err(|_| unavailable())?)
                .bind(c.instance_id.to_string())
                .fetch_one(&pool)
                .await
                .map_err(database_error)?;
    }
    Ok(Json(response))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageQuery {
    peer: Option<String>,
    before: Option<String>,
    limit: Option<i64>,
}
fn cursor(before: Option<&str>) -> Result<(DateTime<Utc>, Uuid), ApiError> {
    let invalid = || {
        ApiError::status_message(
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid exchange cursor",
        )
    };
    match before {
        None => Ok((DateTime::<Utc>::MAX_UTC, Uuid::max())),
        Some(s) if s.len() <= 128 => {
            let (at, id) = s.split_once('|').ok_or_else(invalid)?;
            Ok((
                at.parse().map_err(|_| invalid())?,
                id.parse().map_err(|_| invalid())?,
            ))
        }
        _ => Err(invalid()),
    }
}
// Exact project, root-only and provenance exclusions are applied before pagination.
// Never deserialize a whole payload into an API record.
const SELECT: &str = r#"SELECT t.id,t.created_at,GREATEST(t.updated_at,(SELECT max(e.recorded_at) FROM agency_observation_events e WHERE e.task_id=t.id)) AS updated_at,t.contract->>'specialist_id' AS specialist_id,
 t.contract->>'status' AS status,
 left(coalesce(t.payload#>>'{payload,request,objective}',t.contract->>'objective',''),4096) AS objective,
 CASE WHEN t.payload->>'capability'='delegate' THEN t.payload#>>'{payload,peer}' ELSE t.requester END AS peer,
 CASE WHEN t.payload->>'capability'='delegate' THEN 'outgoing' ELSE 'incoming' END AS direction,
 left(r.remote_id,256) AS remote_task_id,
 (o.task_id IS NOT NULL AND EXISTS(SELECT 1 FROM agency_observation_events e WHERE e.task_id=t.id AND e.kind='request')) AS journal_available,
 left(t.contract#>>'{clarification,question}',4096) AS question,
 left(t.contract#>>'{clarification,answer}',4096) AS answer,
 left(t.contract#>>'{outcome,summary}',4096) AS result
 FROM agency_tasks t JOIN projects p ON p.id=t.project_id
 JOIN agency_mandates m ON m.id=t.mandate_id AND m.project_id=t.project_id
 LEFT JOIN agency_remote_tasks r ON r.task_id=t.id
 LEFT JOIN agency_task_observation_origins o ON o.task_id=t.id
 WHERE p.slug=$1 AND t.parent_id IS NULL
 AND m.document->>'subject_instance_id'=$4::text
 AND t.payload->>'capability' IN ('delegate','inspect','plan_prepare','plan_apply','plan_inspect','model_draft')
 AND t.requester NOT LIKE 'inbox-gateway:%'
 AND t.payload::text NOT LIKE '%"conversation_turn"%'
 AND t.payload::text NOT LIKE '%inbox-gateway:%'
 AND NOT EXISTS(SELECT 1 FROM inbox_task_origins i WHERE i.task_id=t.id)
 AND NOT EXISTS(SELECT 1 FROM inbox_dispatches d WHERE d.task_id=t.id)
 AND ((t.payload->>'capability'='delegate' AND t.payload#>>'{payload,kind}'='delegate'
       AND t.payload#>>'{payload,request,capability}' IN ('inspect','plan_prepare','plan_apply','plan_inspect','model_draft')
       AND t.payload#>>'{payload,request,capability}'=t.payload#>>'{payload,request,payload,kind}'
       AND t.payload#>>'{resource,kind}'='peer'
       AND t.payload#>>'{resource,id}'=t.payload#>>'{payload,peer}'
       AND r.peer_id=t.payload#>>'{payload,peer}' AND r.peer_id=ANY($2))
 OR (t.payload->>'capability'<>'delegate' AND t.payload->>'capability'=t.payload#>>'{payload,kind}'
       AND t.requester=ANY($2) AND r.task_id IS NULL))
 AND ((o.task_id IS NOT NULL AND o.home_instance_id::text=$4::text AND o.requester=t.requester
       AND ((o.direction='outgoing' AND o.peer=r.peer_id AND o.requester_operator AND o.requester_instance_id=o.home_instance_id)
         OR (o.direction='incoming' AND o.peer=t.requester AND NOT o.requester_operator AND o.requester_instance_id<>o.home_instance_id)))
 OR (o.task_id IS NULL AND EXISTS(SELECT 1 FROM area_import_receipts a WHERE a.singleton=true AND a.target_instance::text=$4::text)
   AND EXISTS(SELECT 1 FROM jsonb_to_recordset($3::jsonb) AS l(task_id uuid,peer text,direction text,requester text,remote_task_id text,source_task_id uuid)
    WHERE l.task_id=t.id AND l.requester=t.requester
      AND ((l.direction='outgoing' AND l.peer=r.peer_id AND l.remote_task_id=r.remote_id AND t.payload->>'capability'='delegate')
        OR (l.direction='incoming' AND l.peer=t.requester AND l.remote_task_id IS NULL AND l.source_task_id::text=t.request_id AND t.payload->>'capability'<>'delegate')))))"#;
fn safe(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| (!c.is_control() || *c == '\n' || *c == '\t') && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}'))
        .take(limit)
        .collect()
}
fn label(text: &str, limit: usize) -> String {
    safe(text, limit).replace(['\n', '\t'], " ")
}
fn summary(r: &PgRow) -> Result<ExchangeSummary, ApiError> {
    Ok(ExchangeSummary {
        task_id: r.try_get("id").map_err(database_error)?,
        peer: label(
            &r.try_get::<String, _>("peer").map_err(database_error)?,
            128,
        ),
        direction: r.try_get("direction").map_err(database_error)?,
        specialist_id: label(
            &r.try_get::<Option<String>, _>("specialist_id")
                .map_err(database_error)?
                .unwrap_or_default(),
            128,
        ),
        objective: safe(
            &r.try_get::<String, _>("objective")
                .map_err(database_error)?,
            4096,
        ),
        status: label(
            &r.try_get::<Option<String>, _>("status")
                .map_err(database_error)?
                .unwrap_or_else(|| "unknown".into()),
            64,
        ),
        created_at: r.try_get("created_at").map_err(database_error)?,
        updated_at: r.try_get("updated_at").map_err(database_error)?,
        journal_available: r.try_get("journal_available").map_err(database_error)?,
    })
}
async fn authorized(
    s: &AppState,
    p: &AuthenticatedPrincipal,
    project: &str,
) -> Result<PgPool, ApiError> {
    let c = config(s)?;
    if !exchange_access(c, p, project) {
        return Err(ApiError::forbidden(
            "Explicit project companion exchange access is required",
        ));
    }
    let pool = s.pool()?;
    provenance(&pool, c).await?;
    Ok(pool)
}
pub(crate) async fn exchanges(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path(project): Path<String>,
    Query(q): Query<PageQuery>,
) -> Result<Json<ExchangePage>, ApiError> {
    let pool = authorized(&s, &p, &project).await?;
    let c = config(&s)?;
    let limit = q.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) || q.peer.as_ref().is_some_and(|p| p.len() > 128) {
        return Err(ApiError::status_message(
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid exchange page size or peer",
        ));
    }
    let (at, id) = cursor(q.before.as_deref())?;
    let rows=sqlx::query(&format!("{SELECT} AND ($5::text IS NULL OR CASE WHEN t.payload->>'capability'='delegate' THEN r.peer_id ELSE t.requester END=$5) AND (t.created_at,t.id)<($6,$7) ORDER BY t.created_at DESC,t.id DESC LIMIT $8"))
        .bind(&project).bind(&c.peer_ids).bind(serde_json::to_value(&c.legacy_exchanges).map_err(|_|unavailable())?).bind(c.instance_id.to_string()).bind(q.peer).bind(at).bind(id).bind(limit+1).fetch_all(&pool).await.map_err(database_error)?;
    let more = rows.len() > limit as usize;
    let exchanges = rows
        .iter()
        .take(limit as usize)
        .map(summary)
        .collect::<Result<Vec<_>, _>>()?;
    let next_before = if more {
        exchanges
            .last()
            .map(|e| format!("{}|{}", e.created_at.to_rfc3339(), e.task_id))
    } else {
        None
    };
    Ok(Json(ExchangePage {
        exchanges,
        next_before,
    }))
}
pub(crate) async fn detail(
    State(s): State<AppState>,
    Extension(p): Extension<AuthenticatedPrincipal>,
    Path((project, id)): Path<(String, Uuid)>,
) -> Result<Json<ExchangeDetail>, ApiError> {
    let pool = authorized(&s, &p, &project).await?;
    let c = config(&s)?;
    let r = sqlx::query(&format!("{SELECT} AND t.id=$5"))
        .bind(&project)
        .bind(&c.peer_ids)
        .bind(serde_json::to_value(&c.legacy_exchanges).map_err(|_| unavailable())?)
        .bind(c.instance_id.to_string())
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(database_error)?
        .ok_or_else(|| ApiError::not_found("Exchange unavailable"))?;
    let exchange = summary(&r)?;
    let mut events = vec![];
    let mut truncated = false;
    if exchange.journal_available {
        let rows=sqlx::query("SELECT sequence,kind,body,recorded_at FROM agency_observation_events WHERE task_id=$1 ORDER BY sequence LIMIT 201").bind(id).fetch_all(&pool).await.map_err(database_error)?;
        truncated = rows.len() > 200;
        for r in rows.iter().take(200) {
            events.push(ExchangeEvent {
                sequence: r.try_get("sequence").map_err(database_error)?,
                kind: r.try_get("kind").map_err(database_error)?,
                body: safe(
                    &r.try_get::<String, _>("body").map_err(database_error)?,
                    4096,
                ),
                recorded_at: r.try_get("recorded_at").map_err(database_error)?,
            });
        }
    } else {
        // The old overwritten contract is a snapshot, never reconstructed history.
        for (kind, body) in [
            ("request", Some(exchange.objective.clone())),
            (
                "clarification",
                r.try_get::<Option<String>, _>("question")
                    .map_err(database_error)?,
            ),
            (
                "answer",
                r.try_get::<Option<String>, _>("answer")
                    .map_err(database_error)?,
            ),
            (
                "result",
                r.try_get::<Option<String>, _>("result")
                    .map_err(database_error)?,
            ),
        ] {
            if let Some(body) = body {
                events.push(ExchangeEvent {
                    sequence: events.len() as i64,
                    kind: "snapshot".into(),
                    body: format!("{kind}: {}", safe(&body, 4096)),
                    recorded_at: exchange.updated_at,
                });
            }
        }
    }
    Ok(Json(ExchangeDetail {
        remote_task_id: r
            .try_get::<Option<String>, _>("remote_task_id")
            .map_err(database_error)?
            .map(|s| label(&s, 256)),
        exchange,
        events,
        events_truncated: truncated,
        independently_verified: false,
    }))
}
#[cfg(test)]
mod tests;
