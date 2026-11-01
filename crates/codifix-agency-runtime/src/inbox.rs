//! Same-database human gateway. No peer bearer token or public discovery record
//! can create an origin binding; only a live service-issued ticket can do so.
use crate::{
    config::Config,
    store::{Store, Submit},
};
use anyhow::{Result, ensure};
use codifix_agency::conversation::{ConversationReply, ConversationTurn};
use serde_json::{Value, json};
use uuid::Uuid;

pub async fn dispatch_once(store: &Store, cfg: &Config) -> Result<bool> {
    if !cfg.inbox_enabled {
        return Ok(false);
    }
    let tickets = codifix_inbox::store::pending_dispatches(&store.pool, cfg.instance_id).await?;
    let mut dispatched = false;
    for ticket in tickets {
        let result = async {
            let request: Submit = serde_json::from_value(ticket.request)?;
            ensure!(
                cfg.specialists
                    .iter()
                    .any(|s| s.specialist_id == request.specialist_id
                        && s.capabilities.contains(&request.capability)),
                "specialist unavailable"
            );
            if let Some(peer_id) = ticket.peer_id {
                ensure!(
                    request.capability == "delegate" && request.payload["peer"] == peer_id,
                    "inbox peer binding mismatch"
                );
                let peer = cfg
                    .peers
                    .iter()
                    .find(|p| p.id == peer_id)
                    .ok_or_else(|| anyhow::anyhow!("peer is not enrolled"))?;
                let remote: Submit = serde_json::from_value(request.payload["request"].clone())?;
                if remote.capability == "conversation_turn" {
                    let turn: ConversationTurn =
                        serde_json::from_value(remote.payload["turn"].clone())?;
                    turn.validate_recipient(
                        cfg.instance_id,
                        peer.instance_id,
                        &remote.specialist_id,
                    )?;
                }
            }
            store
                .submit_inbox(&request, ticket.id, cfg.instance_id)
                .await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if result.is_err() {
            codifix_inbox::store::reject_dispatch(
                &store.pool,
                ticket.id,
                cfg.instance_id,
                "Dispatch denied by current authority, configuration, or request validation.",
            )
            .await?;
        } else {
            dispatched = true;
        }
    }
    Ok(dispatched)
}

/// Project durable outcomes after restarts too; a missed notification cannot
/// strand the inbox. The home store deduplicates task revisions/results.
pub async fn reconcile(store: &Store, cfg: &Config) -> Result<()> {
    if !cfg.inbox_enabled {
        return Ok(());
    }
    cancel_commands(store, cfg).await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT t.id FROM agency_tasks t JOIN inbox_task_origins o ON o.task_id=t.id LEFT JOIN agency_inbox_projections p ON p.task_id=t.id WHERE o.home_instance_id=$1 AND (p.revision IS NULL OR p.revision<>t.revision OR t.contract->>'status' IN ('submitted','working','input_required','plan_review_required') OR EXISTS(SELECT 1 FROM inbox_task_commands c WHERE c.task_id=t.id AND NOT c.done) OR EXISTS(SELECT 1 FROM agency_remote_tasks r WHERE r.task_id=t.id AND r.cancel_requested AND NOT r.cancel_completed) OR EXISTS(SELECT 1 FROM inbox_task_cards c WHERE c.task_id=t.id AND c.document->>'cancel_requested'='true')) ORDER BY t.updated_at,t.id LIMIT 256"
    ).bind(cfg.instance_id).fetch_all(&store.pool).await?;
    for id in ids {
        let mut task = store.get_internal(id).await?;
        if !task.contract.status.is_terminal() && store.live_authority(&task).await.is_err() {
            task = store.cancel(id, &gateway(cfg.instance_id)).await?;
            let _ = crate::exchange::cancel_remote(store, cfg, id).await;
        }
        let evidence: Option<Value> =
            sqlx::query_scalar("SELECT evidence FROM agency_outcomes WHERE task_id=$1")
                .bind(id)
                .fetch_optional(&store.pool)
                .await?;
        let normalized = evidence
            .as_ref()
            .map(|value| normalize_evidence(&task.request, value))
            .transpose();
        let status = serde_json::to_value(task.contract.status)?;
        let (status, question, normalized) = match normalized {
            Ok(evidence) => (
                status
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid status"))?,
                task.contract
                    .clarification
                    .as_ref()
                    .filter(|q| q.answer.is_none())
                    .map(|q| q.question.as_str()),
                evidence,
            ),
            Err(_) => (
                "failed",
                Some(
                    "The remote specialist returned an invalid or mismatched conversation result.",
                ),
                None,
            ),
        };
        codifix_inbox::store::record_task_result(
            &store.pool,
            id,
            status,
            question,
            normalized.as_ref(),
        )
        .await?;
        sqlx::query("INSERT INTO agency_inbox_projections(task_id,revision) VALUES($1,$2) ON CONFLICT(task_id) DO UPDATE SET revision=EXCLUDED.revision,projected_at=clock_timestamp()")
            .bind(id).bind(task.revision).execute(&store.pool).await?;
    }
    Ok(())
}

fn gateway(home: Uuid) -> crate::config::Peer {
    crate::config::Peer {
        id: format!("inbox-gateway:{home}"),
        instance_id: home,
        token_env: String::new(),
        operator: false,
        mandates: Default::default(),
        endpoint: None,
        outbound_token_env: None,
    }
}

async fn cancel_commands(store: &Store, cfg: &Config) -> Result<()> {
    let commands:Vec<(Uuid,Uuid)>=sqlx::query_as("SELECT c.id,c.task_id FROM inbox_task_commands c JOIN inbox_task_origins o ON o.task_id=c.task_id WHERE o.home_instance_id=$1 AND c.kind='cancel' AND NOT c.done ORDER BY c.id LIMIT 256")
        .bind(cfg.instance_id).fetch_all(&store.pool).await?;
    for (id, task) in commands {
        store.cancel(task, &gateway(cfg.instance_id)).await?;
        let _ = crate::exchange::cancel_remote(store, cfg, task).await;
        sqlx::query("UPDATE inbox_task_commands SET done=true WHERE id=$1")
            .bind(id)
            .execute(&store.pool)
            .await?;
    }
    Ok(())
}

/// Remote evidence remains explicitly peer-reported. Only a typed reply bound
/// to the original turn is handed to the home proposal admission path.
fn normalize_evidence(request: &Submit, evidence: &Value) -> Result<Value> {
    if request.capability != "delegate" {
        return Ok(evidence.clone());
    }
    let remote: Submit = serde_json::from_value(request.payload["request"].clone())?;
    if remote.capability != "conversation_turn" {
        return Ok(evidence.clone());
    }
    let turn: ConversationTurn = serde_json::from_value(remote.payload["turn"].clone())?;
    let artifacts = evidence
        .pointer("/reported_result/artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("remote conversation artifact missing"))?;
    let mut found = None;
    for artifact in artifacts {
        if let Some(parts) = artifact.get("parts").and_then(Value::as_array) {
            for part in parts {
                if let Some(data) = part.get("data")
                    && let Some(value) = data.get("conversation_reply")
                {
                    ensure!(found.is_none(), "ambiguous remote conversation artifact");
                    ensure!(
                        data["turn_id"] == json!(turn.turn_id)
                            && data["disclosure_digest"] == turn.disclosure_digest,
                        "remote conversation input binding changed"
                    );
                    let reply: ConversationReply = serde_json::from_value(value.clone())?;
                    reply.validate_for(&turn)?;
                    found = Some(serde_json::to_value(reply)?);
                }
            }
        }
    }
    Ok(
        json!({"conversation_reply":found.ok_or_else(||anyhow::anyhow!("remote reply missing"))?,
        "turn_id":turn.turn_id,"disclosure_digest":turn.disclosure_digest,
        "peer":request.payload["peer"],"independent_verification":false,"reported_result":evidence}),
    )
}
