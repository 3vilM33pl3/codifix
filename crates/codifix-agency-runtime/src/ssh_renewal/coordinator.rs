//! Advance a named expiry alert through its host and CA companions.
use super::*;
use crate::{
    config::{Config, Peer},
    discovery::bounded,
    social_transport::{Call, Transport},
    store::Submit,
};
use chrono::{DateTime, NaiveTime};
use codifix_agency::{
    Budget, ResourceScope,
    a2a::{self, Part},
};
use serde::Deserialize;
use sqlx::Row;

#[derive(Deserialize)]
struct Coordinator {
    home_requester: String,
    home_specialist: String,
    home_mandate: Uuid,
    ca_peer: String,
    ca_specialist: String,
    ca_mandate: Uuid,
    renew_before_seconds: u32,
    deadline_seconds: u32,
}
struct Event<'a> {
    id: Uuid,
    host: &'a str,
    deadline: DateTime<Utc>,
}
impl Event<'_> {
    fn request(
        &self,
        kind: &str,
        specialist: &str,
        mandate: Uuid,
        criterion: &str,
        mut payload: Value,
    ) -> Submit {
        payload["kind"] = json!(kind);
        Submit {
            request_id: Uuid::new_v5(&self.id, kind.as_bytes()).to_string(),
            mandate_id: mandate,
            specialist_id: specialist.into(),
            capability: kind.into(),
            objective: format!("SSH host renewal for {}: {kind}", self.host),
            resource: ResourceScope {
                kind: "ssh_host".into(),
                id: self.host.into(),
            },
            deadline: self.deadline,
            acceptance_criteria: vec![criterion.into()],
            budget: Budget {
                micro_usd: 0,
                calls: 0,
                turns: 4,
                delegation_depth: 0,
            },
            payload,
            parent_id: None,
        }
    }
}

async fn local(store: &Store, peer: &Peer, req: Submit) -> Result<(TaskStatus, Value)> {
    let task = store.submit(&req, peer).await?;
    let evidence: Option<Value> =
        sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_optional(&store.pool)
            .await?;
    Ok((task.contract.status, evidence.unwrap_or(Value::Null)))
}

pub async fn advance(store: &Store, cfg: &Config, host: &str, event_id: Uuid) -> Result<Value> {
    let profile = super::profile(host)?;
    let options: Coordinator = serde_json::from_value(profile["coordinator"].clone())?;
    ensure!(
        options.deadline_seconds > 0
            && options.deadline_seconds <= 604800
            && options.renew_before_seconds > 0
            && !event_id.is_nil(),
        "invalid expiry alert"
    );
    let requester = cfg
        .peers
        .iter()
        .find(|p| p.id == options.home_requester && p.instance_id == cfg.instance_id && !p.operator)
        .context("local scoped requester missing")?;
    let ca = cfg
        .peers
        .iter()
        .find(|p| p.id == options.ca_peer && !p.operator && p.instance_id != cfg.instance_id)
        .context("CA peer missing")?;
    ensure!(
        cfg.social
            .as_ref()
            .is_some_and(|s| s.peers.contains_key(&ca.id)),
        "CA requires a private DID-bound peer"
    );
    ensure!(
        cfg.specialists
            .iter()
            .any(|s| s.specialist_id == options.home_specialist
                && ["ssh_host_inspect", "ssh_host_install"]
                    .iter()
                    .all(|c| s.capabilities.contains(*c))),
        "local renewal specialist missing"
    );
    sqlx::query("INSERT INTO agency_ssh_renewal_events(instance_id,event_id,host) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(cfg.instance_id).bind(event_id).bind(host).execute(&store.pool).await?;
    let row=sqlx::query("SELECT host,started_at,ca_task_id FROM agency_ssh_renewal_events WHERE instance_id=$1 AND event_id=$2").bind(cfg.instance_id).bind(event_id).fetch_one(&store.pool).await?;
    ensure!(
        row.try_get::<String, _>("host")? == host,
        "event belongs to a different host"
    );
    // Reopen the finished alert even after its maintenance window, task
    // deadline, or originating PDS session has expired.
    if let Some(ca_id) = row.try_get::<Option<Uuid>, _>("ca_task_id")? {
        let install_id = Uuid::new_v5(&event_id, b"ssh_host_install").to_string();
        let receipt:Option<(String,Value)>=sqlx::query_as("SELECT t.contract->>'status',o.evidence FROM agency_tasks t JOIN agency_outcomes o ON o.task_id=t.id WHERE t.requester=$1 AND t.request_id=$2 AND t.contract->>'status' IN ('completed','failed','rejected')")
            .bind(&requester.id).bind(install_id).fetch_optional(&store.pool).await?;
        if let Some((status, evidence)) = receipt {
            return Ok(
                json!({"phase":"installation","status":status,"ca_task":ca_id,"evidence":evidence}),
            );
        }
    }
    let event = Event {
        id: event_id,
        host,
        deadline: row.try_get::<DateTime<Utc>, _>("started_at")?
            + chrono::Duration::seconds(i64::from(options.deadline_seconds)),
    };
    let (state, observed) = local(
        store,
        requester,
        event.request(
            "ssh_host_inspect",
            &options.home_specialist,
            options.home_mandate,
            "served_host_certificate_observed",
            json!({}),
        ),
    )
    .await?;
    if state != TaskStatus::Completed {
        return Ok(json!({"phase":"inspection","status":state,"evidence":observed}));
    }
    let expiry = observed["current_expiry"]
        .as_i64()
        .context("missing observed expiry")?;
    if expiry
        > row.try_get::<DateTime<Utc>, _>("started_at")?.timestamp()
            + i64::from(options.renew_before_seconds)
    {
        return Ok(json!({"phase":"inspection","status":"not_due","expiry":expiry}));
    }
    let transport = Transport::new(store, cfg, ca)?;
    let remote: a2a::Task = if let Some(id) = row.try_get::<Option<Uuid>, _>("ca_task_id")? {
        let task: a2a::Task = serde_json::from_value(
            bounded(transport.call(Call::Status(&id.to_string())).await?).await?,
        )?;
        ensure!(task.id == id.to_string(), "CA task mismatch");
        task
    } else {
        let req = event.request(
            "ssh_host_sign",
            &options.ca_specialist,
            options.ca_mandate,
            "approved_certificate_handed_off",
            json!({"request":observed}),
        );
        let message = json!({"message":{"role":"ROLE_USER","messageId":req.request_id,"parts":[{"data":req}]},"configuration":{"returnImmediately":true}});
        let response: a2a::SendMessageResponse =
            serde_json::from_value(bounded(transport.call(Call::Send(&message)).await?).await?)?;
        let a2a::SendMessageResponse::Task(task) = response else {
            anyhow::bail!("missing durable CA task")
        };
        let id: Uuid = task.id.parse()?;
        ensure!(sqlx::query("UPDATE agency_ssh_renewal_events SET ca_task_id=$3 WHERE instance_id=$1 AND event_id=$2 AND (ca_task_id IS NULL OR ca_task_id=$3)").bind(cfg.instance_id).bind(event_id).bind(id).execute(&store.pool).await?.rows_affected()==1,"CA task changed");
        task
    };
    remote.validate()?;
    let evidence = remote
        .artifacts
        .iter()
        .flat_map(|a| &a.parts)
        .find_map(|part| {
            if let Part::Data(value) = part {
                Some(value)
            } else {
                None
            }
        });
    if remote.status.state != a2a::TaskState::Completed {
        return Ok(
            json!({"phase":"signing","status":remote.status.state,"ca_task":remote.id,"evidence":evidence}),
        );
    }
    let window: [String; 2] = serde_json::from_value(profile["window_utc"].clone())?;
    let start = NaiveTime::parse_from_str(&window[0], "%H:%M")?;
    let end = NaiveTime::parse_from_str(&window[1], "%H:%M")?;
    let now = NaiveTime::parse_from_str(&Utc::now().format("%H:%M").to_string(), "%H:%M")?;
    if !(if start <= end {
        start <= now && now <= end
    } else {
        now >= start || now <= end
    }) {
        return Ok(
            json!({"phase":"installation","status":"waiting_for_window","ca_task":remote.id}),
        );
    }
    let (status, evidence) = local(
        store,
        requester,
        event.request(
            "ssh_host_install",
            &options.home_specialist,
            options.home_mandate,
            "fresh_ssh_login_verified",
            json!({"renewal_id":remote.id}),
        ),
    )
    .await?;
    Ok(json!({"phase":"installation","status":status,"ca_task":remote.id,"evidence":evidence}))
}
