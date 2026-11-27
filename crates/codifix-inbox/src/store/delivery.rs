//! Admission and transactional delivery to a human-owned conversation.
use super::{
    Store, admission, append, context_message, digest, enqueue_turn, member_live, owner, template,
};
use crate::{Detail, SendMessage};
use anyhow::{Result, ensure};
use chrono::Utc;
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

impl Store {
    pub async fn send(&self, principal: Uuid, id: Uuid, req: &SendMessage) -> Result<Detail> {
        validate_message(req)?;
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let conversation = owner(&mut tx, self.home_instance_id, principal, id).await?;
        deliver(&mut tx, &conversation, req, None).await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }

    pub async fn respond(
        &self,
        principal: Uuid,
        conversation: Uuid,
        task: Uuid,
        client_message_id: Uuid,
        answer: String,
    ) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let row = owner(&mut tx, self.home_instance_id, principal, conversation).await?;
        let (root, member): (Uuid, Uuid) = sqlx::query_as(
            "SELECT o.root_message_id, o.member_id FROM inbox_task_origins o
             JOIN inbox_task_cards c ON c.task_id = o.task_id
             WHERE o.task_id = $1 AND o.conversation_id = $2 AND o.principal_id = $3",
        )
        .bind(task)
        .bind(conversation)
        .bind(principal)
        .fetch_one(&mut *tx)
        .await?;
        let req = SendMessage {
            client_message_id,
            body: answer,
            format: "text".into(),
            artifact: None,
            recipient_member_ids: vec![member],
        };
        validate_message(&req)?;
        deliver(&mut tx, &row, &req, Some((task, root))).await?;
        tx.commit().await?;
        self.detail(principal, conversation).await
    }
}

fn validate_message(req: &SendMessage) -> Result<()> {
    ensure!(
        !req.client_message_id.is_nil()
            && !req.body.trim().is_empty()
            && req.body.len() <= 32768
            && matches!(req.format.as_str(), "text" | "markdown" | "plan")
            && serde_json::to_vec(&req.artifact)?.len() <= 32768
            && req.recipient_member_ids.len() <= 8,
        "invalid message"
    );
    ensure!(
        context_message(&json!({"body":req.body,"artifact":req.artifact,"role":"human"}))["body"]
            .as_str()
            .is_some_and(|body| body.len() <= 32768),
        "message and plan artifact exceed context limit"
    );
    Ok(())
}

/// Both entry points acquire the admission lock and check the current owner
/// before looking up a delivery key. A replay must match the full original
/// request, including its clarification binding, and never queues more work.
async fn deliver(
    tx: &mut Transaction<'_, Postgres>,
    conversation: &PgRow,
    req: &SendMessage,
    resume: Option<(Uuid, Uuid)>,
) -> Result<()> {
    let id: Uuid = conversation.try_get("id")?;
    let home: Uuid = conversation.try_get("home_instance_id")?;
    let principal: Uuid = conversation.try_get("principal_id")?;
    let hash = digest(&(req, resume))?;
    if already_delivered(tx, id, req.client_message_id, &hash).await? {
        return Ok(());
    }
    ensure!(
        !conversation.try_get::<bool, _>("paused")?,
        "conversation paused"
    );
    let mut recipients = req.recipient_member_ids.clone();
    if recipients.is_empty() {
        recipients.push(
            conversation
                .try_get::<Option<Uuid>, _>("lead_member_id")?
                .ok_or_else(|| anyhow::anyhow!("choose a lead"))?,
        );
    }
    recipients.sort();
    recipients.dedup();
    let mut grants = Vec::new();
    for member in &recipients {
        grants.push(member_live(tx, home, principal, id, *member).await?.0);
    }
    let root = resume
        .map(|(_, root)| root)
        .unwrap_or(req.client_message_id);
    if let Some((task, _)) = resume {
        let question: bool = sqlx::query_scalar("SELECT document->'question' IS NOT NULL AND document->'question'<>'null'::jsonb FROM inbox_task_cards WHERE task_id=$1 AND conversation_id=$2 FOR UPDATE").bind(task).bind(id).fetch_one(&mut **tx).await?;
        ensure!(question, "question already answered");
        sqlx::query("UPDATE inbox_task_cards SET document=jsonb_set(jsonb_set(document,'{question}','null'::jsonb),'{status}',to_jsonb('continuing'::text)) WHERE task_id=$1").bind(task).execute(&mut **tx).await?;
    }
    append(
        tx,
        id,
        req.client_message_id,
        "human",
        None,
        root,
        &req.body,
        &req.format,
        req.artifact.as_ref(),
        &recipients,
        &hash,
        "queued",
    )
    .await?;
    if resume.is_none() {
        // One shared ceiling per dimension. Adding recipients does not add
        // their allowances together. Each dispatch still uses its own grant.
        let mut budget = codifix_agency::Budget::default();
        for grant in &grants {
            let limit = template(&grant.request)?.3;
            budget.micro_usd = budget.micro_usd.max(limit.micro_usd);
            budget.calls = budget.calls.max(limit.calls);
            budget.turns = budget.turns.max(limit.turns);
            budget.delegation_depth = budget.delegation_depth.max(limit.delegation_depth);
        }
        let deadline = grants
            .iter()
            .map(|g| g.expires_at)
            .min()
            .ok_or_else(|| anyhow::anyhow!("recipient missing"))?
            .min(Utc::now() + chrono::Duration::hours(1));
        sqlx::query(
            "INSERT INTO inbox_roots(id,conversation_id,budget,deadline) VALUES($1,$2,$3,$4)",
        )
        .bind(root)
        .bind(id)
        .bind(serde_json::to_value(budget)?)
        .bind(deadline)
        .execute(&mut **tx)
        .await?;
    }
    for member in recipients {
        enqueue_turn(
            tx,
            home,
            principal,
            id,
            member,
            root,
            req.client_message_id,
            None,
        )
        .await?;
    }
    Ok(())
}

/// Check a stable delivery key after authenticating its caller and acquiring
/// the admission lock. Each transport must bind its sender into its scope
/// or digest before passing either value here.
pub(super) async fn already_delivered(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    key: Uuid,
    hash: &str,
) -> Result<bool> {
    if let Some(row) =
        sqlx::query("SELECT conversation_id,request_digest FROM inbox_messages WHERE id=$1")
            .bind(key)
            .fetch_optional(&mut **tx)
            .await?
    {
        ensure!(
            row.try_get::<Uuid, _>("conversation_id")? == conversation
                && row.try_get::<String, _>("request_digest")? == hash,
            "message id conflict"
        );
        return Ok(true);
    }
    Ok(false)
}
