use crate::*;
use anyhow::{Result, ensure};
use codifix_agency::{Budget, Mandate, ResourceScope, Usage};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    pin::Pin,
};

pub fn digest(value: &impl Serialize) -> Result<String> {
    // JSON Value sorts object keys, matching ticket verification in the runtime.
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::to_value(value)?)?)
    ))
}
pub async fn admission(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('agency-admission',0))")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn human(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<()> {
    ensure!(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE)").bind(id).fetch_one(&mut **tx).await?, "human unavailable");
    Ok(())
}
#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
    pub home_instance_id: Uuid,
}

fn template(request: &Value) -> Result<(Uuid, String, ResourceScope, Budget)> {
    let mandate = serde_json::from_value(request["mandate_id"].clone())?;
    let capability = request["capability"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("capability required"))?
        .to_owned();
    let resource = serde_json::from_value(request["resource"].clone())?;
    let budget: Budget = serde_json::from_value(request["budget"].clone())?;
    ensure!(
        budget.calls > 0 && budget.turns > 0 && budget.micro_usd <= i64::MAX as u64,
        "invalid invocation budget"
    );
    ensure!(
        request["parent_id"].is_null(),
        "grant template cannot forge task ancestry"
    );
    Ok((mandate, capability, resource, budget))
}
async fn grant_live(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    home: Uuid,
    owner: Uuid,
) -> Result<(InvocationGrant, i64)> {
    human(tx, owner).await?;
    let row = sqlx::query("SELECT document,revision FROM inbox_grants WHERE id=$1 AND home_instance_id=$2 AND principal_id=$3 AND NOT revoked AND expires_at>clock_timestamp()").bind(id).bind(home).bind(owner).fetch_one(&mut **tx).await?;
    let grant: InvocationGrant = serde_json::from_value(row.try_get("document")?)?;
    ensure!(
        grant.contact.enrolled && grant.contact.available,
        "specialist unavailable"
    );
    let (mandate_id, capability, resource, _) = template(&grant.request)?;
    project_access(tx, owner, mandate_id).await?;
    check_mandate(tx, mandate_id, &capability, &resource, home).await?;
    Ok((grant, row.try_get("revision")?))
}
async fn project_access(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    mandate_id: Uuid,
) -> Result<()> {
    // Human global_role is deliberately ignored, matching service authentication.
    // The companion cannot evaluate service-local group mappings; explicit global
    // permissions or a persisted project membership are required for invocation.
    let principal = sqlx::query("SELECT global_permissions_json FROM auth_principals WHERE id=$1 AND kind='human_oidc' AND disabled_at IS NULL FOR SHARE")
        .bind(owner).fetch_one(&mut **tx).await?;
    let project: Uuid =
        sqlx::query_scalar("SELECT project_id FROM agency_mandates WHERE id=$1 FOR SHARE")
            .bind(mandate_id)
            .fetch_one(&mut **tx)
            .await?;
    let memberships = sqlx::query("SELECT role,permissions_json FROM auth_project_memberships WHERE principal_id=$1 AND project_id=$2 FOR SHARE")
        .bind(owner).bind(project).fetch_all(&mut **tx).await?;
    let explicit: Option<Value> = principal.try_get("global_permissions_json")?;
    let mut can_read = explicit.as_ref().is_some_and(permission_read);
    for membership in memberships {
        let permissions: Option<Value> = membership.try_get("permissions_json")?;
        can_read |= match permissions {
            Some(permissions) => permission_read(&permissions),
            None => matches!(
                membership.try_get::<String, _>("role")?.as_str(),
                "reader" | "writer" | "operator" | "admin"
            ),
        };
    }
    ensure!(can_read, "persisted project access required or revoked");
    Ok(())
}
fn permission_read(value: &Value) -> bool {
    value.as_array().is_some_and(|permissions| {
        permissions
            .iter()
            .any(|p| p.as_str() == Some("memory_read"))
    })
}
async fn check_mandate(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    capability: &str,
    resource: &ResourceScope,
    home: Uuid,
) -> Result<Mandate> {
    let row =
        sqlx::query("SELECT document,paused,revoked,project_id FROM agency_mandates WHERE id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    ensure!(
        !row.try_get::<bool, _>("paused")? && !row.try_get::<bool, _>("revoked")?,
        "mandate inactive"
    );
    let m: Mandate = serde_json::from_value(row.try_get("document")?)?;
    m.validate(Utc::now())?;
    ensure!(m.subject_instance_id == home, "mandate home mismatch");
    m.authorize(capability, resource, None, Usage::default(), Utc::now())?;
    Ok(m)
}
async fn owner(
    tx: &mut Transaction<'_, Postgres>,
    home: Uuid,
    principal: Uuid,
    conversation: Uuid,
) -> Result<PgRow> {
    human(tx, principal).await?;
    Ok(sqlx::query("SELECT c.*,(SELECT COUNT(*) FROM inbox_messages m WHERE m.conversation_id=c.id AND m.role<>'human' AND m.sequence>c.read_sequence) AS unread_count FROM inbox_conversations c WHERE c.id=$1 AND c.home_instance_id=$2 AND c.principal_id=$3 FOR UPDATE OF c").bind(conversation).bind(home).bind(principal).fetch_one(&mut **tx).await?)
}
fn conversation(row: &PgRow) -> Result<Conversation> {
    Ok(Conversation {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        lead_member_id: row.try_get("lead_member_id")?,
        paused: row.try_get("paused")?,
        revision: row.try_get("revision")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        unread_count: row.try_get("unread_count")?,
    })
}
fn message(row: &PgRow) -> Result<Message> {
    Ok(Message {
        id: row.try_get("id")?,
        conversation_id: row.try_get("conversation_id")?,
        sequence: row.try_get("sequence")?,
        role: row.try_get("role")?,
        member_id: row.try_get("member_id")?,
        body: row.try_get("body")?,
        format: row.try_get("format")?,
        artifact: row.try_get("artifact")?,
        created_at: row.try_get("created_at")?,
        delivery: row.try_get("delivery")?,
        root_id: row.try_get("root_id")?,
        recipient_member_ids: serde_json::from_value(row.try_get("recipient_member_ids")?)?,
    })
}
impl Store {
    pub async fn install_grant(&self, grant: &InvocationGrant) -> Result<()> {
        ensure!(
            grant.home_instance_id == self.home_instance_id && grant.expires_at > Utc::now(),
            "invalid home or expiry"
        );
        ensure!(
            grant.contact.enrolled
                && grant.contact.available
                && !grant.contact.id.is_nil()
                && !grant.contact.specialist_id.is_empty(),
            "invalid contact"
        );
        ensure!(
            grant.actions.len() <= 16 && serde_json::to_vec(grant)?.len() <= 131072,
            "grant too large"
        );
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        human(&mut tx, grant.principal_id).await?;
        let mut names = std::collections::BTreeSet::new();
        for request in
            std::iter::once(&grant.request).chain(grant.actions.iter().map(|a| &a.request))
        {
            let (id, cap, resource, budget) = template(request)?;
            let m = check_mandate(&mut tx, id, &cap, &resource, self.home_instance_id).await?;
            project_access(&mut tx, grant.principal_id, id).await?;
            ensure!(
                request["specialist_id"] == m.specialist_id && grant.expires_at <= m.expires_at,
                "grant exceeds mandate"
            );
            m.budget.check(Usage {
                micro_usd: budget.micro_usd,
                calls: budget.calls,
                turns: budget.turns,
            })?;
            ensure!(
                budget.delegation_depth <= m.budget.delegation_depth,
                "grant depth exceeds mandate"
            );
        }
        let remote = grant.request["payload"]["kind"] == "delegate";
        let target = if remote {
            &grant.request["payload"]["request"]
        } else {
            &grant.request
        };
        ensure!(
            grant.contact.remote == remote
                && target["specialist_id"].as_str() == Some(grant.contact.specialist_id.as_str())
                && if remote {
                    grant.contact.instance_id != self.home_instance_id
                } else {
                    grant.contact.instance_id == self.home_instance_id
                },
            "contact/template identity mismatch"
        );
        if remote {
            ensure!(
                grant.request["payload"]["peer"]
                    .as_str()
                    .is_some_and(|p| !p.is_empty())
                    && target["payload"]["kind"] == "conversation_turn"
                    && target["parent_id"].is_null(),
                "invalid remote conversation template"
            );
            template(target)?;
        }
        for a in &grant.actions {
            ensure!(
                matches!(
                    a.request["payload"]["kind"].as_str(),
                    Some(
                        "inspect"
                            | "plan_prepare"
                            | "plan_apply"
                            | "plan_inspect"
                            | "openbao_recover"
                    )
                ),
                "unsupported action template"
            );
            ensure!(
                (!a.key.is_empty()
                    && a.key.len() <= 128
                    && a.key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b)))
                    && names.insert(&a.key),
                "invalid action key"
            );
        }
        ensure!(
            matches!(
                grant.request["payload"]["kind"].as_str(),
                Some("conversation_turn" | "delegate")
            ),
            "conversation template required"
        );
        sqlx::query("INSERT INTO inbox_grants(id,home_instance_id,principal_id,contact_id,document,expires_at) VALUES($1,$2,$3,$4,$5,$6)").bind(grant.id).bind(self.home_instance_id).bind(grant.principal_id).bind(grant.contact.id).bind(serde_json::to_value(grant)?).bind(grant.expires_at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn revoke_grant(&self, id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        ensure!(sqlx::query("UPDATE inbox_grants SET revoked=true,revision=revision+1 WHERE id=$1 AND home_instance_id=$2").bind(id).bind(self.home_instance_id).execute(&mut *tx).await?.rows_affected()==1,"grant not found");
        invalidate(&mut tx, Some(id), None).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn specialists(&self, principal: Uuid) -> Result<Vec<Specialist>> {
        let mut tx = self.pool.begin().await?;
        human(&mut tx, principal).await?;
        let rows=sqlx::query("SELECT document,revoked,expires_at FROM inbox_grants WHERE home_instance_id=$1 AND principal_id=$2 ORDER BY id").bind(self.home_instance_id).bind(principal).fetch_all(&mut *tx).await?;
        let mut out = Vec::new();
        for row in rows {
            let g: InvocationGrant = serde_json::from_value(row.try_get("document")?)?;
            let mut s = g.contact;
            s.grant_id = Some(g.id);
            s.mandate_id = serde_json::from_value(g.request["mandate_id"].clone()).ok();
            s.available &= !row.try_get::<bool, _>("revoked")?
                && row.try_get::<DateTime<Utc>, _>("expires_at")? > Utc::now();
            out.push(s);
        }
        tx.commit().await?;
        Ok(out)
    }
    pub async fn list(&self, principal: Uuid) -> Result<Vec<Conversation>> {
        let mut tx = self.pool.begin().await?;
        human(&mut tx, principal).await?;
        let rows=sqlx::query("SELECT c.*,(SELECT COUNT(*) FROM inbox_messages m WHERE m.conversation_id=c.id AND m.role<>'human' AND m.sequence>c.read_sequence) AS unread_count FROM inbox_conversations c WHERE c.home_instance_id=$1 AND c.principal_id=$2 ORDER BY c.updated_at DESC LIMIT 200").bind(self.home_instance_id).bind(principal).fetch_all(&mut *tx).await?;
        rows.iter().map(conversation).collect()
    }
    pub async fn detail(&self, principal: Uuid, id: Uuid) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        let c = conversation(&owner(&mut tx, self.home_instance_id, principal, id).await?)?;
        let rows=sqlx::query("SELECT m.*,g.document,g.revoked,g.expires_at FROM inbox_members m JOIN inbox_grants g ON g.id=m.grant_id WHERE m.conversation_id=$1 ORDER BY m.joined_sequence,m.id").bind(id).fetch_all(&mut *tx).await?;
        let mut members = Vec::new();
        for r in rows {
            let g: InvocationGrant = serde_json::from_value(r.try_get("document")?)?;
            let mut s = g.contact;
            s.grant_id = Some(g.id);
            s.mandate_id = serde_json::from_value(g.request["mandate_id"].clone()).ok();
            s.available &= !r.try_get::<bool, _>("revoked")?
                && r.try_get::<DateTime<Utc>, _>("expires_at")? > Utc::now();
            members.push(Member {
                id: r.try_get("id")?,
                specialist: s,
                active: r.try_get("active")?,
                revision: r.try_get("revision")?,
                joined_sequence: r.try_get("joined_sequence")?,
                disclosure_summary: r.try_get("disclosure_summary")?,
                selected_message_ids: serde_json::from_value(r.try_get("selected_message_ids")?)?,
            });
        }
        let messages=sqlx::query("SELECT * FROM (SELECT * FROM inbox_messages WHERE conversation_id=$1 ORDER BY sequence DESC LIMIT 500) m ORDER BY sequence").bind(id).fetch_all(&mut *tx).await?.iter().map(message).collect::<Result<Vec<_>>>()?;
        let tasks = sqlx::query_scalar::<_, Value>(
            "SELECT document FROM inbox_task_cards WHERE conversation_id=$1 ORDER BY task_id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(Detail {
            conversation: c,
            members,
            messages,
            tasks,
        })
    }
    pub async fn message_page(
        &self,
        principal: Uuid,
        id: Uuid,
        before_sequence: i64,
        limit: u32,
    ) -> Result<MessagePage> {
        ensure!(
            before_sequence > 0 && (1..=100).contains(&limit),
            "invalid message page"
        );
        let mut tx = self.pool.begin().await?;
        owner(&mut tx, self.home_instance_id, principal, id).await?;
        let mut rows = sqlx::query("SELECT * FROM inbox_messages WHERE conversation_id=$1 AND sequence<$2 ORDER BY sequence DESC LIMIT $3")
            .bind(id).bind(before_sequence).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.pop();
        }
        rows.reverse();
        let messages = rows.iter().map(message).collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(MessagePage { messages, has_more })
    }
    pub async fn create(&self, principal: Uuid, req: &CreateConversation) -> Result<Detail> {
        ensure!(
            !req.client_request_id.is_nil()
                && !req.title.trim().is_empty()
                && req.title.len() <= 200,
            "invalid conversation"
        );
        let id = req.client_request_id;
        let hash = digest(req)?;
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        human(&mut tx, principal).await?;
        if let Some(row)=sqlx::query("SELECT home_instance_id,principal_id,request_digest FROM inbox_conversations WHERE id=$1").bind(id).fetch_optional(&mut *tx).await? {ensure!(row.try_get::<Uuid,_>("home_instance_id")?==self.home_instance_id && row.try_get::<Uuid,_>("principal_id")?==principal && row.try_get::<String,_>("request_digest")?==hash,"conversation id conflict");tx.commit().await?;return self.detail(principal,id).await;}
        sqlx::query("INSERT INTO inbox_conversations(id,home_instance_id,principal_id,request_digest,title) VALUES($1,$2,$3,$4,$5)").bind(id).bind(self.home_instance_id).bind(principal).bind(hash).bind(&req.title).execute(&mut *tx).await?;
        let member = Uuid::new_v5(&id, b"lead");
        join(
            &mut tx,
            self.home_instance_id,
            principal,
            id,
            &AddMember {
                client_request_id: member,
                specialist_id: req.specialist_id,
                summary: String::new(),
                selected_message_ids: vec![],
                confirmed: true,
            },
        )
        .await?;
        sqlx::query("UPDATE inbox_conversations SET lead_member_id=$2 WHERE id=$1")
            .bind(id)
            .bind(member)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn add_member(&self, principal: Uuid, id: Uuid, req: &AddMember) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        owner(&mut tx, self.home_instance_id, principal, id).await?;
        join(&mut tx, self.home_instance_id, principal, id, req).await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn send(&self, principal: Uuid, id: Uuid, req: &SendMessage) -> Result<Detail> {
        self.send_inner(principal, id, req, None).await
    }
    async fn send_inner(
        &self,
        principal: Uuid,
        id: Uuid,
        req: &SendMessage,
        resume: Option<(Uuid, Uuid)>,
    ) -> Result<Detail> {
        ensure!(
            !req.client_message_id.is_nil()
                && !req.body.trim().is_empty()
                && req.body.len() <= 32768
                && matches!(req.format.as_str(), "text" | "markdown" | "plan")
                && serde_json::to_vec(&req.artifact)?.len() <= 32768
                && req.recipient_member_ids.len() <= 8,
            "invalid message"
        );
        ensure!(context_message(&json!({"body":req.body,"artifact":req.artifact,"role":"human"}))["body"].as_str().is_some_and(|body| body.len()<=32768), "message and plan artifact exceed context limit");
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let c = owner(&mut tx, self.home_instance_id, principal, id).await?;
        let hash = digest(&(req, resume))?;
        if let Some(row) =
            sqlx::query("SELECT conversation_id,request_digest FROM inbox_messages WHERE id=$1")
                .bind(req.client_message_id)
                .fetch_optional(&mut *tx)
                .await?
        {
            ensure!(
                row.try_get::<Uuid, _>("conversation_id")? == id
                    && row.try_get::<String, _>("request_digest")? == hash,
                "message id conflict"
            );
            tx.commit().await?;
            return self.detail(principal, id).await;
        }
        ensure!(!c.try_get::<bool, _>("paused")?, "conversation paused");
        let mut recipients = req.recipient_member_ids.clone();
        if recipients.is_empty() {
            recipients.push(
                c.try_get::<Option<Uuid>, _>("lead_member_id")?
                    .ok_or_else(|| anyhow::anyhow!("choose a lead"))?,
            );
        }
        recipients.sort();
        recipients.dedup();
        let mut grants = Vec::new();
        for member in &recipients {
            grants.push(
                member_live(&mut tx, self.home_instance_id, principal, id, *member)
                    .await?
                    .0,
            );
        }
        let root = resume
            .map(|(_, root)| root)
            .unwrap_or(req.client_message_id);
        if let Some((task, _)) = resume {
            let question: bool = sqlx::query_scalar("SELECT document->'question' IS NOT NULL AND document->'question'<>'null'::jsonb FROM inbox_task_cards WHERE task_id=$1 AND conversation_id=$2 FOR UPDATE").bind(task).bind(id).fetch_one(&mut *tx).await?;
            ensure!(question, "question already answered");
            sqlx::query("UPDATE inbox_task_cards SET document=jsonb_set(jsonb_set(document,'{question}','null'::jsonb),'{status}',to_jsonb('continuing'::text)) WHERE task_id=$1").bind(task).execute(&mut *tx).await?;
        }
        append(
            &mut tx,
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
            let budget = template(&grants[0].request)?.3;
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
            .execute(&mut *tx)
            .await?;
        }
        for member in recipients {
            enqueue_turn(
                &mut tx,
                self.home_instance_id,
                principal,
                id,
                member,
                root,
                req.client_message_id,
                None,
            )
            .await?;
        }
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn remove_member(&self, principal: Uuid, id: Uuid, member: Uuid) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        owner(&mut tx, self.home_instance_id, principal, id).await?;
        ensure!(sqlx::query("UPDATE inbox_members SET active=false,revision=revision+1 WHERE id=$1 AND conversation_id=$2").bind(member).bind(id).execute(&mut *tx).await?.rows_affected()==1,"member missing");
        sqlx::query("UPDATE inbox_conversations SET lead_member_id=NULL,revision=revision+1 WHERE id=$1 AND lead_member_id=$2").bind(id).bind(member).execute(&mut *tx).await?;
        invalidate(&mut tx, None, Some(member)).await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn lead(&self, principal: Uuid, id: Uuid, member: Uuid) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        owner(&mut tx, self.home_instance_id, principal, id).await?;
        member_live(&mut tx, self.home_instance_id, principal, id, member).await?;
        sqlx::query(
            "UPDATE inbox_conversations SET lead_member_id=$2,revision=revision+1 WHERE id=$1",
        )
        .bind(id)
        .bind(member)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn pause(&self, principal: Uuid, id: Uuid, paused: bool) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        owner(&mut tx, self.home_instance_id, principal, id).await?;
        sqlx::query("UPDATE inbox_conversations SET paused=$2,revision=revision+1 WHERE id=$1")
            .bind(id)
            .bind(paused)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
    pub async fn read(&self, principal: Uuid, id: Uuid, sequence: i64) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        let c = owner(&mut tx, self.home_instance_id, principal, id).await?;
        ensure!(
            sequence >= 0 && sequence < c.try_get::<i64, _>("next_sequence")?,
            "invalid cursor"
        );
        sqlx::query(
            "UPDATE inbox_conversations SET read_sequence=GREATEST(read_sequence,$2) WHERE id=$1",
        )
        .bind(id)
        .bind(sequence)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.detail(principal, id).await
    }
}
async fn join(
    tx: &mut Transaction<'_, Postgres>,
    home: Uuid,
    principal: Uuid,
    id: Uuid,
    req: &AddMember,
) -> Result<()> {
    ensure!(
        req.confirmed
            && !req.client_request_id.is_nil()
            && req.summary.len() <= 16384
            && req.selected_message_ids.len() <= 32,
        "reviewed disclosure required"
    );
    let hash = digest(req)?;
    if let Some(r) =
        sqlx::query("SELECT conversation_id,request_digest FROM inbox_members WHERE id=$1")
            .bind(req.client_request_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        ensure!(
            r.try_get::<Uuid, _>("conversation_id")? == id
                && r.try_get::<String, _>("request_digest")? == hash,
            "member request conflict"
        );
        return Ok(());
    }
    let gid:Uuid=sqlx::query_scalar("SELECT id FROM inbox_grants WHERE home_instance_id=$1 AND principal_id=$2 AND contact_id=$3").bind(home).bind(principal).bind(req.specialist_id).fetch_one(&mut **tx).await?;
    grant_live(tx, gid, home, principal).await?;
    let sequence: i64 =
        sqlx::query_scalar("SELECT next_sequence FROM inbox_conversations WHERE id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM inbox_members WHERE conversation_id=$1 AND active",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(count < 8, "participant limit");
    let mut selected = Vec::new();
    for mid in &req.selected_message_ids {
        let r = sqlx::query(
            "SELECT * FROM inbox_messages WHERE id=$1 AND conversation_id=$2 AND sequence<$3",
        )
        .bind(mid)
        .bind(id)
        .bind(sequence)
        .fetch_one(&mut **tx)
        .await?;
        selected.push(serde_json::to_value(message(&r)?)?);
    }
    let disclosure = json!({"summary":req.summary,"selected":selected,"watermark":sequence-1,"recipient":req.specialist_id});
    ensure!(
        serde_json::to_vec(&disclosure)?.len() <= 65536,
        "joining context too large"
    );
    sqlx::query("INSERT INTO inbox_members(id,conversation_id,grant_id,request_digest,joined_sequence,disclosure_summary,selected_message_ids,disclosure) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(req.client_request_id).bind(id).bind(gid).bind(hash).bind(sequence).bind(&req.summary).bind(json!(req.selected_message_ids)).bind(disclosure).execute(&mut **tx).await?;
    Ok(())
}
async fn member_live(
    tx: &mut Transaction<'_, Postgres>,
    home: Uuid,
    principal: Uuid,
    conversation: Uuid,
    member: Uuid,
) -> Result<(InvocationGrant, i64, i64)> {
    let r = sqlx::query(
        "SELECT grant_id,revision FROM inbox_members WHERE id=$1 AND conversation_id=$2 AND active",
    )
    .bind(member)
    .bind(conversation)
    .fetch_one(&mut **tx)
    .await?;
    let (g, gr) = grant_live(tx, r.try_get("grant_id")?, home, principal).await?;
    Ok((g, gr, r.try_get("revision")?))
}
#[allow(clippy::too_many_arguments)]
async fn append(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    id: Uuid,
    role: &str,
    member: Option<Uuid>,
    root: Uuid,
    body: &str,
    format: &str,
    artifact: Option<&Value>,
    recipients: &[Uuid],
    hash: &str,
    delivery: &str,
) -> Result<()> {
    let sequence:i64=sqlx::query_scalar("UPDATE inbox_conversations SET next_sequence=next_sequence+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING next_sequence-1").bind(conversation).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO inbox_messages(id,conversation_id,sequence,role,member_id,root_id,body,format,artifact,recipient_member_ids,request_digest,delivery) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(conversation).bind(sequence).bind(role).bind(member).bind(root).bind(body).bind(format).bind(artifact).bind(json!(recipients)).bind(hash).bind(delivery).execute(&mut **tx).await?;
    Ok(())
}
fn context_message(m: &Value) -> Value {
    let role = if m["role"] == "human" {
        "user"
    } else {
        "specialist"
    };
    let mut body = m["body"].as_str().unwrap_or_default().to_owned();
    if !m["artifact"].is_null() {
        body.push_str("\nPlan artifact:\n");
        body.push_str(&m["artifact"].to_string());
    }
    // A validated model reply may include a separate clarification question.
    // Keep the complete owner-visible message, but bound its context projection.
    if m["role"] != "human" && body.len() > 32768 {
        let end = body
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= 32740)
            .last()
            .unwrap_or(0);
        body.truncate(end);
        body.push_str("\n[context truncated]");
    }
    json!({"message_id":m["id"],"role":role,"speaker":m["member_id"].as_str().unwrap_or("human"),"body":body})
}
fn turn_payload(request: &Value) -> &Value {
    if request["payload"]["kind"] == "delegate" {
        &request["payload"]["request"]["payload"]["turn"]
    } else {
        &request["payload"]["turn"]
    }
}

/// Prove that model-derived text cannot carry excluded history into another
/// member's context. Exact human-selected messages are an explicit new disclosure;
/// otherwise a specialist message inherits its immutable source ticket's inputs.
/// Bounded traversal fails closed rather than trusting a model's summary claim.
struct DisclosureFilter {
    conversation: Uuid,
    member: Uuid,
    joined_sequence: i64,
    selected: BTreeSet<Uuid>,
    messages: HashMap<Uuid, bool>,
    tickets: HashMap<Uuid, bool>,
    visited: usize,
}
type DisclosureFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;
impl DisclosureFilter {
    async fn new(
        tx: &mut Transaction<'_, Postgres>,
        conversation: Uuid,
        member: Uuid,
    ) -> Result<Self> {
        let row = sqlx::query("SELECT joined_sequence,selected_message_ids FROM inbox_members WHERE id=$1 AND conversation_id=$2")
            .bind(member).bind(conversation).fetch_one(&mut **tx).await?;
        Ok(Self {
            conversation,
            member,
            joined_sequence: row.try_get("joined_sequence")?,
            selected: serde_json::from_value(row.try_get("selected_message_ids")?)?,
            messages: HashMap::new(),
            tickets: HashMap::new(),
            visited: 0,
        })
    }
    fn message_allowed<'a>(
        &'a mut self,
        tx: &'a mut Transaction<'_, Postgres>,
        id: Uuid,
        depth: usize,
    ) -> DisclosureFuture<'a> {
        Box::pin(async move {
            if id == Uuid::new_v5(&self.member, b"joining-summary") || self.selected.contains(&id) {
                return Ok(true);
            }
            if let Some(allowed) = self.messages.get(&id) {
                return Ok(*allowed);
            }
            // A cycle or an oversized lineage cannot establish disclosure consent.
            self.messages.insert(id, false);
            self.visited += 1;
            if depth > 64 || self.visited > 4096 {
                return Ok(false);
            }
            let Some(row) = sqlx::query("SELECT role,sequence,source_ticket_id FROM inbox_messages WHERE id=$1 AND conversation_id=$2")
                .bind(id).bind(self.conversation).fetch_optional(&mut **tx).await? else { return Ok(false); };
            if row.try_get::<i64, _>("sequence")? < self.joined_sequence {
                return Ok(false);
            }
            let allowed = match row.try_get::<String, _>("role")?.as_str() {
                "human" | "system" => true,
                "specialist" => match row.try_get::<Option<Uuid>, _>("source_ticket_id")? {
                    Some(ticket) => self.ticket_allowed(tx, ticket, depth + 1).await?,
                    None => false,
                },
                _ => false,
            };
            self.messages.insert(id, allowed);
            Ok(allowed)
        })
    }
    fn ticket_allowed<'a>(
        &'a mut self,
        tx: &'a mut Transaction<'_, Postgres>,
        id: Uuid,
        depth: usize,
    ) -> DisclosureFuture<'a> {
        Box::pin(async move {
            if let Some(allowed) = self.tickets.get(&id) {
                return Ok(*allowed);
            }
            self.tickets.insert(id, false);
            self.visited += 1;
            if depth > 64 || self.visited > 4096 {
                return Ok(false);
            }
            let Some(row) = sqlx::query("SELECT request,source_ticket_id FROM inbox_dispatches WHERE id=$1 AND conversation_id=$2")
                .bind(id).bind(self.conversation).fetch_optional(&mut **tx).await? else { return Ok(false); };
            let request: Value = row.try_get("request")?;
            let turn: codifix_agency::conversation::ConversationTurn =
                match serde_json::from_value(turn_payload(&request).clone()) {
                    Ok(turn) => turn,
                    Err(_) => return Ok(false),
                };
            if turn.validate().is_err()
                || turn.conversation_id != self.conversation
                || turn.turn_id != id
            {
                return Ok(false);
            }
            let parent: Option<Uuid> = row.try_get("source_ticket_id")?;
            for input in turn.context {
                let allowed = if input.message_id == Uuid::new_v5(&id, b"peer-request") {
                    match parent {
                        Some(source) => self.ticket_allowed(tx, source, depth + 1).await?,
                        None => false,
                    }
                } else {
                    self.message_allowed(tx, input.message_id, depth + 1)
                        .await?
                };
                if !allowed {
                    return Ok(false);
                }
            }
            self.tickets.insert(id, true);
            Ok(true)
        })
    }
}

async fn context(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    member: Uuid,
) -> Result<(Vec<Value>, String)> {
    let row = sqlx::query(
        "SELECT disclosure,joined_sequence FROM inbox_members WHERE id=$1 AND conversation_id=$2",
    )
    .bind(member)
    .bind(conversation)
    .fetch_one(&mut **tx)
    .await?;
    let disclosure: Value = row.try_get("disclosure")?;
    let mut context = Vec::new();
    if let Some(s) = disclosure["summary"].as_str().filter(|s| !s.is_empty()) {
        context.push(json!({"message_id":Uuid::new_v5(&member,b"joining-summary"),"role":"context","speaker":"human-reviewed-summary","body":s}));
    }
    if let Some(selected) = disclosure["selected"].as_array() {
        context.extend(selected.iter().map(context_message));
    }
    let rows=sqlx::query("SELECT * FROM (SELECT * FROM inbox_messages WHERE conversation_id=$1 AND sequence>=$2 ORDER BY sequence DESC LIMIT 80) m ORDER BY sequence").bind(conversation).bind(row.try_get::<i64,_>("joined_sequence")?).fetch_all(&mut **tx).await?;
    let mut filter = DisclosureFilter::new(tx, conversation, member).await?;
    for row in rows {
        if filter.message_allowed(tx, row.try_get("id")?, 0).await? {
            context.push(context_message(&serde_json::to_value(message(&row)?)?));
        }
    }
    ensure!(
        context.len() <= 128
            && context
                .iter()
                .all(|m| m["body"].as_str().is_some_and(|s| s.len() <= 32768)),
        "context exceeds message limit"
    );
    while serde_json::to_vec(&context)?.len() > 100_000 && context.len() > 1 {
        context.remove(0);
    }
    ensure!(
        serde_json::to_vec(&context)?.len() <= 100_000,
        "context too large"
    );
    let hash = digest(&json!({"disclosure":disclosure,"context":context}))?;
    Ok((context, hash))
}
#[allow(clippy::too_many_arguments)]
async fn enqueue_turn(
    tx: &mut Transaction<'_, Postgres>,
    home: Uuid,
    principal: Uuid,
    conversation: Uuid,
    member: Uuid,
    root: Uuid,
    source: Uuid,
    peer_message: Option<(&str, Uuid)>,
) -> Result<()> {
    let id = Uuid::new_v5(&source, member.as_bytes());
    if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM inbox_dispatches WHERE id=$1)")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?
    {
        return Ok(());
    }
    let (g, gr, mr) = member_live(tx, home, principal, conversation, member).await?;
    let r=sqlx::query("SELECT turns,budget,deadline,cancelled FROM inbox_roots WHERE id=$1 AND conversation_id=$2 FOR UPDATE").bind(root).bind(conversation).fetch_one(&mut **tx).await?;
    let root_budget: Budget = serde_json::from_value(r.try_get("budget")?)?;
    ensure!(
        !r.try_get::<bool, _>("cancelled")?
            && u32::try_from(r.try_get::<i32, _>("turns")?)? < root_budget.turns.min(8),
        "response turn budget exhausted"
    );
    let deadline = r.try_get::<DateTime<Utc>, _>("deadline")?.min(g.expires_at);
    ensure!(deadline > Utc::now(), "response deadline passed");
    let (mut ctx, mut disclosure_digest) = context(tx, conversation, member).await?;
    if let Some((body, source_ticket)) = peer_message {
        let mut disclosure = DisclosureFilter::new(tx, conversation, member).await?;
        ensure!(
            disclosure.ticket_allowed(tx, source_ticket, 0).await?,
            "peer request includes undisclosed context"
        );
        ctx.push(json!({"message_id":Uuid::new_v5(&id,b"peer-request"),"role":"context","speaker":"participant-request","body":body}));
        disclosure_digest =
            digest(&json!({"base":disclosure_digest,"context":ctx,"source_ticket":source_ticket}))?;
    }
    let actions=g.actions.iter().map(|a|json!({"key":a.key,"capability":a.request["capability"],"resource":a.request["resource"]})).collect::<Vec<_>>();
    let others=sqlx::query("SELECT g.document,m.id FROM inbox_members m JOIN inbox_grants g ON g.id=m.grant_id WHERE m.conversation_id=$1 AND m.id<>$2 AND m.active AND NOT g.revoked AND g.expires_at>clock_timestamp()").bind(conversation).bind(member).fetch_all(&mut **tx).await?;
    let peers=others.iter().map(|v| {let d:Value=v.try_get("document")?;Ok(json!({"specialist_id":v.try_get::<Uuid,_>("id")?,"display_name":d["contact"]["display_name"]}))}).collect::<Result<Vec<_>>>()?;
    let turn = json!({"version":1,"home_instance_id":home,"conversation_id":conversation,"root_message_id":root,"turn_id":id,"recipient_instance_id":g.contact.instance_id,"recipient_specialist_id":g.contact.specialist_id,"disclosure_digest":disclosure_digest,"context":ctx,"available_actions":actions,"available_peers":peers});
    serde_json::from_value::<codifix_agency::conversation::ConversationTurn>(turn.clone())?
        .validate()?;
    let payload = json!({"kind":"conversation_turn","turn":turn,"routing":g.routing});
    let mut request = g.request.clone();
    request["request_id"] = json!(id.to_string());
    request["deadline"] = json!(deadline);
    request["parent_id"] = Value::Null;
    if request["payload"]["kind"] == "delegate" {
        request["payload"]["request"]["request_id"] = json!(id.to_string());
        request["payload"]["request"]["deadline"] = json!(deadline);
        request["payload"]["request"]["parent_id"] = Value::Null;
        request["payload"]["request"]["payload"] = payload;
    } else {
        request["payload"] = payload;
    }
    sqlx::query("UPDATE inbox_roots SET turns=turns+1 WHERE id=$1")
        .bind(root)
        .execute(&mut **tx)
        .await?;
    insert_ticket(
        tx,
        id,
        home,
        principal,
        conversation,
        member,
        mr,
        &g,
        gr,
        root,
        &request,
        deadline,
    )
    .await?;
    if let Some((_, source_ticket)) = peer_message {
        sqlx::query("UPDATE inbox_dispatches SET source_ticket_id=$2 WHERE id=$1")
            .bind(id)
            .bind(source_ticket)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn insert_ticket(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    home: Uuid,
    principal: Uuid,
    conversation: Uuid,
    member: Uuid,
    mr: i64,
    g: &InvocationGrant,
    gr: i64,
    root: Uuid,
    request: &Value,
    deadline: DateTime<Utc>,
) -> Result<()> {
    ensure!(
        serde_json::to_vec(request)?.len() <= 262144,
        "dispatch too large"
    );
    sqlx::query("INSERT INTO inbox_dispatches(id,home_instance_id,principal_id,conversation_id,member_id,member_revision,grant_id,grant_revision,root_message_id,request,request_digest,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(id).bind(home).bind(principal).bind(conversation).bind(member).bind(mr).bind(g.id).bind(gr).bind(root).bind(request).bind(digest(request)?).bind(deadline).execute(&mut **tx).await?;
    Ok(())
}
async fn invalidate(
    tx: &mut Transaction<'_, Postgres>,
    grant: Option<Uuid>,
    member: Option<Uuid>,
) -> Result<()> {
    sqlx::query("UPDATE inbox_dispatches SET status='cancelled' WHERE task_id IS NULL AND (($1::uuid IS NOT NULL AND grant_id=$1) OR ($2::uuid IS NOT NULL AND member_id=$2))").bind(grant).bind(member).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO inbox_task_commands(id,task_id,conversation_id,kind,body) SELECT gen_random_uuid(),task_id,conversation_id,'cancel','{}'::jsonb FROM inbox_task_origins WHERE ($1::uuid IS NOT NULL AND grant_id=$1) OR ($2::uuid IS NOT NULL AND member_id=$2)").bind(grant).bind(member).execute(&mut **tx).await?;
    Ok(())
}

pub async fn pending_dispatches(pool: &PgPool, home: Uuid) -> Result<Vec<DispatchTicket>> {
    let rows=sqlx::query("SELECT id,request FROM inbox_dispatches WHERE home_instance_id=$1 AND status='queued' ORDER BY created_at,id LIMIT 32").bind(home).fetch_all(pool).await?;
    rows.into_iter()
        .map(|r| {
            let request: Value = r.try_get("request")?;
            let peer_id = request["payload"]["peer"].as_str().map(str::to_owned);
            Ok(DispatchTicket {
                id: r.try_get("id")?,
                request,
                peer_id,
            })
        })
        .collect()
}
async fn ticket_live(
    tx: &mut Transaction<'_, Postgres>,
    ticket: Uuid,
) -> Result<(PgRow, InvocationGrant)> {
    let r = sqlx::query("SELECT * FROM inbox_dispatches WHERE id=$1 FOR UPDATE")
        .bind(ticket)
        .fetch_one(&mut **tx)
        .await?;
    ensure!(
        r.try_get::<DateTime<Utc>, _>("expires_at")? > Utc::now()
            && r.try_get::<String, _>("status")? != "cancelled",
        "dispatch expired or cancelled"
    );
    let home = r.try_get("home_instance_id")?;
    let principal = r.try_get("principal_id")?;
    let conversation = r.try_get("conversation_id")?;
    let c = owner(tx, home, principal, conversation).await?;
    ensure!(!c.try_get::<bool, _>("paused")?, "conversation paused");
    let (g, gr, mr) =
        member_live(tx, home, principal, conversation, r.try_get("member_id")?).await?;
    ensure!(
        g.id == r.try_get::<Uuid, _>("grant_id")?
            && gr == r.try_get::<i64, _>("grant_revision")?
            && mr == r.try_get::<i64, _>("member_revision")?,
        "authority changed"
    );
    ensure!(
        !sqlx::query_scalar::<_, bool>("SELECT cancelled FROM inbox_roots WHERE id=$1")
            .bind(r.try_get::<Uuid, _>("root_message_id")?)
            .fetch_one(&mut **tx)
            .await?,
        "response cancelled"
    );
    Ok((r, g))
}
pub async fn redeem_dispatch(
    tx: &mut Transaction<'_, Postgres>,
    ticket: Uuid,
    task: Uuid,
    hash: &str,
    home: Uuid,
) -> Result<()> {
    let (r, _) = ticket_live(tx, ticket).await?;
    ensure!(
        r.try_get::<Uuid, _>("home_instance_id")? == home
            && r.try_get::<String, _>("request_digest")? == hash
            && digest(&r.try_get::<Value, _>("request")?)? == hash,
        "dispatch binding mismatch"
    );
    ensure!(
        r.try_get::<Option<Uuid>, _>("task_id")?
            .is_none_or(|id| id == task),
        "ticket already redeemed"
    );
    sqlx::query("INSERT INTO inbox_task_origins(task_id,home_instance_id,principal_id,conversation_id,member_id,root_message_id,grant_id,ticket_id) SELECT $2,home_instance_id,principal_id,conversation_id,member_id,root_message_id,grant_id,id FROM inbox_dispatches WHERE id=$1 ON CONFLICT(task_id) DO NOTHING").bind(ticket).bind(task).execute(&mut **tx).await?;
    sqlx::query("UPDATE inbox_dispatches SET task_id=$2,status='dispatched' WHERE id=$1")
        .bind(ticket)
        .bind(task)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE inbox_messages SET delivery='confirmed' WHERE root_id=$1 AND role='human'")
        .bind(r.try_get::<Uuid, _>("root_message_id")?)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub async fn authorize_task_origin(
    tx: &mut Transaction<'_, Postgres>,
    task: Uuid,
    capability: &str,
    resource: &ResourceScope,
    extra: Usage,
) -> Result<()> {
    let Some(row) =
        sqlx::query("SELECT ticket_id,root_message_id FROM inbox_task_origins WHERE task_id=$1")
            .bind(task)
            .fetch_optional(&mut **tx)
            .await?
    else {
        return Ok(());
    };
    let (ticket, g) = ticket_live(tx, row.try_get("ticket_id")?).await?;
    let req: Value = ticket.try_get("request")?;
    let (actual_mandate, cap, res, _) = template(&req)?;
    project_access(tx, g.principal_id, actual_mandate).await?;
    ensure!(
        cap == capability && res == *resource,
        "task exceeds invocation authority"
    );
    let root = row.try_get::<Uuid, _>("root_message_id")?;
    let r = sqlx::query(
        "SELECT budget,used_microusd,used_calls,deadline FROM inbox_roots WHERE id=$1 FOR UPDATE",
    )
    .bind(root)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        r.try_get::<DateTime<Utc>, _>("deadline")? > Utc::now() && g.expires_at > Utc::now(),
        "response expired"
    );
    let budget: Budget = serde_json::from_value(r.try_get("budget")?)?;
    let used = Usage {
        micro_usd: u64::try_from(r.try_get::<i64, _>("used_microusd")?)?,
        calls: u32::try_from(r.try_get::<i64, _>("used_calls")?)?,
        turns: 0,
    }
    .checked_add(Usage { turns: 0, ..extra })?;
    budget.check(used)?;
    if extra.micro_usd != 0 || extra.calls != 0 {
        sqlx::query("UPDATE inbox_roots SET used_microusd=$2,used_calls=$3 WHERE id=$1")
            .bind(root)
            .bind(i64::try_from(used.micro_usd)?)
            .bind(i64::from(used.calls))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
pub async fn inherit_task_origin(
    tx: &mut Transaction<'_, Postgres>,
    parent: Uuid,
    child: Uuid,
) -> Result<()> {
    sqlx::query("INSERT INTO inbox_task_origins(task_id,home_instance_id,principal_id,conversation_id,member_id,root_message_id,grant_id,ticket_id) SELECT $2,home_instance_id,principal_id,conversation_id,member_id,root_message_id,grant_id,ticket_id FROM inbox_task_origins WHERE task_id=$1 ON CONFLICT(task_id) DO NOTHING").bind(parent).bind(child).execute(&mut **tx).await?;
    Ok(())
}

pub async fn reject_dispatch(pool: &PgPool, id: Uuid, home: Uuid, _reason: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    admission(&mut tx).await?;
    let root:Option<Uuid>=sqlx::query_scalar("UPDATE inbox_dispatches SET status='failed' WHERE id=$1 AND home_instance_id=$2 AND task_id IS NULL AND status='queued' RETURNING root_message_id").bind(id).bind(home).fetch_optional(&mut *tx).await?;
    if let Some(root) = root {
        sqlx::query(
            "UPDATE inbox_messages SET delivery='failed' WHERE root_id=$1 AND role='human'",
        )
        .bind(root)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn record_task_result(
    pool: &PgPool,
    task: Uuid,
    status: &str,
    question: Option<&str>,
    evidence: Option<&Value>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    admission(&mut tx).await?;
    let Some(o)=sqlx::query("SELECT o.*,t.payload FROM inbox_task_origins o JOIN agency_tasks t ON t.id=o.task_id WHERE o.task_id=$1").bind(task).fetch_optional(&mut *tx).await? else{return Ok(());};
    let conversation = o.try_get::<Uuid, _>("conversation_id")?;
    let member = o.try_get::<Uuid, _>("member_id")?;
    let root = o.try_get::<Uuid, _>("root_message_id")?;
    let reply = evidence.and_then(|v| v.get("conversation_reply")).cloned();
    let response = reply
        .as_ref()
        .map(|v| {
            serde_json::from_value::<codifix_agency::conversation::ConversationReply>(v.clone())
        })
        .transpose()?;
    if let Some(response) = &response {
        let request: Value = sqlx::query_scalar("SELECT request FROM inbox_dispatches WHERE id=$1")
            .bind(o.try_get::<Uuid, _>("ticket_id")?)
            .fetch_one(&mut *tx)
            .await?;
        let turn: codifix_agency::conversation::ConversationTurn =
            serde_json::from_value(turn_payload(&request).clone())?;
        response.validate_for(&turn)?;
    }
    let question = response
        .as_ref()
        .and_then(|r| r.question.as_deref())
        .or(question);
    let cancel_requested = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM inbox_task_commands c WHERE c.task_id=$1 AND c.kind='cancel' AND (NOT c.done OR EXISTS(SELECT 1 FROM agency_remote_tasks r WHERE r.task_id=c.task_id AND r.cancel_requested AND NOT r.cancel_completed)))",
    ).bind(task).fetch_one(&mut *tx).await?;
    let hash = digest(
        &json!({"status":status,"question":question,"evidence":evidence,"cancel_requested":cancel_requested}),
    )?;
    let old: Option<String> =
        sqlx::query_scalar("SELECT result_digest FROM inbox_task_cards WHERE task_id=$1")
            .bind(task)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    if old.as_deref() == Some(&hash) {
        return Ok(());
    }
    let payload: Value = o.try_get("payload")?;
    let card = TaskCard {
        id: task,
        member_id: member,
        status: if question.is_some() {
            "input_required".into()
        } else {
            status.into()
        },
        objective: payload["objective"]
            .as_str()
            .unwrap_or("Specialist task")
            .into(),
        question: question.map(str::to_owned),
        result: evidence.cloned(),
        artifacts: evidence
            .and_then(|v| v.get("artifacts"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        cancel_requested,
        root_id: root,
    };
    sqlx::query("INSERT INTO inbox_task_cards(task_id,conversation_id,document,result_digest) VALUES($1,$2,$3,$4) ON CONFLICT(task_id) DO UPDATE SET document=excluded.document,result_digest=excluded.result_digest").bind(task).bind(conversation).bind(serde_json::to_value(card)?).bind(hash).execute(&mut *tx).await?;
    if let Some(response) = response {
        let event = Uuid::new_v5(&task, b"conversation-reply");
        if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM inbox_messages WHERE id=$1)")
            .bind(event)
            .fetch_one(&mut *tx)
            .await?
        {
            let body = if let Some(q) = &response.question {
                format!("{}\n\n{}", response.reply, q)
            } else {
                response.reply.clone()
            };
            append(
                &mut tx,
                conversation,
                event,
                "specialist",
                Some(member),
                root,
                &body,
                "markdown",
                None,
                &[],
                &digest(&response)?,
                "confirmed",
            )
            .await?;
            sqlx::query("UPDATE inbox_messages SET source_ticket_id=$2 WHERE id=$1")
                .bind(event)
                .bind(o.try_get::<Uuid, _>("ticket_id")?)
                .execute(&mut *tx)
                .await?;
            // A late result is visible to its owner, but never authorizes new work.
            // Rejected SQL admissions must not poison the result transaction.
            sqlx::query("SAVEPOINT inbox_result_authority")
                .execute(&mut *tx)
                .await?;
            let live = ticket_live(&mut tx, o.try_get("ticket_id")?).await.is_ok();
            if !live {
                sqlx::query("ROLLBACK TO SAVEPOINT inbox_result_authority")
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("RELEASE SAVEPOINT inbox_result_authority")
                .execute(&mut *tx)
                .await?;
            if live {
                let home = o.try_get("home_instance_id")?;
                let principal = o.try_get("principal_id")?;
                for (index, action) in response.actions.iter().enumerate() {
                    sqlx::query("SAVEPOINT inbox_proposal")
                        .execute(&mut *tx)
                        .await?;
                    let result = enqueue_action(
                        &mut tx,
                        home,
                        principal,
                        conversation,
                        member,
                        root,
                        Uuid::new_v5(&event, format!("action:{index}").as_bytes()),
                        action,
                    )
                    .await;
                    if result.is_err() {
                        sqlx::query("ROLLBACK TO SAVEPOINT inbox_proposal")
                            .execute(&mut *tx)
                            .await?;
                        append_denial(
                            &mut tx,
                            conversation,
                            event,
                            root,
                            &format!("action:{index}"),
                        )
                        .await?;
                    }
                    sqlx::query("RELEASE SAVEPOINT inbox_proposal")
                        .execute(&mut *tx)
                        .await?;
                }
                for (index, peer) in response.peer_requests.iter().enumerate() {
                    sqlx::query("SAVEPOINT inbox_proposal")
                        .execute(&mut *tx)
                        .await?;
                    let result = async {
                        let target: Uuid = peer.specialist_id.parse()?;
                        ensure!(target != member, "self peer request denied");
                        enqueue_turn(
                            &mut tx,
                            home,
                            principal,
                            conversation,
                            target,
                            root,
                            Uuid::new_v5(&event, format!("peer:{index}").as_bytes()),
                            Some((&peer.message, o.try_get("ticket_id")?)),
                        )
                        .await
                    }
                    .await;
                    if result.is_err() {
                        sqlx::query("ROLLBACK TO SAVEPOINT inbox_proposal")
                            .execute(&mut *tx)
                            .await?;
                        append_denial(&mut tx, conversation, event, root, &format!("peer:{index}"))
                            .await?;
                    }
                    sqlx::query("RELEASE SAVEPOINT inbox_proposal")
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
    }
    tx.commit().await?;
    Ok(())
}
async fn append_denial(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    event: Uuid,
    root: Uuid,
    key: &str,
) -> Result<()> {
    let id = Uuid::new_v5(&event, key.as_bytes());
    append(tx,conversation,id,"system",None,root,"A proposed action or peer turn could not be admitted under the current authority, disclosure boundary or remaining budget.","text",None,&[],&id.to_string(),"confirmed").await
}
#[allow(clippy::too_many_arguments)]
async fn enqueue_action(
    tx: &mut Transaction<'_, Postgres>,
    home: Uuid,
    principal: Uuid,
    conversation: Uuid,
    member: Uuid,
    root: Uuid,
    id: Uuid,
    action: &codifix_agency::conversation::ActionProposal,
) -> Result<()> {
    let (g, gr, mr) = member_live(tx, home, principal, conversation, member).await?;
    let a = g
        .actions
        .iter()
        .find(|a| a.key == action.key)
        .ok_or_else(|| anyhow::anyhow!("unknown action"))?;
    let mut request = a.request.clone();
    let kind = request["payload"]["kind"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("action kind missing"))?
        .to_owned();
    ensure!(
        matches!(
            kind.as_str(),
            "inspect" | "plan_prepare" | "plan_apply" | "plan_inspect" | "openbao_recover"
        ),
        "action kind denied"
    );
    let mut payload = action
        .payload
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("action payload required"))?;
    ensure!(
        payload.get("kind").is_none_or(|v| v == &kind),
        "action kind changed"
    );
    ensure!(
        ![
            "delegation_trace",
            "execution_lease_seconds",
            "source_destinations"
        ]
        .iter()
        .any(|key| payload.contains_key(*key)),
        "reserved action fields"
    );
    payload.insert("kind".into(), json!(kind));
    let deadline: DateTime<Utc> =
        sqlx::query_scalar("SELECT deadline FROM inbox_roots WHERE id=$1 AND NOT cancelled")
            .bind(root)
            .fetch_one(&mut **tx)
            .await?;
    request["request_id"] = json!(id.to_string());
    request["deadline"] = json!(deadline.min(g.expires_at));
    request["parent_id"] = Value::Null;
    request["payload"] = json!(payload);
    let (mid, cap, resource, _) = template(&request)?;
    check_mandate(tx, mid, &cap, &resource, home).await?;
    insert_ticket(
        tx,
        id,
        home,
        principal,
        conversation,
        member,
        mr,
        &g,
        gr,
        root,
        &request,
        deadline,
    )
    .await
}
impl Store {
    pub async fn cancel(&self, principal: Uuid, conversation: Uuid, task: Uuid) -> Result<Detail> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        owner(&mut tx, self.home_instance_id, principal, conversation).await?;
        let root:Uuid=sqlx::query_scalar("SELECT root_message_id FROM inbox_task_origins WHERE task_id=$1 AND conversation_id=$2 AND principal_id=$3 AND home_instance_id=$4").bind(task).bind(conversation).bind(principal).bind(self.home_instance_id).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE inbox_roots SET cancelled=true WHERE id=$1")
            .bind(root)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE inbox_dispatches SET status='cancelled' WHERE root_message_id=$1 AND task_id IS NULL").bind(root).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO inbox_task_commands(id,task_id,conversation_id,kind,body) SELECT gen_random_uuid(),task_id,conversation_id,'cancel','{}'::jsonb FROM inbox_task_origins WHERE root_message_id=$1").bind(root).execute(&mut *tx).await?;
        sqlx::query("UPDATE inbox_task_cards SET document=jsonb_set(document,'{cancel_requested}','true'::jsonb) WHERE task_id IN(SELECT task_id FROM inbox_task_origins WHERE root_message_id=$1)").bind(root).execute(&mut *tx).await?;
        tx.commit().await?;
        self.detail(principal, conversation).await
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
        owner(&mut tx, self.home_instance_id, principal, conversation).await?;
        let o=sqlx::query("SELECT o.root_message_id,o.member_id,c.document FROM inbox_task_origins o JOIN inbox_task_cards c ON c.task_id=o.task_id WHERE o.task_id=$1 AND o.conversation_id=$2 AND o.principal_id=$3").bind(task).bind(conversation).bind(principal).fetch_one(&mut *tx).await?;
        let root = o.try_get("root_message_id")?;
        let member = o.try_get("member_id")?;
        tx.commit().await?;
        self.send_inner(
            principal,
            conversation,
            &SendMessage {
                client_message_id,
                body: answer,
                format: "text".into(),
                artifact: None,
                recipient_member_ids: vec![member],
            },
            Some((task, root)),
        )
        .await
    }
}
