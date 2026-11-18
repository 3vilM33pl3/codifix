use crate::config::Peer;
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use codifix_agency::{
    Budget, Mandate, MandateStatus, ResourceScope, TaskContract, TaskStatus, Usage,
};
use codifix_record::JobKind;
use codifix_store::jobs::{self, JobActor, NewJob};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    pub request_id: String,
    pub mandate_id: Uuid,
    pub specialist_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub capability: String,
    pub resource: ResourceScope,
    pub deadline: DateTime<Utc>,
    pub budget: Budget,
    pub payload: Value,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredTask {
    pub contract: TaskContract,
    pub request: Submit,
    pub requester: String,
    pub project_id: Uuid,
    pub revision: i64,
    pub used_micro_usd: u64,
    pub used_calls: u32,
    pub used_turns: u32,
}
#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
}

impl Store {
    /// Metadata-only opt-in. Schema provisioning remains a separate operator action.
    pub async fn heartbeat(&self, cfg: &crate::config::Config) -> Result<()> {
        if !cfg.observation_enabled {
            return Ok(());
        }
        let specialists = cfg
            .specialists
            .iter()
            .map(|s| s.specialist_id.clone())
            .take(100)
            .collect::<Vec<_>>();
        let peers = cfg
            .peers
            .iter()
            .filter(|p| !p.operator && p.instance_id != cfg.instance_id)
            .map(|p| p.id.clone())
            .take(100)
            .collect::<Vec<_>>();
        sqlx::query("INSERT INTO agency_runtime_heartbeats(instance_id,version,enabled_specialists,enrolled_peers) VALUES($1,$2,$3,$4) ON CONFLICT(instance_id) DO UPDATE SET version=excluded.version,enabled_specialists=excluded.enabled_specialists,enrolled_peers=excluded.enrolled_peers,observed_at=clock_timestamp()")
            .bind(cfg.instance_id).bind(env!("CARGO_PKG_VERSION")).bind(specialists).bind(peers).execute(&self.pool).await?;
        Ok(())
    }
}

pub struct PlanAdmission<'a> {
    pub task_id: Uuid,
    pub revision: i64,
    pub plan_id: Uuid,
    pub plan_digest: &'a str,
    pub lease: &'a jobs::JobLease,
    pub usage: Usage,
    pub intent: Option<&'a codifix_store::effects::EffectIntent>,
}

pub fn fingerprint(value: &impl Serialize) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
async fn admission(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('agency-admission',0))")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn mandate_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(Mandate, Uuid, Usage)> {
    let row = sqlx::query("SELECT * FROM agency_mandates WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    let mut m: Mandate = serde_json::from_value(row.try_get("document")?)?;
    ensure!(!row.try_get::<bool, _>("revoked")?, "mandate revoked");
    ensure!(!row.try_get::<bool, _>("paused")?, "mandate paused");
    m.status = MandateStatus::Active;
    m.validate(Utc::now())?;
    let used = Usage {
        micro_usd: u64::try_from(
            row.try_get::<i64, _>("used_microusd")? + row.try_get::<i64, _>("reserved_microusd")?,
        )?,
        calls: u32::try_from(
            row.try_get::<i64, _>("used_calls")? + row.try_get::<i64, _>("reserved_calls")?,
        )?,
        turns: u32::try_from(
            row.try_get::<i64, _>("used_turns")? + row.try_get::<i64, _>("reserved_turns")?,
        )?,
    };
    // Immutable parent links; bounded traversal prevents malformed/cyclic records.
    let mut parent = m.parent_id;
    let mut seen = std::collections::BTreeSet::from([m.id]);
    while let Some(id) = parent {
        ensure!(
            seen.len() <= 16 && seen.insert(id),
            "invalid mandate ancestry"
        );
        let ancestor =
            sqlx::query("SELECT document,paused,revoked FROM agency_mandates WHERE id=$1")
                .bind(id)
                .fetch_one(&mut **tx)
                .await?;
        ensure!(
            !ancestor.try_get::<bool, _>("paused")? && !ancestor.try_get::<bool, _>("revoked")?,
            "ancestor inactive"
        );
        let a: Mandate = serde_json::from_value(ancestor.try_get("document")?)?;
        a.validate(Utc::now())?;
        parent = a.parent_id;
    }
    Ok((m, row.try_get("project_id")?, used))
}
impl Store {
    pub async fn install_mandate(
        &self,
        project: Uuid,
        mandate: &Mandate,
        peer: &Peer,
        instance: Uuid,
    ) -> Result<()> {
        mandate.validate(Utc::now())?;
        ensure!(
            mandate.budget.micro_usd <= i64::MAX as u64 && mandate.budget.delegation_depth <= 16,
            "budget out of range"
        );
        ensure!(
            mandate.subject_instance_id == instance,
            "mandate must target this runtime"
        );
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        if let Some(parent) = mandate.parent_id {
            ensure!(
                peer.operator || peer.mandates.contains(&parent),
                "delegation denied"
            );
            let (p, parent_project, usage) = mandate_tx(&mut tx, parent).await?;
            ensure!(
                parent_project == project
                    && (peer.operator || peer.instance_id == p.subject_instance_id),
                "delegation identity mismatch"
            );
            p.validate_child(mandate, usage, Utc::now())?;
            // The parent allocation stays reserved for the child's lifetime;
            // revocation does not silently refund effects with uncertain outcomes.
            sqlx::query("UPDATE agency_mandates SET reserved_microusd=reserved_microusd+$2,reserved_calls=reserved_calls+$3,reserved_turns=reserved_turns+$4 WHERE id=$1")
                .bind(parent).bind(mandate.budget.micro_usd as i64).bind(i64::from(mandate.budget.calls)).bind(i64::from(mandate.budget.turns)).execute(&mut *tx).await?;
        } else {
            ensure!(
                peer.operator && mandate.issuer_instance_id == instance,
                "root grants require local operator"
            );
        }
        sqlx::query("INSERT INTO agency_mandates(id,project_id,subject_id,parent_id,document) VALUES($1,$2,$3,$4,$5)")
            .bind(mandate.id).bind(project).bind(&mandate.specialist_id).bind(mandate.parent_id).bind(serde_json::to_value(mandate)?).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn control_mandate(&self, id: Uuid, action: &str, peer: &Peer) -> Result<()> {
        ensure!(peer.operator, "operator required");
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let sql = match action {
            "pause" => "UPDATE agency_mandates SET paused=true,revision=revision+1 WHERE id=$1",
            "resume" => {
                "UPDATE agency_mandates SET paused=false,revision=revision+1 WHERE id=$1 AND NOT revoked"
            }
            "revoke" => "UPDATE agency_mandates SET revoked=true,revision=revision+1 WHERE id=$1",
            _ => anyhow::bail!("unknown control action"),
        };
        ensure!(
            sqlx::query(sql)
                .bind(id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1,
            "mandate unavailable"
        );
        tx.commit().await?;
        Ok(())
    }
    pub async fn submit(&self, req: &Submit, peer: &Peer) -> Result<StoredTask> {
        self.submit_inner(req, peer, None, false).await
    }

    /// Only trusted host configuration can opt authenticated peer work into capture.
    pub async fn submit_observed(
        &self,
        req: &Submit,
        peer: &Peer,
        capture: bool,
    ) -> Result<StoredTask> {
        self.submit_inner(req, peer, None, capture).await
    }

    /// A same-database dispatch ticket is the only way to obtain this internal
    /// caller. It has no HTTP credential and no operator authority.
    pub async fn submit_inbox(&self, req: &Submit, ticket: Uuid, home: Uuid) -> Result<StoredTask> {
        let peer = Peer {
            id: format!("inbox-gateway:{home}"),
            instance_id: home,
            token_env: String::new(),
            operator: false,
            mandates: std::collections::BTreeSet::from([req.mandate_id]),
            endpoint: None,
            outbound_token_env: None,
        };
        self.submit_inner(req, &peer, Some((ticket, home)), false)
            .await
    }

    async fn submit_inner(
        &self,
        req: &Submit,
        peer: &Peer,
        inbox: Option<(Uuid, Uuid)>,
        capture: bool,
    ) -> Result<StoredTask> {
        ensure!(
            peer.operator || peer.mandates.contains(&req.mandate_id),
            "peer has no grant"
        );
        ensure!(
            !req.request_id.is_empty()
                && req.request_id.len() <= 128
                && req.objective.len() <= 8192
                && req.acceptance_criteria.len() <= 32,
            "invalid request bounds"
        );
        ensure!(
            req.deadline > Utc::now() && req.deadline <= Utc::now() + chrono::Duration::days(7),
            "invalid deadline"
        );
        ensure!(
            req.budget.micro_usd <= i64::MAX as u64 && req.budget.turns > 0,
            "invalid task budget"
        );
        ensure!(
            serde_json::to_vec(req)?.len() <= 262144,
            "task payload too large"
        );
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let existing: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id,request_digest FROM agency_tasks WHERE requester=$1 AND request_id=$2",
        )
        .bind(&peer.id)
        .bind(&req.request_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, value)) = existing {
            ensure!(
                value == fingerprint(req)?,
                "request id reused with different input"
            );
            if let Some((ticket, home)) = inbox {
                codifix_inbox::store::redeem_dispatch(
                    &mut tx,
                    ticket,
                    id,
                    &fingerprint(&serde_json::to_value(req)?)?,
                    home,
                )
                .await?;
            }
            tx.commit().await?;
            return self.get(id, peer).await;
        }
        let (m, project, usage) = mandate_tx(&mut tx, req.mandate_id).await?;
        ensure!(
            m.specialist_id == req.specialist_id,
            "specialist binding mismatch"
        );
        if req.capability == "conversation_turn" {
            let turn: codifix_agency::conversation::ConversationTurn =
                serde_json::from_value(req.payload["turn"].clone())?;
            ensure!(
                req.payload["kind"] == "conversation_turn",
                "conversation operation mismatch"
            );
            turn.validate_recipient(peer.instance_id, m.subject_instance_id, &req.specialist_id)?;
        }
        ensure!(req.deadline <= m.expires_at, "task exceeds mandate expiry");
        m.authorize(&req.capability, &req.resource, None, usage, Utc::now())?;
        m.budget.remaining(usage)?.check(Usage {
            micro_usd: req.budget.micro_usd,
            calls: req.budget.calls,
            turns: req.budget.turns,
        })?;
        ensure!(
            req.budget.delegation_depth <= m.budget.delegation_depth,
            "delegation depth exceeded"
        );
        if let Some(parent) = req.parent_id {
            let p = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1 FOR UPDATE")
                .bind(parent)
                .fetch_one(&mut *tx)
                .await?;
            ensure!(
                p.try_get::<String, _>("requester")? == peer.id,
                "parent task belongs to another requester"
            );
            ensure!(
                m.parent_id == Some(p.try_get("mandate_id")?),
                "child task requires attenuated child mandate"
            );
            let contract: TaskContract = serde_json::from_value(p.try_get("contract")?)?;
            ensure!(!contract.status.is_terminal(), "parent task closed");
            let parent_request: Submit = serde_json::from_value(p.try_get("payload")?)?;
            ensure!(
                req.deadline <= parent_request.deadline
                    && parent_request.budget.delegation_depth > 0
                    && req.budget.delegation_depth < parent_request.budget.delegation_depth,
                "parent task deadline or depth exceeded"
            );
            let parent_used = Usage {
                micro_usd: u64::try_from(
                    p.try_get::<i64, _>("used_microusd")?
                        + p.try_get::<i64, _>("reserved_microusd")?,
                )?,
                calls: u32::try_from(
                    p.try_get::<i64, _>("used_calls")? + p.try_get::<i64, _>("reserved_calls")?,
                )?,
                turns: u32::try_from(
                    p.try_get::<i64, _>("used_turns")? + p.try_get::<i64, _>("reserved_turns")?,
                )?,
            };
            parent_request.budget.remaining(parent_used)?.check(Usage {
                micro_usd: req.budget.micro_usd,
                calls: req.budget.calls,
                turns: req.budget.turns,
            })?;
            sqlx::query("UPDATE agency_tasks SET reserved_microusd=reserved_microusd+$2,reserved_calls=reserved_calls+$3,reserved_turns=reserved_turns+$4 WHERE id=$1").bind(parent).bind(req.budget.micro_usd as i64).bind(i64::from(req.budget.calls)).bind(i64::from(req.budget.turns)).execute(&mut *tx).await?;
        }
        let active:i64=sqlx::query_scalar("SELECT count(*) FROM agency_tasks WHERE requester=$1 AND contract->>'status' IN ('submitted','working','input_required','plan_review_required')").bind(&peer.id).fetch_one(&mut *tx).await?;
        ensure!(active < 128, "too many active tasks for this requester");
        let execution_lease = req
            .payload
            .get("execution_lease_seconds")
            .map(|v| {
                v.as_i64()
                    .filter(|n| (5..=60).contains(n))
                    .ok_or_else(|| anyhow::anyhow!("invalid execution lease"))
            })
            .transpose()?;
        let id = Uuid::new_v4();
        let contract = TaskContract {
            id,
            mandate_id: m.id,
            specialist_id: m.specialist_id.clone(),
            objective: req.objective.clone(),
            acceptance_criteria: req.acceptance_criteria.clone(),
            revision: 1,
            status: TaskStatus::Submitted,
            clarification: None,
            plan_revision: None,
            outcome: None,
            updated_at: Utc::now(),
        };
        contract.validate()?;
        sqlx::query("INSERT INTO agency_tasks(id,project_id,mandate_id,requester,request_id,parent_id,contract,payload,request_digest,execution_lease_seconds,execution_lease_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,CASE WHEN $10::integer IS NULL THEN NULL ELSE clock_timestamp()+make_interval(secs=>$10) END)")
            .bind(id).bind(project).bind(m.id).bind(&peer.id).bind(&req.request_id).bind(req.parent_id).bind(serde_json::to_value(&contract)?).bind(serde_json::to_value(req)?).bind(fingerprint(req)?).bind(execution_lease.map(|n|n as i32)).execute(&mut *tx).await?;
        if let Some((ticket, home)) = inbox {
            codifix_inbox::store::redeem_dispatch(
                &mut tx,
                ticket,
                id,
                &fingerprint(&serde_json::to_value(req)?)?,
                home,
            )
            .await?;
        } else if let Some(parent) = req.parent_id {
            codifix_inbox::store::inherit_task_origin(&mut tx, parent, id).await?;
        }
        // Preserve authenticated admission, never infer identity from a later payload.
        // Inbox and child tasks deliberately have no public peer observation origin.
        if capture
            && inbox.is_none()
            && req.parent_id.is_none()
            && peer.id.len() <= 128
            && req.resource.id.len() <= 128
            && !peer.id.starts_with("inbox-gateway:")
            && [
                "delegate",
                "inspect",
                "plan_prepare",
                "plan_apply",
                "plan_inspect",
                "model_draft",
            ]
            .contains(&req.capability.as_str())
            && !serde_json::to_string(&req.payload)?.contains("conversation_turn")
            && !serde_json::to_string(&req.payload)?.contains("inbox-gateway:")
        {
            let outgoing = peer.operator
                && peer.instance_id == m.subject_instance_id
                && req.capability == "delegate"
                && req.resource.kind == "peer"
                && req.payload["kind"] == "delegate"
                && req.payload["peer"].as_str() == Some(req.resource.id.as_str())
                && [
                    "inspect",
                    "plan_prepare",
                    "plan_apply",
                    "plan_inspect",
                    "model_draft",
                ]
                .contains(&req.payload["request"]["capability"].as_str().unwrap_or(""))
                && req.payload["request"]["capability"].as_str()
                    == req.payload["request"]["payload"]["kind"].as_str();
            let incoming = !peer.operator
                && peer.instance_id != m.subject_instance_id
                && req.capability != "delegate"
                && req.payload["kind"].as_str() == Some(req.capability.as_str());
            if outgoing || incoming {
                sqlx::query("INSERT INTO agency_task_observation_origins(task_id,home_instance_id,requester,requester_instance_id,requester_operator,peer,direction) VALUES($1,$2,$3,$4,$5,$6,$7)")
                    .bind(id).bind(m.subject_instance_id).bind(&peer.id).bind(peer.instance_id).bind(peer.operator)
                    .bind(if outgoing { &req.resource.id } else { &peer.id }).bind(if outgoing { "outgoing" } else { "incoming" })
                    .execute(&mut *tx).await?;
            }
        }
        enqueue(&mut tx, project, id, 1).await?;
        tx.commit().await?;
        self.get(id, peer).await
    }
    pub async fn get(&self, id: Uuid, peer: &Peer) -> Result<StoredTask> {
        let row = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        ensure!(
            peer.operator || row.try_get::<String, _>("requester")? == peer.id,
            "task unavailable"
        );
        decode(&row)
    }
    pub async fn get_internal(&self, id: Uuid) -> Result<StoredTask> {
        decode(
            &sqlx::query("SELECT * FROM agency_tasks WHERE id=$1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?,
        )
    }
    pub async fn list(&self, peer: &Peer) -> Result<Vec<StoredTask>> {
        let rows=sqlx::query("SELECT * FROM agency_tasks WHERE $1 OR requester=$2 ORDER BY created_at DESC LIMIT 100").bind(peer.operator).bind(&peer.id).fetch_all(&self.pool).await?;
        rows.iter().map(decode).collect()
    }
    pub async fn authorize(&self, id: Uuid, revision: i64, extra: Usage) -> Result<Mandate> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let m = authorize_tx(&mut tx, id, revision, extra).await?;
        tx.commit().await?;
        Ok(m)
    }
    /// Elect exactly one physical sender atomically with the budget debit and
    /// intent. An uncertain prior attempt cannot be turned into a resend.
    pub async fn admit_effect(
        &self,
        id: Uuid,
        revision: i64,
        usage: Usage,
        intent: &codifix_store::effects::EffectIntent,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let inserted:Option<Uuid>=sqlx::query_scalar("INSERT INTO agency_attempts(id,task_id) VALUES($1,$2) ON CONFLICT DO NOTHING RETURNING id").bind(intent.id).bind(id).fetch_optional(&mut *tx).await?;
        ensure!(
            inserted.is_some(),
            "effect already admitted; reconciliation required"
        );
        if intent.source == "agency-model" {
            let binding: Option<Value> = sqlx::query_scalar("SELECT binding FROM agency_learning_assignments WHERE task_id=$1 AND task_revision=$2").bind(id).bind(revision).fetch_optional(&mut *tx).await?;
            if let Some(binding) = binding {
                let binding: crate::learning::DraftBinding = serde_json::from_value(binding)?;
                let (baseline,canary):(Option<Uuid>,Option<Uuid>)=sqlx::query_as("SELECT baseline_candidate_id,canary_candidate_id FROM agency_learning_heads WHERE specialist_id=$1 AND workload=$2 FOR SHARE").bind(&binding.specialist_id).bind(&binding.workload).fetch_one(&mut *tx).await?;
                ensure!(
                    canary.or(baseline) == Some(binding.candidate_id),
                    "assigned improvement no longer active"
                );
            }
        }
        authorize_tx(&mut tx, id, revision, usage).await?;
        codifix_store::effects::begin(&mut *tx, intent).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Fences the accepted plan head and worker lease in the same transaction as
    /// budget/authority and physical intent. A later head may invalidate future
    /// admissions, but cannot retroactively unsend an admitted operation.
    pub async fn authorize_plan(&self, request: PlanAdmission<'_>) -> Result<()> {
        let mut guard = jobs::begin_result(&self.pool, request.lease).await?;
        let tx = guard.transaction();
        admission(tx).await?;
        ensure!(
            request.lease.job.operation_id == request.task_id,
            "job task mismatch"
        );
        let (mandate, job): (Uuid, Option<Uuid>) =
            sqlx::query_as("SELECT mandate_id,job_id FROM agency_tasks WHERE id=$1")
                .bind(request.task_id)
                .fetch_one(&mut **tx)
                .await?;
        ensure!(job == Some(request.lease.job.id), "task replaced job");
        let head: String = sqlx::query_scalar(
            "SELECT digest FROM agency_plan_heads WHERE plan_id=$1 AND mandate_id=$2 FOR SHARE",
        )
        .bind(request.plan_id)
        .bind(mandate)
        .fetch_one(&mut **tx)
        .await?;
        ensure!(head == request.plan_digest, "plan head changed");
        if let Some(intent) = request.intent {
            let inserted:Option<Uuid>=sqlx::query_scalar("INSERT INTO agency_attempts(id,task_id) VALUES($1,$2) ON CONFLICT DO NOTHING RETURNING id").bind(intent.id).bind(request.task_id).fetch_optional(&mut **tx).await?;
            ensure!(inserted.is_some(), "effect already admitted");
            codifix_store::effects::begin(&mut **tx, intent).await?;
        }
        authorize_tx(tx, request.task_id, request.revision, request.usage).await?;
        jobs::checkpoint_result(guard).await?;
        Ok(())
    }
    pub async fn live_authority(&self, task: &StoredTask) -> Result<()> {
        ensure!(task.request.deadline > Utc::now(), "deadline expired");
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        mandate_tx(&mut tx, task.request.mandate_id).await?;
        let expiry: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT execution_lease_until FROM agency_tasks WHERE id=$1")
                .bind(task.contract.id)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            expiry.is_none_or(|e| e > Utc::now()),
            "execution lease expired"
        );
        task_ancestors(&mut tx, task.request.parent_id).await?;
        codifix_inbox::store::authorize_task_origin(
            &mut tx,
            task.contract.id,
            &task.request.capability,
            &task.request.resource,
            Usage::default(),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    /// Charge the initial activation once even across a recovered worker lease.
    pub async fn start_task(&self, id: Uuid, revision: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let first: Option<Uuid> = sqlx::query_scalar("INSERT INTO agency_task_starts(task_id) VALUES($1) ON CONFLICT DO NOTHING RETURNING task_id").bind(id).fetch_optional(&mut *tx).await?;
        authorize_tx(
            &mut tx,
            id,
            revision,
            Usage {
                turns: u32::from(first.is_some()),
                ..Usage::default()
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn renew_execution_lease(&self, id: Uuid, peer: &Peer) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let row = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let task = decode(&row)?;
        ensure!(
            peer.operator || peer.id == task.requester,
            "task unavailable"
        );
        ensure!(
            !task.contract.status.is_terminal() && task.request.deadline > Utc::now(),
            "task closed"
        );
        let (m, _, _) = mandate_tx(&mut tx, task.request.mandate_id).await?;
        codifix_inbox::store::authorize_task_origin(
            &mut tx,
            task.contract.id,
            &task.request.capability,
            &task.request.resource,
            Usage::default(),
        )
        .await?;
        let seconds: Option<i32> = row.try_get("execution_lease_seconds")?;
        ensure!(seconds.is_some(), "task has no renewable lease");
        sqlx::query("UPDATE agency_tasks SET execution_lease_until=LEAST($2,$3,clock_timestamp()+make_interval(secs=>$4)) WHERE id=$1").bind(id).bind(task.request.deadline).bind(m.expires_at).bind(f64::from(seconds.unwrap_or(5))).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn cancel(&self, id: Uuid, peer: &Peer) -> Result<StoredTask> {
        let mut task = self.get(id, peer).await?;
        if task.contract.status.is_terminal() {
            return Ok(task);
        }
        task.contract.transition(TaskStatus::Canceled, Utc::now())?;
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        ensure!(sqlx::query("UPDATE agency_tasks SET contract=$2,revision=revision+1 WHERE id=$1 AND revision=$3").bind(id).bind(serde_json::to_value(&task.contract)?).bind(task.revision).execute(&mut *tx).await?.rows_affected()==1,"task revision conflict");
        sqlx::query("UPDATE agency_remote_tasks SET cancel_requested=true WHERE task_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.get(id, peer).await
    }
    pub async fn update_contract(&self, task: &StoredTask) -> Result<()> {
        ensure!(sqlx::query("UPDATE agency_tasks SET contract=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$3")
            .bind(task.contract.id).bind(serde_json::to_value(&task.contract)?).bind(task.revision).execute(&self.pool).await?.rows_affected()==1,"task revision conflict");
        Ok(())
    }
    pub async fn respond(
        &self,
        id: Uuid,
        message_id: &str,
        answer: &str,
        replacement: Option<Value>,
        peer: &Peer,
    ) -> Result<StoredTask> {
        let mut tx = self.pool.begin().await?;
        admission(&mut tx).await?;
        let row = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let mut task = decode(&row)?;
        ensure!(
            task.request.capability != "conversation_turn",
            "conversation answers require a new home dispatch ticket"
        );
        ensure!(
            peer.operator || task.requester == peer.id,
            "task unavailable"
        );
        ensure!(task.request.deadline > Utc::now(), "task deadline expired");
        let body = json!({"answer":answer,"replacement":replacement});
        let prior: Option<Value> = sqlx::query_scalar(
            "SELECT body FROM agency_messages WHERE task_id=$1 AND message_id=$2",
        )
        .bind(id)
        .bind(message_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(prior) = prior {
            ensure!(prior == body, "message id conflict");
            tx.commit().await?;
            return self.get(id, peer).await;
        }
        ensure!(
            !message_id.is_empty() && message_id.len() <= 128 && answer.len() <= 8192,
            "invalid message"
        );
        let (m, _, usage) = mandate_tx(&mut tx, task.request.mandate_id).await?;
        m.budget.check(usage.checked_add(Usage {
            turns: 1,
            ..Usage::default()
        })?)?;
        task.request.budget.check(Usage {
            micro_usd: task.used_micro_usd
                + u64::try_from(row.try_get::<i64, _>("reserved_microusd")?)?,
            calls: task.used_calls + u32::try_from(row.try_get::<i64, _>("reserved_calls")?)?,
            turns: task
                .used_turns
                .checked_add(u32::try_from(row.try_get::<i64, _>("reserved_turns")?)?)
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| anyhow::anyhow!("turn limit"))?,
        })?;
        task.contract
            .answer_clarification(answer.into(), Utc::now())?;
        if let Some(payload) = replacement {
            ensure!(
                serde_json::to_vec(&payload)?.len() <= 262144,
                "payload too large"
            );
            let mut payload = payload;
            for key in [
                "delegation_trace",
                "execution_lease_seconds",
                "source_destinations",
            ] {
                if let Some(trace) = task.request.payload.get(key).cloned() {
                    payload
                        .as_object_mut()
                        .ok_or_else(|| anyhow::anyhow!("payload must be object"))?
                        .insert(key.into(), trace);
                }
            }
            task.request.payload = payload;
        }
        sqlx::query(
            "INSERT INTO agency_messages(task_id,message_id,sender,body) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(message_id)
        .bind(&peer.id)
        .bind(body)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE agency_tasks SET contract=$2,payload=$3,revision=revision+1,used_turns=used_turns+1,execution_lease_until=CASE WHEN execution_lease_seconds IS NULL THEN NULL ELSE LEAST((payload->>'deadline')::timestamptz,clock_timestamp()+make_interval(secs=>execution_lease_seconds)) END WHERE id=$1").bind(id).bind(serde_json::to_value(&task.contract)?).bind(serde_json::to_value(&task.request)?).execute(&mut *tx).await?;
        sqlx::query("UPDATE agency_mandates SET used_turns=used_turns+1 WHERE id=$1")
            .bind(m.id)
            .execute(&mut *tx)
            .await?;
        enqueue(&mut tx, task.project_id, id, task.revision + 1).await?;
        tx.commit().await?;
        self.get(id, peer).await
    }
}
async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    id: Uuid,
    revision: i64,
) -> Result<()> {
    let job = Uuid::new_v5(&id, format!("revision:{revision}").as_bytes());
    jobs::enqueue(
        tx,
        &NewJob {
            id: job,
            project_id: project,
            operation_id: id,
            kind: JobKind::SpecialistTask,
            work_key: format!("agency:{id}:{revision}"),
            actor: JobActor::System("agency".into()),
            input: json!({"task_id":id,"revision":revision}),
            dependencies: vec![],
        },
    )
    .await?;
    sqlx::query("UPDATE agency_tasks SET job_id=$2 WHERE id=$1")
        .bind(id)
        .bind(job)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub(crate) fn decode(row: &sqlx::postgres::PgRow) -> Result<StoredTask> {
    Ok(StoredTask {
        contract: serde_json::from_value(row.try_get("contract")?)?,
        request: serde_json::from_value(row.try_get("payload")?)?,
        requester: row.try_get("requester")?,
        project_id: row.try_get("project_id")?,
        revision: row.try_get("revision")?,
        used_micro_usd: u64::try_from(row.try_get::<i64, _>("used_microusd")?)?,
        used_calls: u32::try_from(row.try_get::<i64, _>("used_calls")?)?,
        used_turns: u32::try_from(row.try_get::<i64, _>("used_turns")?)?,
    })
}

async fn authorize_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    revision: i64,
    extra: Usage,
) -> Result<Mandate> {
    let row = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    let task = decode(&row)?;
    codifix_inbox::store::authorize_task_origin(
        tx,
        id,
        &task.request.capability,
        &task.request.resource,
        extra,
    )
    .await?;
    ensure!(
        row.try_get::<Option<DateTime<Utc>>, _>("execution_lease_until")?
            .is_none_or(|expiry| expiry > Utc::now()),
        "execution lease expired"
    );
    ensure!(
        task.revision == revision
            && task.contract.status == TaskStatus::Working
            && task.request.deadline > Utc::now(),
        "task no longer executable"
    );
    task_ancestors(tx, task.request.parent_id).await?;
    let total = Usage {
        micro_usd: task.used_micro_usd
            + u64::try_from(row.try_get::<i64, _>("reserved_microusd")?)?,
        calls: task.used_calls + u32::try_from(row.try_get::<i64, _>("reserved_calls")?)?,
        turns: task.used_turns + u32::try_from(row.try_get::<i64, _>("reserved_turns")?)?,
    }
    .checked_add(extra)?;
    task.request.budget.check(total)?;
    let (mut m, _, usage) = mandate_tx(tx, task.request.mandate_id).await?;
    // Enrolled senders may narrow, never broaden, the destination grant. The
    // bound survives clarification and is propagated on every further delegation.
    if let Some(value) = task.request.payload.get("source_destinations") {
        let permitted: std::collections::BTreeSet<codifix_agency::ModelDestination> =
            serde_json::from_value(value.clone())?;
        m.destinations = m.destinations.intersection(&permitted).cloned().collect();
    }
    m.authorize(
        &task.request.capability,
        &task.request.resource,
        None,
        usage.checked_add(extra)?,
        Utc::now(),
    )?;
    sqlx::query("UPDATE agency_mandates SET used_microusd=used_microusd+$2,used_calls=used_calls+$3,used_turns=used_turns+$4 WHERE id=$1")
            .bind(m.id).bind(i64::try_from(extra.micro_usd)?).bind(i64::from(extra.calls)).bind(i64::from(extra.turns)).execute(&mut **tx).await?;
    sqlx::query("UPDATE agency_tasks SET used_microusd=used_microusd+$2,used_calls=used_calls+$3,used_turns=used_turns+$4 WHERE id=$1")
            .bind(id).bind(i64::try_from(extra.micro_usd)?).bind(i64::from(extra.calls)).bind(i64::from(extra.turns)).execute(&mut **tx).await?;
    Ok(m)
}

async fn task_ancestors(
    tx: &mut Transaction<'_, Postgres>,
    mut parent: Option<Uuid>,
) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = parent {
        ensure!(seen.len() < 16 && seen.insert(id), "invalid task ancestry");
        let row = sqlx::query("SELECT * FROM agency_tasks WHERE id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        let task = decode(&row)?;
        ensure!(
            !task.contract.status.is_terminal() && task.request.deadline > Utc::now(),
            "parent task closed"
        );
        ensure!(
            row.try_get::<Option<DateTime<Utc>>, _>("execution_lease_until")?
                .is_none_or(|e| e > Utc::now()),
            "parent execution lease expired"
        );
        parent = task.request.parent_id;
    }
    Ok(())
}
