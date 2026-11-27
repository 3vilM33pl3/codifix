//! Correlate service reports, apply one reviewed runbook, and verify a user request.
mod delivery;
use crate::{
    config::Config,
    store::{Store, StoredTask},
};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use codifix_agency::Usage;
use codifix_store::effects::{self, EffectIntent, EffectKind, EffectReason, EffectStatus};
pub(crate) use delivery::deliver_once;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub alert_peer: String,
    pub specialist: String,
    pub mandate_id: Uuid,
    pub services: BTreeMap<String, Service>,
    #[serde(default)]
    notify: Vec<String>,
    inbox: Option<IncidentInbox>,

    linear: Linear,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IncidentInbox {
    operator_conversation: Option<Uuid>,
    operator_peer: Option<String>,
    #[serde(default)]
    reporting_peers: std::collections::BTreeSet<String>,
}
#[derive(Deserialize)]
struct Linear {
    peer: String,
    specialist: String,
    mandate_id: Uuid,
    project_id: Uuid,
}
#[derive(Deserialize, Serialize)]
pub(crate) struct Service {
    inspect: Vec<String>,
    health: Vec<String>,
    user_request: Vec<String>,
    repairs: BTreeMap<String, Repair>,
    runbook: PathBuf,
    postmortem_directory: PathBuf,
}
#[derive(Deserialize, Serialize)]
struct Repair {
    condition: Vec<String>,
    command: Vec<String>,
    autonomous: bool,
    action: String,
    risks: String,
    recovery_options: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Investigation {
    impact: String,
    logs: String,
    metrics: String,
    recent_changes: String,
    cause: String,
    repair: String,
    #[serde(default)]
    runbook_note: String,
    #[serde(default)]
    followups: Vec<String>,
}
pub(crate) fn profile() -> Result<Option<Profile>> {
    let Some(path) = std::env::var_os("CODIFIX_INCIDENT_PROFILE") else {
        return Ok(None);
    };
    let bytes = std::fs::read(path)?;
    ensure!(bytes.len() <= 131072, "incident profile too large");
    let profile: Profile = serde_json::from_slice(&bytes)?;
    if let Some(inbox) = &profile.inbox {
        ensure!(
            inbox.operator_conversation.is_some() || inbox.operator_peer.is_some(),
            "enroll an operator incident Inbox"
        );
    }
    ensure!(
        profile.inbox.is_some() || !profile.notify.is_empty(),
        "configure an incident notification recipient"
    );
    Ok(Some(profile))
}

// All argv entries originate in operator-owned configuration. Request text is
// never interpolated into a process argument or shell.
async fn command(args: &[String], input: &Value) -> Result<(bool, Vec<u8>)> {
    let (exe, args) = args.split_first().context("empty runbook command")?;
    ensure!(
        Path::new(exe).is_absolute(),
        "runbook commands require absolute executable paths"
    );
    let mut child = Command::new(exe)
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().context("missing command input")?;
    let stdin_bytes = serde_json::to_vec(input)?;
    let mut stdout = child
        .stdout
        .take()
        .context("missing command output")?
        .take(65537);
    let mut bytes = Vec::new();
    let status = tokio::time::timeout(Duration::from_secs(30), async {
        let write = async {
            if let Err(e) = stdin.write_all(&stdin_bytes).await
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(e);
            }
            drop(stdin);
            Ok(())
        };
        tokio::try_join!(write, async {
            stdout.read_to_end(&mut bytes).await.map(|_| ())
        })?;
        anyhow::ensure!(bytes.len() <= 65536, "runbook output exceeded limit");
        Ok::<_, anyhow::Error>(child.wait().await?)
    })
    .await
    .context("runbook command timed out")??;
    Ok((status.success(), bytes))
}
async fn check(args: &[String]) -> bool {
    command(args, &Value::Null).await.is_ok_and(|(ok, _)| ok)
}

async fn event(store: &Store, id: Uuid, key: &str, body: Value) -> Result<()> {
    sqlx::query("INSERT INTO agency_incident_events(incident_id,event_id,body) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(id).bind(key).bind(body).execute(&store.pool).await?;
    Ok(())
}
async fn state(store: &Store, id: Uuid, value: &str) -> Result<()> {
    sqlx::query("UPDATE agency_incidents SET state=$2 WHERE id=$1")
        .bind(id)
        .bind(value)
        .execute(&store.pool)
        .await?;
    Ok(())
}
async fn blocked(
    store: &Store,
    task: &StoredTask,
    id: Uuid,
    key: &str,
    reason: &str,
) -> Result<Option<Value>> {
    state(store, id, key).await?;
    event(store, id, key, json!({"state":key,"details":reason})).await?;
    let mut task = task.clone();
    task.contract.clarify(reason.into(), Utc::now())?;
    store.update_contract(&task).await?;
    Ok(None)
}
pub(crate) async fn report(
    store: &Store,
    cfg: &Config,
    task: &StoredTask,
) -> Result<Option<Value>> {
    let profile = profile()?.context("incident response not configured")?;
    let service = &task.request.resource.id;
    ensure!(
        task.request.resource.kind == "service"
            && task.contract.specialist_id == profile.specialist
            && task.request.mandate_id == profile.mandate_id,
        "incident scope mismatch"
    );
    let rule = profile
        .services
        .get(service)
        .context("unknown monitored service")?;
    let started: DateTime<Utc> = serde_json::from_value(task.request.payload["starts_at"].clone())?;
    ensure!(
        started <= Utc::now() + chrono::Duration::minutes(2),
        "future outage report"
    );
    let summary = task.request.payload["summary"]
        .as_str()
        .context("missing impact summary")?;
    ensure!(
        !summary.is_empty() && summary.len() <= 2048,
        "invalid report"
    );
    let lock = Uuid::new_v5(&cfg.instance_id, service.as_bytes());
    let mut tx = store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock.to_string())
        .execute(&mut *tx)
        .await?;
    let previous: Option<Uuid> =
        sqlx::query_scalar("SELECT incident_id FROM agency_incident_reports WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_optional(&mut *tx)
            .await?;
    let id = if let Some(id) = previous {
        id
    } else {
        let existing:Option<Uuid>=sqlx::query_scalar("SELECT id FROM agency_incidents WHERE instance_id=$1 AND service=$2 AND (closed_at IS NULL OR (opened_at<=$3 AND closed_at>=$3)) ORDER BY opened_at DESC LIMIT 1")
            .bind(cfg.instance_id).bind(service).bind(started).fetch_optional(&mut *tx).await?;
        let id = existing.unwrap_or(task.contract.id);
        if existing.is_none() {
            sqlx::query("INSERT INTO agency_incidents(id,instance_id,service,task_id,opened_at,state) VALUES($1,$2,$3,$1,$4,'reported')")
                .bind(id).bind(cfg.instance_id).bind(service).bind(started).execute(&mut *tx).await?;
        }
        sqlx::query(
            "INSERT INTO agency_incident_reports(task_id,incident_id,reporter) VALUES($1,$2,$3)",
        )
        .bind(task.contract.id)
        .bind(id)
        .bind(&task.requester)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO agency_incident_events(incident_id,event_id,body) VALUES($1,$2,$3)",
        )
        .bind(id)
        .bind(format!("report:{}", task.contract.id))
        .bind(json!({"state":"acknowledged","impact":summary,"source":task.requester}))
        .execute(&mut *tx)
        .await?;
        id
    };
    tx.commit().await?;
    let row = sqlx::query("SELECT * FROM agency_incidents WHERE id=$1")
        .bind(id)
        .fetch_one(&store.pool)
        .await?;
    let phase: String = row.try_get("state")?;
    if row.try_get::<Uuid, _>("task_id")? != task.contract.id || phase == "recovered" {
        return Ok(Some(
            json!({"incident_id":id,"state":phase,"service":service}),
        ));
    }
    let evidence: Investigation =
        if let Some(stored) = row.try_get::<Option<Value>, _>("investigation")? {
            serde_json::from_value(stored)?
        } else {
            let (ok, bytes) = command(&rule.inspect, &json!({"service":service})).await?;
            ensure!(ok, "incident investigation failed");
            let observation: Investigation = serde_json::from_slice(&bytes)?;
            ensure!(
                [
                    &observation.impact,
                    &observation.logs,
                    &observation.metrics,
                    &observation.recent_changes,
                    &observation.cause
                ]
                .into_iter()
                .all(|s| !s.trim().is_empty() && s.len() <= 2048)
                    && observation.followups.len() <= 8
                    && observation
                        .followups
                        .iter()
                        .all(|title| !title.trim().is_empty() && title.len() <= 250),
                "incomplete investigation"
            );
            sqlx::query(
                "UPDATE agency_incidents SET state='investigated',investigation=$2 WHERE id=$1",
            )
            .bind(id)
            .bind(serde_json::to_value(&observation)?)
            .execute(&store.pool)
            .await?;
            event(
                store,
                id,
                "investigation",
                json!({"state":"investigated","evidence":observation}),
            )
            .await?;
            observation
        };
    if phase == "repairing" || phase == "failed" {
        if !(check(&rule.health).await && check(&rule.user_request).await) {
            return blocked(store,task,id,"failed","Repair already attempted. The service or user request still fails. Operator intervention is required.").await;
        }
    } else if !(check(&rule.health).await && check(&rule.user_request).await) {
        let Some(repair) = rule.repairs.get(&evidence.repair) else {
            return blocked(
                store,
                task,
                id,
                "unclassified",
                "No reviewed repair matches this outage. Update the runbook or repair it manually.",
            )
            .await;
        };
        if !check(&repair.condition).await {
            return blocked(store,task,id,"condition_failed","The configured repair condition did not pass. Investigate the service before intervention.").await;
        }
        let digest = crate::store::fingerprint(&(id, service, repair, &evidence))?;
        if !repair.autonomous {
            let response = format!("approve {digest}");
            let question = format!(
                "Incident {id}. Impact: {}. Proposed action: {}. Risks: {}. Recovery options: {}. Reply with `{response}`.",
                evidence.impact, repair.action, repair.risks, repair.recovery_options
            );
            let authorized =
                task.contract.clarification.as_ref().is_some_and(|q| {
                    q.question == question && q.answer.as_deref() == Some(&response)
                });
            let operators: Vec<_> = cfg
                .peers
                .iter()
                .filter(|p| p.operator && p.instance_id == cfg.instance_id)
                .map(|p| p.id.clone())
                .collect();
            let approver = if authorized {
                sqlx::query_scalar::<_,String>("SELECT sender FROM agency_messages WHERE task_id=$1 AND sender=ANY($2) AND body->>'answer'=$3 LIMIT 1")
                    .bind(task.contract.id).bind(&operators).bind(response).fetch_optional(&store.pool).await?
            } else {
                None
            };
            let Some(approver) = approver else {
                return blocked(store, task, id, "waiting_for_operator", &question).await;
            };
            event(
                store,
                id,
                "approved",
                json!({"state":"approved","operator":approver,"action":repair.action}),
            )
            .await?;
        }
        let effect = Uuid::new_v5(&id, b"approved-repair");
        store
            .admit_effect(
                task.contract.id,
                task.revision,
                Usage {
                    calls: 1,
                    ..Default::default()
                },
                &EffectIntent {
                    id: effect,
                    project_id: Some(task.project_id),
                    kind: EffectKind::ProviderRequest,
                    host_id: cfg.instance_id.to_string(),
                    resource: format!("service:{service}"),
                    input_fingerprint: digest,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-incident".into(),
                },
            )
            .await?;
        state(store, id, "repairing").await?;
        event(
            store,
            id,
            "repair",
            json!({"state":"repairing","action":repair.action}),
        )
        .await?;
        if !command(&repair.command, &json!({"incident_id":id}))
            .await
            .is_ok_and(|(ok, _)| ok)
        {
            return blocked(store,task,id,"failed","The selected repair failed. See the incident timeline and inspect the service before another intervention.").await;
        }
        effects::conclude(
            &store.pool,
            effect,
            Uuid::new_v4(),
            EffectStatus::Confirmed,
            EffectReason::Observed,
        )
        .await?;
    }
    let health = check(&rule.health).await;
    let user_request = check(&rule.user_request).await;
    let checks = json!({"health":health,"user_request":user_request});
    sqlx::query("UPDATE agency_incidents SET verified=$2 WHERE id=$1")
        .bind(id)
        .bind(&checks)
        .execute(&store.pool)
        .await?;
    if !health || !user_request {
        return blocked(store,task,id,"failed",&format!("Recovery verification failed: health={health}, representative request={user_request}. Operator assistance is required.")).await;
    }
    let major = rule
        .repairs
        .get(&evidence.repair)
        .is_some_and(|repair| !repair.autonomous);
    close_notes(store, rule, id, service, &evidence, major).await?;
    event(store,id,"recovered",json!({"state":"recovered","service":service,"evidence":checks,"impact":evidence.impact,"cause":evidence.cause})).await?;
    for (i, item) in evidence.followups.iter().enumerate() {
        event(
            store,
            id,
            &format!("followup:{i}"),
            json!({"state":"followup","title":item}),
        )
        .await?;
    }
    sqlx::query(
        "UPDATE agency_incidents SET state='recovered',closed_at=clock_timestamp() WHERE id=$1",
    )
    .bind(id)
    .execute(&store.pool)
    .await?;
    Ok(Some(
        json!({"incident_id":id,"service":service,"state":"recovered","verified":checks}),
    ))
}

fn event_summary(body: &Value) -> String {
    let mut fields = vec![body["state"].as_str().unwrap_or("update").to_owned()];
    for field in [
        "source", "impact", "details", "action", "cause", "title", "operator",
    ] {
        if let Some(value) = body[field].as_str() {
            fields.push(value.to_owned());
        }
    }
    if let Some(evidence) = body.get("evidence") {
        fields.push(evidence.to_string());
    }
    fields.join(" · ").chars().take(160).collect()
}

async fn close_notes(
    store: &Store,
    rule: &Service,
    id: Uuid,
    service: &str,
    e: &Investigation,
    major: bool,
) -> Result<()> {
    if !e.runbook_note.trim().is_empty() {
        let tag = format!("<!-- incident:{id} -->");
        let text = tokio::fs::read_to_string(&rule.runbook).await?;
        if !text.contains(&tag) {
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&rule.runbook)
                .await?;
            file.write_all(format!("\n{tag}\n## Incident {id}\n\n{}\n", e.runbook_note).as_bytes())
                .await?;
        }
    }
    if major {
        let rows: Vec<(DateTime<Utc>, Value)> = sqlx::query_as(
            "SELECT at,body FROM agency_incident_events WHERE incident_id=$1 ORDER BY seq",
        )
        .bind(id)
        .fetch_all(&store.pool)
        .await?;
        let mut text = format!(
            "# {service} incident {id}\n\n## Impact\n\n{}\n\n## Cause or remaining uncertainty\n\n{}\n\n## Timeline\n\n",
            e.impact, e.cause
        );
        for (at, body) in rows {
            text.push_str(&format!("- {at} — {}\n", event_summary(&body)));
        }
        text.push_str(&format!(
            "- {} — service health and representative request passed\n\n## Follow-up\n\n",
            Utc::now()
        ));
        for followup in &e.followups {
            text.push_str(&format!("- {followup}\n"));
        }
        tokio::fs::create_dir_all(&rule.postmortem_directory).await?;
        tokio::fs::write(rule.postmortem_directory.join(format!("{id}.md")), text).await?;
    }
    Ok(())
}
