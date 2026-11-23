use super::*;
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

pub(super) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/incidents/alertmanager", post(alerts))
        .route("/v1/incidents/{id}", get(status))
}
#[derive(Deserialize)]
struct Alerts {
    alerts: Vec<Alert>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Alert {
    status: String,
    fingerprint: String,
    starts_at: DateTime<Utc>,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
}
async fn alerts(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Alerts>,
) -> ApiResult<Value> {
    let caller = peer(&app, &headers)?;
    let cfg = crate::incidents::profile()
        .map_err(safe_error)?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "disabled"))?;
    if caller.operator || caller.id != cfg.alert_peer || input.alerts.len() > 64 {
        return Err(failure(StatusCode::FORBIDDEN, "alert_source_denied"));
    }
    let mut ids = Vec::new();
    for alert in input.alerts {
        if alert.status == "resolved" {
            continue;
        }
        let service = alert
            .labels
            .get("service")
            .ok_or_else(|| failure(StatusCode::BAD_REQUEST, "service_required"))?;
        if alert.status != "firing"
            || !cfg.services.contains_key(service)
            || alert.fingerprint.is_empty()
            || alert.fingerprint.len() > 128
        {
            return Err(failure(StatusCode::BAD_REQUEST, "invalid_alert"));
        }
        let key = Uuid::new_v5(
            &app.config.instance_id,
            format!("{service}:{}:{}", alert.fingerprint, alert.starts_at).as_bytes(),
        );
        // A repeat can arrive days after the first alert; return its stable
        // receipt without refreshing authority or issuing another repair.
        if let Some(id) = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM agency_tasks WHERE requester=$1 AND request_id=$2",
        )
        .bind(&caller.id)
        .bind(key.to_string())
        .fetch_optional(&app.store.pool)
        .await
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "storage_unavailable"))?
        {
            ids.push(id);
            continue;
        }
        let req = Submit {
            request_id: key.to_string(),
            mandate_id: cfg.mandate_id,
            specialist_id: cfg.specialist.clone(),
            objective: format!("Investigate outage of {service}"),
            acceptance_criteria: vec!["incident_report_processed".into()],
            capability: "incident_report".into(),
            resource: codifix_agency::ResourceScope {
                kind: "service".into(),
                id: service.clone(),
            },
            deadline: alert.starts_at + chrono::Duration::days(6),
            budget: codifix_agency::Budget {
                calls: 16,
                turns: 12,
                micro_usd: 0,
                delegation_depth: 0,
            },
            payload: json!({"kind":"incident_report","starts_at":alert.starts_at,"summary":alert.annotations.get("summary").or_else(||alert.labels.get("alertname")).unwrap_or(service)}),
            parent_id: None,
        };
        check_specialist(&app, &req)?;
        ids.push(
            app.store
                .submit(&req, &caller)
                .await
                .map_err(safe_error)?
                .contract
                .id,
        );
    }
    Ok(Json(json!({"tasks":ids})))
}
async fn status(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Value> {
    let p = peer(&app, &headers)?;
    let row=sqlx::query("SELECT i.state,i.service,i.ticket,i.verified,i.task_id FROM agency_incidents i WHERE i.id=$1 AND i.instance_id=$2 AND ($3 OR EXISTS(SELECT 1 FROM agency_incident_reports r WHERE r.incident_id=i.id AND r.reporter=$4))")
        .bind(id).bind(app.config.instance_id).bind(p.operator).bind(&p.id).fetch_optional(&app.store.pool).await.map_err(|_|failure(StatusCode::SERVICE_UNAVAILABLE,"storage_unavailable"))?.ok_or_else(||failure(StatusCode::NOT_FOUND,"unknown_incident"))?;
    let history:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('at',at,'event',body,'delivery_problem',problem) FROM agency_incident_events WHERE incident_id=$1 ORDER BY seq").bind(id).fetch_all(&app.store.pool).await.map_err(|_|failure(StatusCode::SERVICE_UNAVAILABLE,"storage_unavailable"))?;
    Ok(Json(
        json!({"id":id,"service":row.get::<String,_>("service"),"state":row.get::<String,_>("state"),"task_id":row.get::<Uuid,_>("task_id"),"verified":row.get::<Option<Value>,_>("verified"),"ticket":row.get::<Option<Value>,_>("ticket"),"events":history}),
    ))
}
