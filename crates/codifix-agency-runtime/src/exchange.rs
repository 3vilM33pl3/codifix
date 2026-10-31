use crate::{
    config::Config,
    discovery::bounded,
    store::{Store, StoredTask, Submit, fingerprint},
};
use anyhow::{Result, ensure};
use codifix_agency::{
    ResourceScope, Usage,
    a2a::{self, Part},
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub async fn delegate(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Option<Value>> {
    let peer_id = task.request.payload["peer"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("peer required"))?;
    ensure!(
        task.request.resource
            == ResourceScope {
                kind: "peer".into(),
                id: peer_id.into()
            },
        "peer disclosure scope mismatch"
    );
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == peer_id)
        .ok_or_else(|| anyhow::anyhow!("peer is not enrolled"))?;
    ensure!(
        peer.instance_id != cfg.instance_id,
        "self delegation would deadlock a project slot"
    );
    let endpoint = peer
        .endpoint
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("peer has no task endpoint"))?;
    let token = std::env::var(
        peer.outbound_token_env
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("peer outbound credential absent"))?,
    )
    .map_err(|_| anyhow::anyhow!("peer credential unavailable"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let mut stored = sqlx::query("SELECT * FROM agency_remote_tasks WHERE task_id=$1")
        .bind(task.contract.id)
        .fetch_optional(&store.pool)
        .await?;
    if stored.is_none() {
        let mut request: Submit = serde_json::from_value(task.request.payload["request"].clone())?;
        ensure!(
            request.deadline <= task.request.deadline
                && request.budget.delegation_depth < task.request.budget.delegation_depth,
            "delegated task exceeds deadline or depth"
        );
        request.request_id = task.contract.id.to_string();
        request.parent_id = None;
        let body = a2a::SendMessageRequest {
            message: a2a::Message {
                message_id: request.request_id.clone(),
                role: a2a::Role::User,
                parts: vec![Part::Data(serde_json::to_value(&request)?)],
                context_id: None,
                task_id: None,
            },
            tenant: None,
            configuration: Some(a2a::SendMessageConfiguration {
                return_immediately: true,
                ..Default::default()
            }),
            metadata: None,
        };
        let effect = Uuid::new_v5(&task.contract.id, b"delegate");
        store
            .admit_effect(
                task.contract.id,
                task.revision,
                Usage {
                    micro_usd: request.budget.micro_usd,
                    calls: request.budget.calls,
                    turns: request.budget.turns,
                },
                &codifix_store::effects::EffectIntent {
                    id: effect,
                    project_id: Some(task.project_id),
                    kind: codifix_store::effects::EffectKind::ProviderRequest,
                    host_id: "agency".into(),
                    resource: format!("peer:{peer_id}"),
                    input_fingerprint: fingerprint(&body)?,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-delegate".into(),
                },
            )
            .await?;
        sqlx::query("INSERT INTO agency_remote_tasks(task_id,peer_id,request) VALUES($1,$2,$3)")
            .bind(task.contract.id)
            .bind(peer_id)
            .bind(serde_json::to_value(&body)?)
            .execute(&store.pool)
            .await?;
        let result = bounded(
            client
                .post(format!("{}/message:send", endpoint.trim_end_matches('/')))
                .header("A2A-Version", "1.0")
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?,
        )
        .await?;
        let wire: a2a::SendMessageResponse = serde_json::from_value(result.clone())?;
        let a2a::SendMessageResponse::Task(remote) = wire else {
            anyhow::bail!("delegation requires durable remote task")
        };
        remote.validate()?;
        sqlx::query("UPDATE agency_remote_tasks SET remote_id=$2,context_id=$3,last_response=$4 WHERE task_id=$1").bind(task.contract.id).bind(&remote.id).bind(&remote.context_id).bind(result).execute(&store.pool).await?;
        codifix_store::effects::conclude(
            &store.pool,
            effect,
            Uuid::new_v4(),
            codifix_store::effects::EffectStatus::Confirmed,
            codifix_store::effects::EffectReason::Observed,
        )
        .await?;
        stored = sqlx::query("SELECT * FROM agency_remote_tasks WHERE task_id=$1")
            .bind(task.contract.id)
            .fetch_optional(&store.pool)
            .await?;
    }
    let row = stored.ok_or_else(|| anyhow::anyhow!("remote task state missing"))?;
    ensure!(
        row.try_get::<String, _>("peer_id")? == peer_id,
        "remote peer binding changed"
    );
    let remote_id = row
        .try_get::<Option<String>, _>("remote_id")?
        .ok_or_else(|| {
            anyhow::anyhow!("remote submission uncertain; reconcile by original message ID")
        })?;
    let context = row
        .try_get::<Option<String>, _>("context_id")?
        .ok_or_else(|| anyhow::anyhow!("remote context missing"))?;
    // A source answer is forwarded once per local revision, retaining the remote
    // task/context. Reusing the stable message ID permits recipient deduplication.
    if let Some(answer) = task
        .contract
        .clarification
        .as_ref()
        .and_then(|c| c.answer.as_ref())
    {
        if row.try_get::<Option<i64>, _>("reply_revision")? != Some(task.revision) {
            store
                .authorize(task.contract.id, task.revision, Usage::default())
                .await?;
            let body = a2a::SendMessageRequest {
                message: a2a::Message {
                    message_id: format!("{}:{}", task.contract.id, task.revision),
                    role: a2a::Role::User,
                    parts: vec![Part::Data(
                        json!({"answer":answer,"replacement_payload":task.request.payload.get("answer_payload")}),
                    )],
                    context_id: Some(context.clone()),
                    task_id: Some(remote_id.clone()),
                },
                tenant: None,
                configuration: Some(a2a::SendMessageConfiguration {
                    return_immediately: true,
                    ..Default::default()
                }),
                metadata: None,
            };
            bounded(
                client
                    .post(format!("{}/message:send", endpoint.trim_end_matches('/')))
                    .header("A2A-Version", "1.0")
                    .bearer_auth(&token)
                    .json(&body)
                    .send()
                    .await?,
            )
            .await?;
            sqlx::query("UPDATE agency_remote_tasks SET reply_revision=$2 WHERE task_id=$1")
                .bind(task.contract.id)
                .bind(task.revision)
                .execute(&store.pool)
                .await?;
        }
    }
    loop {
        if let Err(error) = store
            .authorize(task.contract.id, task.revision, Usage::default())
            .await
        {
            let _ = cancel_remote(store, cfg, task.contract.id).await;
            return Err(error);
        }
        let remote: a2a::Task = serde_json::from_value(
            bounded(
                client
                    .get(format!(
                        "{}/tasks/{}",
                        endpoint.trim_end_matches('/'),
                        urlencoding::encode(&remote_id)
                    ))
                    .header("A2A-Version", "1.0")
                    .bearer_auth(&token)
                    .send()
                    .await?,
            )
            .await?,
        )?;
        remote.validate()?;
        ensure!(
            remote.id == remote_id && remote.context_id == context,
            "remote task identity mismatch"
        );
        sqlx::query("UPDATE agency_remote_tasks SET last_response=$2 WHERE task_id=$1")
            .bind(task.contract.id)
            .bind(serde_json::to_value(&remote)?)
            .execute(&store.pool)
            .await?;
        match remote.status.state {
            a2a::TaskState::Completed => {
                return Ok(Some(
                    json!({"peer":peer_id,"reported_result":remote,"independent_verification":false}),
                ));
            }
            a2a::TaskState::InputRequired | a2a::TaskState::AuthRequired => {
                let mut local = task.clone();
                let question = remote
                    .status
                    .message
                    .as_ref()
                    .map(|m| {
                        m.parts
                            .iter()
                            .filter_map(|p| {
                                if let Part::Text(t) = p {
                                    Some(t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_else(|| {
                        "The peer requires input through its authorized channel.".into()
                    });
                local.contract.clarify(question, chrono::Utc::now())?;
                store.update_contract(&local).await?;
                return Ok(None);
            }
            a2a::TaskState::Failed | a2a::TaskState::Canceled | a2a::TaskState::Rejected => {
                anyhow::bail!("remote task did not complete")
            }
            _ => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
}
pub async fn cancel_remote(store: &Store, cfg: &Config, id: Uuid) -> Result<()> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT peer_id,remote_id FROM agency_remote_tasks WHERE task_id=$1")
            .bind(id)
            .fetch_optional(&store.pool)
            .await?;
    let Some((peer_id, Some(remote))) = row else {
        return Ok(());
    };
    let peer = cfg
        .peers
        .iter()
        .find(|p| p.id == peer_id)
        .ok_or_else(|| anyhow::anyhow!("unknown peer"))?;
    let token = std::env::var(
        peer.outbound_token_env
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing credential"))?,
    )?;
    let endpoint = peer
        .endpoint
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing endpoint"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    bounded(
        client
            .post(format!(
                "{}/tasks/{}:cancel",
                endpoint.trim_end_matches('/'),
                urlencoding::encode(&remote)
            ))
            .header("A2A-Version", "1.0")
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await?,
    )
    .await?;
    Ok(())
}
