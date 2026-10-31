use crate::{
    config::Config,
    store::{Store, StoredTask, Submit, fingerprint},
};
use anyhow::{Result, ensure};
use chrono::Utc;
use codifix_agency::{AcceptanceResult, TaskOutcome, TaskStatus, Usage};
use codifix_store::jobs::{self, Completion, JobLease, JobPolicy};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub fn policy() -> JobPolicy {
    JobPolicy {
        concurrent_projects: 4,
        lease_seconds: 120,
        max_attempts: 3,
        ..Default::default()
    }
}
pub async fn run_once(store: &Store, cfg: &Config) -> Result<bool> {
    let Some(lease) = jobs::claim_kinds(
        &store.pool,
        policy(),
        &[codifix_record::JobKind::SpecialistTask],
    )
    .await?
    else {
        return Ok(false);
    };
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
    tick.tick().await;
    let future = run_lease(store, cfg, &lease);
    tokio::pin!(future);
    let result = loop {
        tokio::select! {
            result=&mut future=>break result,
            _=tick.tick()=>{jobs::renew(&store.pool,&lease,policy()).await?;}
        }
    };
    if result.is_err() {
        // No payload/provider errors in operational logs; the private task has a
        // bounded escalation. Unknown external effects are never replayed here.
        if let Ok(mut task) = store.get_internal(lease.job.operation_id).await {
            if task.contract.status == TaskStatus::Working {
                task.contract.clarify("Execution stopped at an authorization, validation, or uncertain-effect boundary. Inspect the private effect ledger before resuming.".into(),Utc::now())?;
                let _ = store.update_contract(&task).await;
            }
        }
    }
    if let Ok(result_guard) = jobs::begin_result(&store.pool, &lease).await {
        jobs::finish_result(
            result_guard,
            if result.is_ok() {
                Completion::Succeeded
            } else {
                Completion::Failure(codifix_record::JobFailureCode::Configuration)
            },
            policy(),
        )
        .await?;
    }
    Ok(true)
}
async fn run_lease(store: &Store, cfg: &Config, lease: &JobLease) -> Result<()> {
    let mut task = store.get_internal(lease.job.operation_id).await?;
    let active_job: Option<Uuid> =
        sqlx::query_scalar("SELECT job_id FROM agency_tasks WHERE id=$1")
            .bind(task.contract.id)
            .fetch_one(&store.pool)
            .await?;
    if active_job != Some(lease.job.id) || task.contract.status.is_terminal() {
        return Ok(());
    }
    if task.contract.status == TaskStatus::Submitted {
        task.contract.transition(TaskStatus::Working, Utc::now())?;
        store.update_contract(&task).await?;
        task = store.get_internal(task.contract.id).await?;
    }
    ensure!(
        task.contract.status == TaskStatus::Working,
        "task awaits input"
    );
    store
        .authorize(task.contract.id, task.revision, Usage::default())
        .await?;
    let kind = task.request.payload["kind"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("operation kind required"))?;
    let expected = match kind {
        "inspect" => "status_observed",
        "plan_prepare" => "plan_valid",
        "plan_apply" => "linear_readback_verified",
        "model_draft" => "model_output_valid_json",
        "openbao_recover" => "openbao_unsealed",
        "delegate" => "delegated_result_reported",
        _ => anyhow::bail!("unsupported specialist operation"),
    };
    ensure!(
        task.request.capability == kind,
        "operation exceeds requested capability"
    );
    if task.request.acceptance_criteria != vec![expected.to_string()] {
        task.contract.clarify(format!("This operation can verify the criterion '{expected}'. Supply a new task with that criterion; arbitrary outcome claims cannot be certified."),Utc::now())?;
        store.update_contract(&task).await?;
        return Ok(());
    }
    let evidence = match kind {
        "inspect" => {
            json!({"observed_at":Utc::now(),"specialist":task.contract.specialist_id,"mandate_active":true})
        }
        "plan_prepare" => match prepare_plan(store, cfg, &task).await? {
            Some(e) => e,
            None => return Ok(()),
        },
        "plan_apply" => apply_plan(store, cfg, &task, lease).await?,
        "model_draft" => {
            let prompt = task.request.payload["prompt"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("prompt required"))?;
            let routing = serde_json::from_value(task.request.payload["routing"].clone())?;
            crate::model::draft(store, cfg, &task, prompt, &routing).await?
        }
        "openbao_recover" => crate::openbao::recover(store, cfg, &task).await?,
        "delegate" => crate::exchange::delegate(store, cfg, &task).await?,
        _ => unreachable!(),
    };
    let reference = format!("agency:task:{}:evidence", task.contract.id);
    let outcome = TaskOutcome {
        summary: format!("Verified {expected}"),
        artifact_refs: vec![reference.clone()],
        evidence_refs: vec![reference.clone()],
        acceptance: vec![AcceptanceResult {
            criterion: expected.into(),
            passed: true,
            evidence_ref: reference,
        }],
    };
    task.contract.finish(outcome, Utc::now())?;
    let mut result = jobs::begin_result(&store.pool, lease).await?;
    let tx = result.transaction();
    ensure!(sqlx::query("UPDATE agency_tasks SET contract=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND revision=$3")
        .bind(task.contract.id).bind(serde_json::to_value(&task.contract)?).bind(task.revision).execute(&mut **tx).await?.rows_affected()==1,"stale task result");
    sqlx::query("INSERT INTO agency_outcomes(task_id,evidence) VALUES($1,$2) ON CONFLICT(task_id) DO NOTHING").bind(task.contract.id).bind(evidence).execute(&mut **tx).await?;
    jobs::checkpoint_result(result).await?;
    Ok(())
}
#[derive(Deserialize)]
struct LinearSettings {
    endpoint: String,
    token_env: String,
    authorities: std::collections::BTreeMap<Uuid, codifix_agency_projects::Authority>,
}
fn linear(
    cfg: &Config,
    task: &StoredTask,
) -> Result<(
    codifix_agency_projects::LinearClient,
    codifix_agency_projects::Authority,
)> {
    let settings: LinearSettings = serde_json::from_value(
        cfg.linear
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Linear is not configured"))?,
    )?;
    let authority = settings
        .authorities
        .get(&task.request.mandate_id)
        .ok_or_else(|| anyhow::anyhow!("no Linear scope for mandate"))?
        .clone();
    let token = std::env::var(&settings.token_env)
        .map_err(|_| anyhow::anyhow!("Linear credential unavailable"))?;
    let client = codifix_agency_projects::LinearClient::new(
        &settings.endpoint,
        &token,
        std::time::Duration::from_secs(30),
    )?;
    Ok((client, authority))
}
async fn prepare_plan(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Option<Value>> {
    let plan: codifix_agency_projects::Plan =
        serde_json::from_value(task.request.payload["plan"].clone())?;
    plan.validate()?;
    if !plan.unresolved_questions.is_empty() {
        let mut task = task.clone();
        task.contract.clarify(
            format!(
                "Questions for {} via {}: {}",
                plan.source.reference,
                plan.source.return_channel,
                plan.unresolved_questions.join("; ")
            ),
            Utc::now(),
        )?;
        store.update_contract(&task).await?;
        return Ok(None);
    }
    let (client, authority) = linear(cfg, task)?;
    codifix_agency_projects::authorize_plan(&plan, &authority)?;
    ensure!(
        task.request.resource
            == codifix_agency::ResourceScope {
                kind: "plan".into(),
                id: plan.id.to_string()
            },
        "plan resource binding mismatch"
    );
    store
        .authorize(
            task.contract.id,
            task.revision,
            Usage {
                calls: 1,
                ..Usage::default()
            },
        )
        .await?;
    let baseline = client.retrieve_plan(&plan).await?;
    let changes = codifix_agency_projects::prepare(&plan, &baseline, &authority)?;
    let id = Uuid::new_v5(&task.contract.id, plan.digest().as_bytes());
    let mut tx = store.pool.begin().await?;
    let head:Option<Uuid>=sqlx::query_scalar("INSERT INTO agency_plan_heads(plan_id,mandate_id,revision,digest,prepared_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(plan_id,mandate_id) DO UPDATE SET revision=$3,digest=$4,prepared_id=$5 WHERE agency_plan_heads.revision<$3 OR (agency_plan_heads.revision=$3 AND agency_plan_heads.digest=$4) RETURNING prepared_id")
        .bind(plan.id).bind(task.request.mandate_id).bind(i64::try_from(plan.revision)?).bind(plan.digest()).bind(id).fetch_optional(&mut *tx).await?;
    ensure!(
        head.is_some(),
        "a newer or conflicting plan revision is already accepted"
    );
    sqlx::query("INSERT INTO agency_prepared_plans(id,mandate_id,plan,changes) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO NOTHING")
        .bind(id).bind(task.request.mandate_id).bind(serde_json::to_value(&plan)?).bind(serde_json::to_value(&changes)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(
        json!({"prepared_id":id,"plan_digest":plan.digest(),"changes":changes,"source":plan.source}),
    ))
}
async fn apply_plan(
    store: &Store,
    cfg: &Config,
    task: &StoredTask,
    lease: &JobLease,
) -> Result<Value> {
    let id: Uuid = serde_json::from_value(task.request.payload["prepared_id"].clone())?;
    let row = sqlx::query("SELECT plan,changes,mandate_id FROM agency_prepared_plans WHERE id=$1")
        .bind(id)
        .fetch_one(&store.pool)
        .await?;
    ensure!(
        row.try_get::<Uuid, _>("mandate_id")? == task.request.mandate_id,
        "prepared plan belongs to another mandate"
    );
    let plan: codifix_agency_projects::Plan = serde_json::from_value(row.try_get("plan")?)?;
    let changes: codifix_agency_projects::PreparedChangeSet =
        serde_json::from_value(row.try_get("changes")?)?;
    let (client, authority) = linear(cfg, task)?;
    ensure!(
        task.request.resource
            == codifix_agency::ResourceScope {
                kind: "plan".into(),
                id: plan.id.to_string()
            },
        "plan resource binding mismatch"
    );
    let mut plan_owner =
        codifix_store::executions::Execution::acquire(&store.pool, plan.id).await?;
    let mut journal = ProjectJournal {
        store: store.clone(),
        task: task.clone(),
        lease: lease.clone(),
        prepared_id: id,
        plan_id: plan.id,
        attempts: std::collections::BTreeMap::new(),
        owner: &mut plan_owner,
    };
    let outcomes = client
        .execute(&plan, &authority, &changes, &mut journal)
        .await?;
    Ok(json!({"prepared_id":id,"plan_digest":plan.digest(),"objects":outcomes}))
}
struct ProjectJournal<'a> {
    owner: &'a mut codifix_store::executions::Execution,
    store: Store,
    task: StoredTask,
    lease: JobLease,
    prepared_id: Uuid,
    plan_id: Uuid,
    attempts: std::collections::BTreeMap<Uuid, Uuid>,
}
impl codifix_agency_projects::StepJournal for ProjectJournal<'_> {
    async fn load(
        &mut self,
        id: Uuid,
    ) -> codifix_agency_projects::Result<Option<codifix_agency_projects::StepReceipt>> {
        let row: Option<(Uuid, Value)> =
            sqlx::query_as("SELECT task_id,receipt FROM agency_steps WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.store.pool)
                .await
                .map_err(|_| journal_error())?;
        row.map(|(_, value)| serde_json::from_value(value).map_err(|_| journal_error()))
            .transpose()
    }
    async fn save(
        &mut self,
        id: Uuid,
        receipt: &codifix_agency_projects::StepReceipt,
    ) -> codifix_agency_projects::Result<()> {
        use codifix_agency_projects::StepPhase;
        let mut tx = self.store.pool.begin().await.map_err(|_| journal_error())?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("agency-step:{id}"))
            .execute(&mut *tx)
            .await
            .map_err(|_| journal_error())?;
        let prior: Option<Value> =
            sqlx::query_scalar("SELECT receipt FROM agency_steps WHERE id=$1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| journal_error())?;
        if let Some(prior) = prior {
            let old: codifix_agency_projects::StepReceipt =
                serde_json::from_value(prior).map_err(|_| journal_error())?;
            let order = |p: StepPhase| match p {
                StepPhase::Prepared => 0,
                StepPhase::Attempted => 1,
                StepPhase::Verified => 2,
            };
            if old.step_digest != receipt.step_digest
                || order(receipt.phase) < order(old.phase)
                || (old.phase == StepPhase::Verified && old != *receipt)
            {
                return Err(journal_error());
            }
        }
        sqlx::query("INSERT INTO agency_steps(id,task_id,receipt) VALUES($1,$2,$3) ON CONFLICT(id) DO UPDATE SET receipt=$3,updated_at=clock_timestamp()")
            .bind(id).bind(self.task.contract.id).bind(serde_json::to_value(receipt).map_err(|_|journal_error())?).execute(&mut *tx).await.map_err(|_|journal_error())?;
        if receipt.phase == StepPhase::Verified {
            let effects:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM external_effects WHERE source='agency-linear' AND resource=$1 AND status IN ('pending','uncertain')").bind(format!("linear-step:{id}")).fetch_all(&mut *tx).await.map_err(|_|journal_error())?;
            for effect in effects {
                codifix_store::effects::conclude_tx(
                    &mut tx,
                    effect,
                    Uuid::new_v4(),
                    codifix_store::effects::EffectStatus::Confirmed,
                    codifix_store::effects::EffectReason::Observed,
                )
                .await
                .map_err(|_| journal_error())?;
            }
        }
        tx.commit().await.map_err(|_| journal_error())?;
        Ok(())
    }
    async fn authorize_step(
        &mut self,
        changes: &codifix_agency_projects::PreparedChangeSet,
        step: &codifix_agency_projects::Step,
    ) -> codifix_agency_projects::Result<()> {
        let result = jobs::begin_result(&self.store.pool, &self.lease)
            .await
            .map_err(|_| journal_error())?;
        jobs::checkpoint_result(result)
            .await
            .map_err(|_| journal_error())?;
        let current: Value =
            sqlx::query_scalar("SELECT changes FROM agency_prepared_plans WHERE id=$1")
                .bind(self.prepared_id)
                .fetch_one(&self.store.pool)
                .await
                .map_err(|_| journal_error())?;
        if current != serde_json::to_value(changes).map_err(|_| journal_error())? {
            return Err(journal_error());
        }
        let head: String = sqlx::query_scalar(
            "SELECT digest FROM agency_plan_heads WHERE plan_id=$1 AND mandate_id=$2",
        )
        .bind(self.plan_id)
        .bind(self.task.request.mandate_id)
        .fetch_one(&self.store.pool)
        .await
        .map_err(|_| journal_error())?;
        if head != changes.plan_digest {
            return Err(journal_error());
        }
        self.owner
            .check_ownership()
            .await
            .map_err(|_| journal_error())?;
        let attempt = Uuid::new_v4();
        self.store
            .admit_effect(
                self.task.contract.id,
                self.task.revision,
                Usage {
                    calls: 1,
                    ..Usage::default()
                },
                &codifix_store::effects::EffectIntent {
                    id: attempt,
                    project_id: Some(self.task.project_id),
                    kind: codifix_store::effects::EffectKind::ProviderRequest,
                    host_id: "agency".into(),
                    resource: format!("linear-step:{}", step.id),
                    input_fingerprint: fingerprint(step).map_err(|_| journal_error())?,
                    actor_id: self.task.contract.specialist_id.clone(),
                    source: "agency-linear".into(),
                },
            )
            .await
            .map_err(|_| journal_error())?;
        self.attempts.insert(step.id, attempt);
        Ok(())
    }
}
fn journal_error() -> codifix_agency_projects::Error {
    codifix_agency_projects::Error::Journal(
        "durability, authority or revision fence rejected the step".into(),
    )
}

pub async fn schedules(store: &Store, cfg: &Config) -> Result<()> {
    let rows=sqlx::query("SELECT id,next_at,interval_seconds,requester,template FROM agency_schedules WHERE enabled AND next_at<=clock_timestamp() ORDER BY next_at LIMIT 16").fetch_all(&store.pool).await?;
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let due: chrono::DateTime<Utc> = row.try_get("next_at")?;
        let interval: i32 = row.try_get("interval_seconds")?;
        let requester: String = row.try_get("requester")?;
        let Some(peer) = cfg.peers.iter().find(|p| p.id == requester) else {
            continue;
        };
        let mut req: Submit = serde_json::from_value(row.try_get("template")?)?;
        req.request_id = format!("schedule:{id}:{}", due.timestamp());
        req.deadline = due + chrono::Duration::seconds(i64::from(interval).max(300));
        if req.deadline <= Utc::now() {
            sqlx::query(
                "UPDATE agency_schedules SET next_at=clock_timestamp() WHERE id=$1 AND next_at=$2",
            )
            .bind(id)
            .bind(due)
            .execute(&store.pool)
            .await?;
            continue;
        }
        if store.submit(&req, peer).await.is_ok() {
            sqlx::query("UPDATE agency_schedules SET next_at=GREATEST(next_at+make_interval(secs=>$2),clock_timestamp()+make_interval(secs=>$2)) WHERE id=$1 AND next_at=$3")
                .bind(id).bind(f64::from(interval)).bind(due).execute(&store.pool).await?;
        }
    }
    Ok(())
}
pub async fn improve(store: &Store, cfg: &Config, input: Value) -> Result<Value> {
    use codifix_agency::improvement::*;
    let action = input["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("action required"))?;
    let mut tx = store.pool.begin().await?;
    let mut candidate: ImprovementCandidate = if action == "propose" {
        let c: ImprovementCandidate = serde_json::from_value(input["candidate"].clone())?;
        ensure!(
            c.stage == ImprovementStage::Candidate
                && c.evaluation.is_none()
                && c.local_acceptance.is_none()
                && c.rollout_ppm == 0
                && c.rollout_successes == 0
                && c.rollout_failures == 0
                && c.owner_instance_id == cfg.instance_id,
            "invalid candidate origin"
        );
        c
    } else {
        let id: Uuid = serde_json::from_value(input["id"].clone())?;
        let value: Value =
            sqlx::query_scalar("SELECT document FROM agency_improvements WHERE id=$1 FOR UPDATE")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        serde_json::from_value(value)?
    };
    let gate = ImprovementGate {
        minimum_held_out_cases: 20,
        minimum_quality_ppm: 900000,
        maximum_rollout_ppm: 100000,
        minimum_rollout_successes: 20,
    };
    let acceptance = || LocalAcceptance {
        owner_instance_id: cfg.instance_id,
        decision_ref: Uuid::new_v4().to_string(),
        accepted_at: Utc::now(),
    };
    match action {
        "propose" => {}
        "evaluate" => candidate.record_evaluation(
            serde_json::from_value(input["evaluation"].clone())?,
            &gate,
            Utc::now(),
        )?,
        "rollout" => candidate.start_rollout(100000, acceptance(), &gate, Utc::now())?,
        "observe" => candidate.record_rollout_result(
            input["passed"]
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("result required"))?,
        )?,
        "promote" => candidate.promote(acceptance(), &gate, Utc::now())?,
        "revert" => candidate.revert(
            input["reason"]
                .as_str()
                .unwrap_or("operator reverted")
                .into(),
        )?,
        _ => anyhow::bail!("unknown improvement action"),
    }
    let value = serde_json::to_value(&candidate)?;
    if action == "propose" {
        let specialist = input["specialist_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("specialist required"))?;
        ensure!(
            cfg.specialists
                .iter()
                .any(|s| s.specialist_id == specialist),
            "unknown specialist"
        );
        sqlx::query("INSERT INTO agency_improvements(id,specialist_id,document) VALUES($1,$2,$3)")
            .bind(candidate.id)
            .bind(specialist)
            .bind(&value)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE agency_improvements SET document=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1").bind(candidate.id).bind(&value).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(value)
}
