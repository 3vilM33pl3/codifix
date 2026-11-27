use super::*;

pub(super) const WATCHING: &str = "watching_manual_recovery";

pub(super) async fn watch(store: &Store, id: Uuid, service: &str) -> Result<Value> {
    let mut tx = store.pool.begin().await?;
    let prior_repair = Uuid::new_v5(&id, b"approved-repair");
    ensure!(
        sqlx::query("UPDATE agency_incidents SET state=$2 WHERE id=$1 AND closed_at IS NULL
            AND state IN ('investigated',$2) AND NOT EXISTS (SELECT 1 FROM external_effects WHERE id=$3)")
            .bind(id).bind(WATCHING).bind(prior_repair).execute(&mut *tx).await?.rows_affected() == 1,
        "incident already has an automated repair history"
    );
    sqlx::query("INSERT INTO agency_incident_events(incident_id,event_id,body) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(id).bind(WATCHING)
        .bind(json!({"state":WATCHING,"details":"Awaiting manual recovery. Homelab will check service health and a representative user request. No automated repair is enabled.","automated_repair":false}))
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"incident_id":id,"state":WATCHING,"service":service,"automated_repair":false}))
}

pub(crate) async fn verify_once(
    store: &Store,
    cfg: &Config,
    cursor: Option<Uuid>,
) -> Result<Option<Uuid>> {
    let Some(profile) = profile()? else {
        return Ok(None);
    };
    let monitored: Vec<&str> = profile
        .services
        .iter()
        .filter(|(_, rule)| rule.manual_recovery)
        .map(|(service, _)| service.as_str())
        .collect();
    if monitored.is_empty() {
        return Ok(None);
    }
    let next: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT incident.id,incident.service FROM agency_incidents incident
         JOIN agency_tasks task ON task.id=incident.task_id
         WHERE incident.instance_id=$1 AND incident.service=ANY($2)
           AND incident.state=$3 AND incident.closed_at IS NULL
           AND task.mandate_id=$4 AND task.contract->>'specialist_id'=$5
           AND task.contract->>'status'='completed'
         ORDER BY COALESCE(incident.id>$6,true) DESC,incident.id LIMIT 1",
    )
    .bind(cfg.instance_id)
    .bind(monitored)
    .bind(WATCHING)
    .bind(profile.mandate_id)
    .bind(&profile.specialist)
    .bind(cursor)
    .fetch_optional(&store.pool)
    .await?;
    let Some((id, service)) = next else {
        return Ok(None);
    };
    let Some(rule) = profile.services.get(&service) else {
        return Ok(Some(id));
    };
    let mut tx = store.pool.begin().await?;
    if crate::store::mandate_tx(&mut tx, profile.mandate_id)
        .await
        .is_err()
    {
        return Ok(Some(id));
    }
    tx.commit().await?;
    let (health, user_request) = tokio::join!(check(&rule.health), check(&rule.user_request));
    if !health || !user_request {
        return Ok(Some(id));
    }
    let checks = json!({"health":health,"user_request":user_request});
    let mut tx = store.pool.begin().await?;
    if crate::store::mandate_tx(&mut tx, profile.mandate_id)
        .await
        .is_err()
    {
        return Ok(Some(id));
    }
    let changed = sqlx::query(
        "UPDATE agency_incidents SET state='recovered',closed_at=clock_timestamp(),verified=$3
        WHERE id=$1 AND instance_id=$2 AND state=$4 AND closed_at IS NULL",
    )
    .bind(id)
    .bind(cfg.instance_id)
    .bind(&checks)
    .bind(WATCHING)
    .execute(&mut *tx)
    .await?;
    if changed.rows_affected() == 1 {
        sqlx::query("INSERT INTO agency_incident_events(incident_id,event_id,body) VALUES($1,'recovered',$2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(json!({"state":"recovered","service":service,"evidence":checks,"automated_repair":false,"details":"Verified service recovery. The companion only ran read-only probes."}))
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Some(id))
}
