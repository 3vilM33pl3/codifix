//! SSH renewal keeps the host private key on the host and signing authority with a local operator.
use crate::{
    operation::Operation,
    store::{Store, StoredTask, fingerprint},
};
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use codifix_agency::{TaskStatus, Usage};
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::{io::AsyncWriteExt, process::Command};
use uuid::Uuid;

pub mod coordinator;
pub mod operator;

fn profile(host: &str) -> Result<Value> {
    let path =
        std::env::var_os("CODIFIX_SSH_RENEWAL_CONFIG").context("renewal profiles missing")?;
    let bytes = std::fs::read(path)?;
    ensure!(bytes.len() <= 65536, "oversized renewal profiles");
    let profiles: Value = serde_json::from_slice(&bytes)?;
    profiles.get(host).cloned().context("host profile missing")
}

pub(crate) fn operator_allowed(host: &str, actor: &str) -> Result<bool> {
    Ok(profile(host)?["operators"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(actor))))
}

async fn adapter(body: Value) -> Result<Value> {
    let mut child = Command::new("python3")
        .args(["-I", "-c", include_str!("../ssh/renew.py")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    {
        let mut stdin = child.stdin.take().context("missing subprocess stdin")?;
        stdin.write_all(&serde_json::to_vec(&body)?).await?;
    }
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(110),
        child.wait_with_output(),
    )
    .await??;
    ensure!(
        result.status.success() && result.stdout.len() <= 65536,
        "SSH adapter unavailable"
    );
    Ok(serde_json::from_slice(&result.stdout)?)
}

async fn stopped(
    store: &Store,
    task: &StoredTask,
    reason: &str,
    rejected: bool,
) -> Result<Option<Value>> {
    let mut contract = task.contract.clone();
    contract.transition(
        if rejected {
            TaskStatus::Rejected
        } else {
            TaskStatus::Failed
        },
        Utc::now(),
    )?;
    let mut tx = store.pool.begin().await?;
    ensure!(sqlx::query("UPDATE agency_tasks SET contract=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$3").bind(contract.id).bind(serde_json::to_value(contract)?).bind(task.revision).execute(&mut *tx).await?.rows_affected()==1,"task changed");
    sqlx::query("INSERT INTO agency_outcomes(task_id,evidence) VALUES($1,$2)")
        .bind(task.contract.id)
        .bind(json!({"reason":reason,"verified":false}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(None)
}

pub(crate) async fn execute(
    op: Operation,
    store: &Store,
    task: &StoredTask,
) -> Result<Option<Value>> {
    let resource = &task.request.resource;
    ensure!(resource.kind == "ssh_host", "invalid SSH host resource");
    let mut body = json!({"id":task.contract.id,"host":resource.id,"requester":task.requester});
    let mut effect = false;
    match op {
        Operation::SshHostInspect => body["operation"] = json!("inspect"),
        Operation::SshHostSign => {
            body["operation"] = json!("review");
            body["request"] = task.request.payload["request"].clone();
            let review = adapter(body.clone()).await?;
            if let Some(reason) = review["reason"].as_str() {
                return stopped(store, task, reason, false).await;
            }
            let Some(answer) = task
                .contract
                .clarification
                .as_ref()
                .and_then(|c| c.answer.as_ref())
            else {
                let mut waiting = task.clone();
                waiting.contract.clarify(format!("Waiting for an authorized operator to Step-sign the existing public key for {} ({}). Stage the signed certificate for task {} and approve or reject.",resource.id,review["ok"]["fingerprint"].as_str().context("invalid review")?,task.contract.id),Utc::now())?;
                store.update_contract(&waiting).await?;
                return Ok(None);
            };
            if let Some(reason) = answer.strip_prefix("reject: ") {
                return stopped(store, task, reason, true).await;
            }
            ensure!(answer == "approve", "invalid signing decision");
            let actor:String=sqlx::query_scalar("SELECT sender FROM agency_messages WHERE task_id=$1 AND body->>'answer'='approve' ORDER BY created_at DESC LIMIT 1").bind(task.contract.id).fetch_one(&store.pool).await?;
            body["approver"] = json!(actor);
            body["operation"] = json!("handoff");
            effect = true;
        }
        Operation::SshHostInstall => {
            body["operation"] = json!("install");
            body["renewal_id"] = json!(serde_json::from_value::<Uuid>(
                task.request.payload["renewal_id"].clone()
            )?);
            effect = true;
        }
        _ => anyhow::bail!("unsupported SSH operation"),
    }
    let effect_id = Uuid::new_v5(&task.contract.id, b"ssh-renewal");
    if effect {
        store
            .admit_effect(
                task.contract.id,
                task.revision,
                Usage::default(),
                &codifix_store::effects::EffectIntent {
                    id: effect_id,
                    project_id: Some(task.project_id),
                    kind: codifix_store::effects::EffectKind::ProviderRequest,
                    host_id: resource.id.clone(),
                    resource: format!("ssh_host:{}", resource.id),
                    input_fingerprint: fingerprint(&body)?,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-ssh-renewal".into(),
                },
            )
            .await?;
    } else {
        store
            .authorize(task.contract.id, task.revision, Usage::default())
            .await?;
    }
    let result = adapter(body).await;
    if effect {
        let status = if result.as_ref().is_ok_and(|v| v.get("ok").is_some()) {
            codifix_store::effects::EffectStatus::Confirmed
        } else {
            codifix_store::effects::EffectStatus::Uncertain
        };
        codifix_store::effects::conclude(
            &store.pool,
            effect_id,
            Uuid::new_v4(),
            status,
            codifix_store::effects::EffectReason::Observed,
        )
        .await?;
    }
    let value = match result {
        Ok(v) => v,
        Err(_) => return stopped(store, task, "ssh_adapter_unavailable", false).await,
    };
    if let Some(reason) = value["reason"].as_str() {
        return stopped(store, task, reason, false).await;
    }
    Ok(Some(
        value.get("ok").context("invalid adapter evidence")?.clone(),
    ))
}
