use super::*;
use crate::{
    discovery::bounded,
    social_transport::{Call, Transport},
    store::Submit,
};
use codifix_agency::{Budget, ResourceScope};
use sqlx::postgres::PgRow;

pub(crate) async fn deliver_once(store: &Store, cfg: &Config) -> Result<bool> {
    let Some(profile) = profile()? else {
        return Ok(false);
    };
    // Preserve event ordering within an incident. An offline Linear peer must
    // not prevent progress notifications from reaching the operator.
    let row=sqlx::query("SELECT e.*,i.service,i.ticket,i.task_id FROM agency_incident_events e JOIN agency_incidents i ON i.id=e.incident_id WHERE i.instance_id=$1 AND (NOT e.notified OR NOT e.tracked) AND e.retry_at<=clock_timestamp() ORDER BY e.retry_at,e.seq LIMIT 1")
        .bind(cfg.instance_id).fetch_optional(&store.pool).await?;
    let Some(row) = row else { return Ok(false) };
    let id: Uuid = row.try_get("incident_id")?;
    let _guard =
        codifix_store::executions::Execution::acquire(&store.pool, Uuid::new_v5(&id, b"delivery"))
            .await?;
    let seq: i64 = row.try_get("seq")?;
    let (mut notified, mut tracked): (bool, bool) =
        sqlx::query_as("SELECT notified,tracked FROM agency_incident_events WHERE seq=$1")
            .bind(seq)
            .fetch_one(&store.pool)
            .await?;
    if !notified {
        let participants:Vec<String>=sqlx::query_scalar("SELECT DISTINCT reporter FROM agency_incident_reports WHERE incident_id=$1 ORDER BY reporter").bind(id).fetch_all(&store.pool).await?;
        let update: Value = row.try_get("body")?;
        notified = profile.inbox.is_some() || !profile.notify.is_empty();
        if let Some(inbox) = &profile.inbox {
            let state = update["state"].as_str().context("unknown incident event")?;
            // The watch follows investigation. Keep its notice interoperable with
            // receivers that do not recognize the new internal incident state.
            let notice_state = if state == manual::WATCHING {
                "investigated"
            } else {
                state
            };
            let notice = crate::notice::IncidentNotice {
                incident_id: id,
                event_id: seq,
                service: row.try_get("service")?,
                state: notice_state.to_owned(),
                summary: update["details"]
                    .as_str()
                    .or_else(|| update["impact"].as_str())
                    .or_else(|| update["action"].as_str())
                    .or_else(|| update["title"].as_str())
                    .or_else(|| update.pointer("/evidence/cause").and_then(Value::as_str))
                    .unwrap_or("Service status updated.")
                    .chars()
                    .take(500)
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            };
            if let Some(chat) = inbox.operator_conversation {
                notified &= cfg.inbox_enabled
                    && notice
                        .receive(store, cfg.instance_id, chat, &cfg.instance_id.to_string())
                        .await
                        .is_ok();
            }
            let mut recipients: std::collections::BTreeSet<_> = participants
                .iter()
                .filter(|p| inbox.reporting_peers.contains(*p))
                .collect();
            if let Some(peer) = &inbox.operator_peer {
                recipients.insert(peer);
            }
            for peer in recipients {
                notified &= notice.send(store, cfg, peer).await.is_ok();
            }
        }
        if !profile.notify.is_empty() {
            let body = json!({"incident_id":id,"task_id":row.try_get::<Uuid,_>("task_id")?,"event_id":seq,"service":row.try_get::<String,_>("service")?,"reporters":participants,"update":update});
            notified &= command(&profile.notify, &body)
                .await
                .is_ok_and(|(ok, _)| ok);
        }
    }
    let earlier:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agency_incident_events WHERE incident_id=$1 AND seq<$2 AND NOT tracked)").bind(id).bind(seq).fetch_one(&store.pool).await?;
    if !tracked && !earlier {
        tracked = sync_ticket(store, cfg, &profile, &row)
            .await
            .unwrap_or(false);
    }
    let problem = if !notified {
        Some("notification pending")
    } else if !tracked {
        Some("Linear update pending")
    } else {
        None
    };
    sqlx::query("UPDATE agency_incident_events SET notified=$2,tracked=$3,problem=$4,retry_at=clock_timestamp()+interval '2 seconds' WHERE seq=$1")
        .bind(seq).bind(notified).bind(tracked).bind(problem).execute(&store.pool).await?;
    Ok(true)
}

async fn sync_ticket(store: &Store, cfg: &Config, profile: &Profile, row: &PgRow) -> Result<bool> {
    let id: Uuid = row.try_get("incident_id")?;
    let seq: i64 = row.try_get("seq")?;
    let service: String = row.try_get("service")?;
    let key: String = row.try_get("event_id")?;
    let at: DateTime<Utc> = row.try_get("at")?;
    let body: Value = row.try_get("body")?;
    let linear = &profile.linear;
    let ticket: Option<Value> =
        sqlx::query_scalar("SELECT ticket FROM agency_incidents WHERE id=$1")
            .bind(id)
            .fetch_one(&store.pool)
            .await?;
    let followup = key.starts_with("followup:");
    let (title, description) = if followup {
        let title = body["title"].as_str().context("missing followup")?;
        (
            format!("{service}: {title}"),
            format!(
                "{title}\n\nFollow-up to incident {id}: {}",
                ticket
                    .as_ref()
                    .and_then(|t| t["url"].as_str())
                    .context("incident ticket not available")?
            ),
        )
    } else {
        let history:Vec<(DateTime<Utc>,Value)>=sqlx::query_as("SELECT at,body FROM (SELECT at,body,seq FROM agency_incident_events WHERE incident_id=$1 AND seq<=$2 ORDER BY seq DESC LIMIT 20) recent ORDER BY seq").bind(id).bind(seq).fetch_all(&store.pool).await?;
        let mut detail = format!("Incident {id}\n\n");
        for (time, entry) in history {
            detail.push_str(&format!("{time} — {}\n", event_summary(&entry)));
        }
        (format!("Incident {service} {id}"), detail.trim_end().into())
    };
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == linear.peer && !p.operator && p.instance_id != cfg.instance_id)
        .context("missing Linear peer")?;
    let transport = Transport::new(store, cfg, peer)?;
    // Keep the existing issue identity and give changed timeline wording its
    // own task key. Already tracked events are never submitted again.
    let key = format!("incident-{id}-{seq}-summary-v2");
    let payload =
        json!({"kind":"linear_track","title":title,"description":description,"lookup_title":title});
    let request = Submit {
        request_id: key.clone(),
        mandate_id: linear.mandate_id,
        specialist_id: linear.specialist.clone(),
        objective: "Maintain the incident record".into(),
        acceptance_criteria: vec!["linear_issue_readback_verified".into()],
        capability: "linear_track".into(),
        resource: ResourceScope {
            kind: "linear_project".into(),
            id: linear.project_id.to_string(),
        },
        deadline: at + chrono::Duration::days(6),
        budget: Budget {
            micro_usd: 0,
            calls: 32,
            turns: 8,
            delegation_depth: 0,
        },
        payload,
        parent_id: None,
    };
    let envelope = json!({"message":{"messageId":key,"role":"ROLE_USER","parts":[{"data":request}]},"configuration":{"returnImmediately":true}});
    let response = bounded(transport.call(Call::Send(&envelope)).await?).await?;
    let remote = response["task"]["id"]
        .as_str()
        .context("missing Linear task ID")?;
    let task = bounded(transport.call(Call::Status(remote)).await?).await?;
    match task["status"]["state"].as_str() {
        Some("TASK_STATE_INPUT_REQUIRED") => {
            let q = task
                .pointer("/status/message/parts/0/text")
                .and_then(Value::as_str)
                .context("missing Linear question")?;
            let rest = q
                .strip_prefix("Proposed Linear issue ")
                .context("Linear requires manual clarification")?;
            let (issue, rest) = rest.split_once("\n\n").context("missing issue preview")?;
            Uuid::parse_str(issue)?;
            let suffix = rest
                .strip_prefix(&format!("{title}\n\n{description}\n\nReply with `confirm "))
                .context("Linear proposed different wording")?;
            let digest = suffix
                .strip_suffix("` or supply corrected wording.")
                .context("invalid confirmation challenge")?;
            ensure!(
                digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit()),
                "invalid confirmation digest"
            );
            let reply = json!({"message":{"messageId":format!("{key}-{}", &digest[..16]),"role":"ROLE_USER","taskId":remote,"contextId":task["contextId"],"parts":[{"data":{"answer":format!("confirm {digest}")}}]},"configuration":{"returnImmediately":true}});
            bounded(transport.call(Call::Send(&reply)).await?).await?;
            Ok(false)
        }
        Some("TASK_STATE_COMPLETED") => {
            let evidence = task["artifacts"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|artifact| artifact["parts"].as_array().into_iter().flatten())
                .map(|part| &part["data"])
                .find(|data| {
                    data["readback_verified"] == true
                        && data["title"] == title
                        && data["url"].is_string()
                        && data["issue_id"].is_string()
                })
                .context("missing verified issue link")?;
            if !followup {
                sqlx::query("UPDATE agency_incidents SET ticket=$2 WHERE id=$1")
                    .bind(id)
                    .bind(evidence)
                    .execute(&store.pool)
                    .await?;
            } else {
                let url = evidence["url"]
                    .as_str()
                    .context("missing verified follow-up URL")?;
                if let Some(rule) = profile.services.get(&service) {
                    link_postmortem_followup(rule, id, &key, &body, url).await?;
                }
                sqlx::query("UPDATE agency_incident_events SET body=jsonb_set(body,'{issue_url}',$2) WHERE seq=$1").bind(seq).bind(json!(url)).execute(&store.pool).await?;
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn link_postmortem_followup(
    rule: &Service,
    incident_id: Uuid,
    key: &str,
    body: &Value,
    url: &str,
) -> Result<()> {
    let postmortem = rule.postmortem_directory.join(format!("{incident_id}.md"));
    let text = match tokio::fs::read_to_string(&postmortem).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let url = reqwest::Url::parse(url)?;
    ensure!(
        url.scheme() == "https" && url.username().is_empty() && url.password().is_none(),
        "invalid follow-up link"
    );
    let marker = format!("<!-- {key} -->");
    let title = body["title"].as_str().context("missing follow-up title")?;
    let escaped_title = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let linked = format!("- [{escaped_title}](<{url}>) {marker}");
    if text.lines().any(|line| line == linked) {
        return Ok(());
    }
    let pending = format!("- {title} {marker}");
    let updated = if text.lines().any(|line| line == pending) {
        text.replace(&pending, &linked)
    } else {
        // Older reports and hand-edited reports may not have the reserved line.
        format!("{}\n\n{linked}\n", text.trim_end())
    };
    let staged = rule
        .postmortem_directory
        .join(format!(".{incident_id}-{}.md", Uuid::new_v4()));
    let result = async {
        tokio::fs::write(&staged, updated).await?;
        tokio::fs::rename(&staged, &postmortem).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staged).await;
    }
    Ok(result?)
}
