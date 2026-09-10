// SPDX-License-Identifier: AGPL-3.0-or-later

//! Recoverable local enrichment. No external calls while result locks are held.
use crate::{
    AppState,
    repository::handlers::{
        consolidation::{AcceptedCluster, ConsolidationReport},
        loops::proposal_creation,
    },
};
use anyhow::{Context, Result};
use mem_record::{ActivityKind, JobFailureCode as Code, JobKind, JobReason, Permission};
use mem_store::{
    events::{MutationContext, TimelineEvent},
    jobs::{self, Completion, JobActor, JobLease, JobPolicy, JobResult, NewJob},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::Notify,
    task::{JoinHandle, JoinSet},
};
use uuid::Uuid;

#[derive(Clone, Default)]
pub(crate) struct Runtime {
    pub notify: Arc<Notify>,
    pub last_poll: Arc<AtomicI64>,
}
pub(crate) struct Workers(JoinHandle<()>);
impl Drop for Workers {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn start(state: AppState) -> Option<Workers> {
    if !state.is_primary()
        || !state.config.background_work.enabled
        || state.config.service.read_only
    {
        return None;
    }
    Some(Workers(tokio::spawn(async move {
        // Dropping this supervisor also aborts and drops all owned result futures.
        let mut active = JoinSet::new();
        let mut poll = tokio::time::interval(Duration::from_secs(
            state.config.background_work.poll_seconds,
        ));
        loop {
            tokio::select! {
                _ = poll.tick() => {},
                _ = state.background.notify.notified() => {},
                _ = active.join_next(), if !active.is_empty() => {},
            }
            let Ok(pool) = state.pool() else {
                continue;
            };
            // A live supervisor is not evidence that database recovery works.
            // When full, probe storage; otherwise only successful claims count.
            let mut healthy = if active.len() >= state.config.background_work.concurrent_projects {
                sqlx::query("SELECT 1").execute(&pool).await.is_ok()
            } else {
                false
            };
            while active.len() < state.config.background_work.concurrent_projects {
                match jobs::claim(&pool, policy(&state)).await {
                    Ok(Some(lease)) => {
                        healthy = true;
                        active.spawn(run_lease(state.clone(), pool.clone(), lease));
                    }
                    Ok(None) => {
                        healthy = true;
                        break;
                    }
                    Err(_) => {
                        healthy = false;
                        tracing::warn!("background claim unavailable; polling will recover");
                        break;
                    }
                }
            }
            if healthy {
                state
                    .background
                    .last_poll
                    .store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
            }
        }
    })))
}

fn policy(state: &AppState) -> JobPolicy {
    let c = &state.config.background_work;
    JobPolicy {
        lease_seconds: c.lease_seconds,
        max_attempts: c.max_attempts,
        initial_delay_seconds: c.initial_delay_seconds,
        max_delay_seconds: c.max_delay_seconds,
    }
}

#[derive(Debug)]
struct Failure(Code);
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for Failure {}
fn fail(code: Code) -> anyhow::Error {
    Failure(code).into()
}

fn classify(error: &anyhow::Error) -> Code {
    if let Some(f) = error.downcast_ref::<Failure>() {
        return f.0;
    }
    if let Some(sql) = error.downcast_ref::<sqlx::Error>() {
        return match sql {
            sqlx::Error::RowNotFound => Code::StaleInput,
            sqlx::Error::Database(e)
                if e.code().is_some_and(|c| {
                    !c.starts_with("08")
                        && !c.starts_with("40")
                        && !c.starts_with("53")
                        && !c.starts_with("57")
                }) =>
            {
                Code::Configuration
            }
            _ => Code::Transient,
        };
    }
    // Only the enumerated classification leaves this function, never provider text.
    let text = format!("{error:#}").to_lowercase();
    if text.contains("insufficient_quota") || text.contains("quota") {
        Code::Quota
    } else if text.contains("401") || text.contains("api key") || text.contains("credential") {
        Code::Credentials
    } else if text.contains("403") || text.contains("forbidden") {
        Code::Authorization
    } else if text.contains("429") || text.contains("rate limit") {
        Code::RateLimited
    } else if error.downcast_ref::<reqwest::Error>().is_some()
        || text.contains("timeout")
        || text.contains("502")
        || text.contains("500")
        || text.contains("503")
        || text.contains("504")
    {
        Code::Transient
    } else {
        Code::Configuration
    }
}

async fn run_lease(state: AppState, pool: PgPool, lease: JobLease) {
    let heartbeat = async {
        let mut renewal = tokio::time::interval(Duration::from_secs(
            state.config.background_work.renewal_seconds,
        ));
        renewal.tick().await;
        loop {
            renewal.tick().await;
            jobs::renew(&pool, &lease, policy(&state)).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), mem_store::Error>(())
    };
    // Keep polling writes while renewal waits on the locks they hold.
    let outcome = tokio::select! {
        result=execute(&state,&pool,&lease)=>result,
        _=heartbeat=>return,
    };
    if let Err(error) = outcome {
        if error
            .downcast_ref::<mem_store::Error>()
            .is_some_and(|e| matches!(e, mem_store::Error::StaleLease))
        {
            return;
        }
        let code = classify(&error);
        tracing::warn!(job_id=%lease.job.id,?code,"background stage failed");
        if let Ok(result) = jobs::begin_result(&pool, &lease).await {
            let _ = jobs::finish_result(result, Completion::Failure(code), policy(&state)).await;
        }
    }
}

async fn authorized<'a>(
    state: &AppState,
    pool: &'a PgPool,
    lease: &JobLease,
) -> Result<JobResult<'a>> {
    let mut result = jobs::begin_result(pool, lease).await?;
    let permissions = crate::auth::background_permissions(
        state,
        result.transaction(),
        &lease.actor,
        lease.job.project_id,
    )
    .await
    .map_err(|e| {
        fail(if e.status.is_server_error() {
            Code::Transient
        } else {
            Code::Authorization
        })
    })?;
    if !permissions.contains(Permission::MemoryCurate)
        || (matches!(
            lease.job.kind,
            JobKind::SemanticDedup | JobKind::Consolidation | JobKind::ConsolidationSynthesis
        ) && !permissions.contains(Permission::LoopsRun))
    {
        return Err(fail(Code::Authorization));
    }
    if let Some(run) = lease.input.get("run_id").filter(|v| !v.is_null()) {
        let run = run
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .ok_or_else(|| fail(Code::InvalidInput))?;
        let manual = lease.input.get("manual").and_then(Value::as_bool) == Some(true);
        if !crate::repository::handlers::loops::run_persistence::job_run_allowed_tx(
            result.transaction(),
            lease.job.project_id,
            run,
            manual,
        )
        .await
        .map_err(|_| fail(Code::Transient))?
        {
            return Err(fail(Code::Authorization));
        }
    }
    Ok(result)
}

fn context(lease: &JobLease) -> MutationContext {
    let actor = match &lease.actor {
        JobActor::Principal(id) | JobActor::Credential { principal: id, .. } => id.to_string(),
        JobActor::System(name) => format!("system:{name}"),
    };
    MutationContext {
        actor_id: actor.clone(),
        actor_name: actor,
        source: "background-work".into(),
        occurred_at: None,
    }
}

fn fingerprint<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

async fn cached<T: DeserializeOwned>(
    pool: &PgPool,
    lease: &JobLease,
    key: &str,
    config: &str,
) -> Result<Option<T>> {
    let value:Option<Value>=sqlx::query_scalar("SELECT output FROM background_job_outputs WHERE job_id=$1 AND output_key=$2 AND config_fingerprint=$3")
        .bind(lease.job.id).bind(key).bind(config).fetch_optional(pool).await?;
    value
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

async fn cache<T: Serialize>(
    result: &mut JobResult<'_>,
    lease: &JobLease,
    key: &str,
    config: &str,
    value: &T,
) -> Result<()> {
    sqlx::query("INSERT INTO background_job_outputs(job_id,output_key,config_fingerprint,output) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(lease.job.id).bind(key).bind(config).bind(serde_json::to_value(value)?).execute(&mut **result.transaction()).await?;
    Ok(())
}

async fn event(
    result: &mut JobResult<'_>,
    lease: &JobLease,
    kind: ActivityKind,
    key: &str,
    summary: String,
) -> Result<()> {
    let mut event = TimelineEvent::new(lease.job.project_id, kind, summary);
    event.id = Uuid::new_v5(&lease.job.id, key.as_bytes());
    event.operation_id = Some(lease.job.operation_id.to_string());
    context(lease).stamp(&mut event);
    mem_store::events::append(result.transaction(), &event).await?;
    Ok(())
}

async fn skip(state: &AppState, pool: &PgPool, lease: &JobLease, reason: JobReason) -> Result<()> {
    let mut result = authorized(state, pool, lease).await?;
    sqlx::query("UPDATE background_jobs SET reason=$2 WHERE id=$1")
        .bind(lease.job.id)
        .bind(serde_json::to_value(reason)?.as_str())
        .execute(&mut **result.transaction())
        .await?;
    jobs::finish_result(result, Completion::Skipped, policy(state)).await?;
    Ok(())
}

async fn execute(state: &AppState, pool: &PgPool, lease: &JobLease) -> Result<()> {
    if lease.input.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(fail(Code::InvalidInput));
    }
    jobs::checkpoint_result(authorized(state, pool, lease).await?).await?;
    let project: String = sqlx::query_scalar("SELECT slug FROM projects WHERE id=$1")
        .bind(lease.job.project_id)
        .fetch_one(pool)
        .await?;
    let versions:Vec<Uuid>=sqlx::query_scalar("SELECT memory_id FROM background_work_versions WHERE project_id=$1 AND operation_id=$2 ORDER BY memory_id")
        .bind(lease.job.project_id).bind(lease.job.operation_id).fetch_all(pool).await?;
    match lease.job.kind {
        JobKind::Embedding => embedding(state, pool, lease, &project, &versions).await,
        JobKind::SemanticDedup => semantic(state, pool, lease, &project, &versions).await,
        JobKind::ValidationDue => {
            if !state.config.reinforcement.enabled || !state.config.reinforcement.validation_enabled
            {
                return skip(state, pool, lease, JobReason::Disabled).await;
            }
            let due = crate::repository::handlers::reinforcement::due_validation_infos(
                state,
                lease.job.project_id,
            )
            .await?;
            let mut result = authorized(state, pool, lease).await?;
            let config = fingerprint(&state.config.reinforcement)?;
            cache(
                &mut result,
                lease,
                "due",
                &config,
                &due.iter().map(|d| d.memory_id).collect::<Vec<_>>(),
            )
            .await?;
            event(
                &mut result,
                lease,
                ActivityKind::MemoryValidation,
                "validation-due",
                format!("Background validation check: {} due version(s)", due.len()),
            )
            .await?;
            jobs::finish_result(result, Completion::Succeeded, policy(state)).await?;
            if !due.is_empty()
                && let Some(runtime) = &state.reinforcement
            {
                runtime.notify.notify_one();
            }
            Ok(())
        }
        JobKind::Consolidation => consolidation(state, pool, lease, &project).await,
        JobKind::ConsolidationSynthesis => synthesis(state, pool, lease, &project).await,
    }
}

async fn embedding(
    state: &AppState,
    pool: &PgPool,
    lease: &JobLease,
    project: &str,
    versions: &[Uuid],
) -> Result<()> {
    if versions.is_empty() {
        return skip(state, pool, lease, JobReason::NoInputs).await;
    }
    if !state
        .automated_embedding_creation_enabled
        .load(Ordering::Relaxed)
    {
        return skip(state, pool, lease, JobReason::Disabled).await;
    }
    let registry = state.embedders.read().await;
    if registry.is_empty() {
        return if state.config.embeddings.backends.is_empty() {
            skip(state, pool, lease, JobReason::Disabled).await
        } else {
            Err(fail(Code::Configuration))
        };
    }
    let spaces: Vec<_> = registry
        .iter_create_enabled()
        .map(|(_, s)| s.embedding_space_key())
        .collect();
    if spaces.is_empty() {
        return skip(state, pool, lease, JobReason::Disabled).await;
    }
    let config = fingerprint(&spaces)?;
    for version in versions {
        if !state
            .automated_embedding_creation_enabled
            .load(Ordering::Relaxed)
        {
            return Err(fail(Code::Configuration));
        }
        jobs::checkpoint_result(authorized(state, pool, lease).await?).await?;
        let key = version.to_string();
        let applied_key = format!("applied:{key}");
        if cached::<bool>(pool, lease, &applied_key, &config).await? == Some(true) {
            continue;
        }
        let output = if let Some(output) =
            cached::<mem_search::rebuild::PreparedMemoryChunks>(pool, lease, &key, &config).await?
        {
            output
        } else {
            let output =
                mem_search::rebuild::prepare_automatic(pool, project, *version, &registry, true)
                    .await?;
            let mut result = authorized(state, pool, lease).await?;
            cache(&mut result, lease, &key, &config, &output).await?;
            jobs::checkpoint_result(result).await?;
            output
        };
        let mut result = authorized(state, pool, lease).await?;
        if !state
            .automated_embedding_creation_enabled
            .load(Ordering::Relaxed)
        {
            return Err(fail(Code::Configuration));
        }
        mem_search::rebuild::apply(result.transaction(), &output).await?;
        cache(&mut result, lease, &applied_key, &config, &true).await?;
        event(
            &mut result,
            lease,
            ActivityKind::Reembed,
            &format!(
                "{key}:{config}:{}:{}",
                lease.job.generation, lease.job.attempts
            ),
            format!("Background embedding completed for version {version}"),
        )
        .await?;
        jobs::checkpoint_result(result).await?;
    }
    jobs::finish_result(
        authorized(state, pool, lease).await?,
        Completion::Succeeded,
        policy(state),
    )
    .await?;
    Ok(())
}

async fn semantic(
    state: &AppState,
    pool: &PgPool,
    lease: &JobLease,
    project: &str,
    versions: &[Uuid],
) -> Result<()> {
    if !state.config.curation.semantic_dedup_enabled {
        return skip(state, pool, lease, JobReason::Disabled).await;
    }
    let dep_skipped:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM background_job_dependencies d JOIN background_jobs j ON j.id=d.depends_on WHERE d.job_id=$1 AND j.status='skipped')")
        .bind(lease.job.id).fetch_one(pool).await?;
    if dep_skipped {
        return skip(state, pool, lease, JobReason::DependencySkipped).await;
    }
    let mut result = authorized(state, pool, lease).await?;
    let pairs = mem_curate::refresh_semantic_relations_tx(
        result.transaction(),
        project,
        versions,
        state.config.curation.semantic_duplicate_threshold,
    )
    .await?;
    let mut events = Vec::new();
    for pair in pairs {
        let mut ids = [pair.memory_id, pair.other_memory_id];
        ids.sort();
        let operation = Uuid::new_v5(
            &lease.job.project_id,
            format!("semantic:{}:{}:{}", ids[0], ids[1], pair.conflict).as_bytes(),
        );
        // A stable version-pair identity deduplicates across curation runs too.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM proposal_creation_receipts WHERE operation_id=$1)",
        )
        .bind(operation)
        .fetch_one(&mut **result.transaction())
        .await?;
        if exists {
            continue;
        }
        let request = mem_record::LoopMemoryProposalCreateRequest {
            project: project.into(),
            loop_id: "semantic_dedup".into(),
            proposal_type: if pair.conflict { "link" } else { "merge" }.into(),
            run_id: None,
            target_memory_id: Some(ids[0]),
            candidate: serde_json::json!({"related_memory_id":ids[1],"relation_type":if pair.conflict {"related_to"}else{"duplicates"}}),
            evidence: serde_json::json!([{"source_kind":"note","excerpt":format!("Cosine {:.3}; lexical overlap {:.2}; versions {} and {}",pair.similarity,pair.lexical_overlap,ids[0],ids[1])}]),
            confidence: if pair.conflict { 0.60 } else { 0.74 },
            risk_notes: Some(
                if pair.conflict {
                    "Possible contradiction; review currency and evidence before acting."
                } else {
                    "Embedding similarity is not factual correctness; human review required."
                }
                .into(),
            ),
        };
        let pending = proposal_creation::prepare_tx(
            result.transaction(),
            &request,
            operation,
            &context(lease),
        )
        .await?;
        if let Some(event) = pending.event {
            events.push(event);
        }
    }
    for event in events {
        mem_store::events::append(result.transaction(), &event).await?;
    }
    event(
        &mut result,
        lease,
        ActivityKind::Reindex,
        "semantic-dedup",
        "Background semantic relation pass completed".into(),
    )
    .await?;
    jobs::finish_result(result, Completion::Succeeded, policy(state)).await?;
    Ok(())
}

async fn consolidation(
    state: &AppState,
    pool: &PgPool,
    lease: &JobLease,
    project: &str,
) -> Result<()> {
    let cfg = &state.config.consolidation;
    if lease.input.get("dry_run").and_then(Value::as_bool) == Some(true) {
        return skip(state, pool, lease, JobReason::DryRun).await;
    }
    let manual = lease.input.get("manual").and_then(Value::as_bool) == Some(true);
    if !cfg.enabled {
        return skip(state, pool, lease, JobReason::Disabled).await;
    }
    let newer_ready:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM background_jobs j WHERE j.project_id=$1 AND j.kind='consolidation' AND j.status='queued' AND (j.created_at,j.id)>($2,$3) AND NOT EXISTS(SELECT 1 FROM background_job_dependencies d JOIN background_jobs p ON p.id=d.depends_on WHERE d.job_id=j.id AND p.status NOT IN ('succeeded','skipped')))")
        .bind(lease.job.project_id).bind(lease.job.created_at).bind(lease.job.id).fetch_one(pool).await?;
    if newer_ready && !manual {
        return skip(state, pool, lease, JobReason::Coalesced).await;
    }
    let config = fingerprint(cfg)?;
    let report: ConsolidationReport =
        if let Some(output) = cached(pool, lease, "scan", &config).await? {
            output
        } else {
            crate::repository::handlers::consolidation::run_memory_consolidation(
                pool,
                project,
                cfg,
                state.config.reinforcement.half_life.as_secs_f64().max(1.0),
            )
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?
        };
    let mut result = authorized(state, pool, lease).await?;
    cache(&mut result, lease, "scan", &config, &report).await?;
    if (cfg.auto_trigger || manual) && !cfg.dry_run {
        for cluster in report.accepted.iter().take(cfg.daily_cap as usize) {
            let mut ids: Vec<_> = cluster.members.iter().map(|m| m.version_id).collect();
            ids.sort();
            let seed = fingerprint(&ids)?;
            let id = Uuid::new_v5(
                &lease.job.project_id,
                format!("synthesis:{seed}").as_bytes(),
            );
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM background_jobs WHERE id=$1)")
                    .bind(id)
                    .fetch_one(&mut **result.transaction())
                    .await?;
            if exists {
                continue;
            }
            jobs::enqueue(
                result.transaction(),
                &NewJob {
                    id,
                    project_id: lease.job.project_id,
                    operation_id: lease.job.operation_id,
                    kind: JobKind::ConsolidationSynthesis,
                    work_key: format!("synthesis:{seed}"),
                    actor: lease.actor.clone(),
                    input: serde_json::json!({"version":1,"source_fingerprint":seed,"manual":manual,"run_id":lease.input.get("run_id")}),
                    dependencies: vec![lease.job.id],
                },
            )
            .await?;
            sqlx::query(
                "INSERT INTO background_synthesis_inputs(job_id,cluster_snapshot) VALUES($1,$2)",
            )
            .bind(id)
            .bind(serde_json::to_value(cluster)?)
            .execute(&mut **result.transaction())
            .await?;
        }
    }
    event(
        &mut result,
        lease,
        ActivityKind::Consolidation,
        "scan",
        format!(
            "Background consolidation scan: {} accepted cluster(s); dry_run={}",
            report.due_infos().len(),
            cfg.dry_run
        ),
    )
    .await?;
    jobs::finish_result(result, Completion::Succeeded, policy(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests;

async fn revalidate(
    result: &mut JobResult<'_>,
    lease: &JobLease,
    cluster: &AcceptedCluster,
) -> Result<()> {
    let mut canonical_ids: Vec<_> = cluster.members.iter().map(|m| m.canonical_id).collect();
    canonical_ids.sort();
    canonical_ids.dedup();
    sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE canonical_id=ANY($1) AND project_id=$2 ORDER BY canonical_id FOR SHARE")
        .bind(&canonical_ids).bind(lease.job.project_id).fetch_all(&mut **result.transaction()).await?;
    let mut version_ids: Vec<_> = cluster.members.iter().map(|m| m.version_id).collect();
    version_ids.sort();
    sqlx::query(
        "SELECT id FROM memory_entries WHERE id=ANY($1) AND project_id=$2 ORDER BY id FOR SHARE",
    )
    .bind(&version_ids)
    .bind(lease.job.project_id)
    .fetch_all(&mut **result.transaction())
    .await?;
    for member in &cluster.members {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_entries WHERE id=$1 AND project_id=$2 AND canonical_id=$3 AND NOT is_tombstone AND memory_state_status(canonical_id)='active' AND id=(SELECT newest.id FROM memory_entries newest WHERE newest.canonical_id=$3 ORDER BY version_no DESC LIMIT 1) AND summary=$4 AND canonical_text=$5)")
            .bind(member.version_id).bind(lease.job.project_id).bind(member.canonical_id).bind(&member.summary).bind(&member.canonical_text).fetch_one(&mut **result.transaction()).await?;
        if !valid {
            return Err(fail(Code::StaleInput));
        }
    }
    Ok(())
}

async fn utility_suppressed_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Uuid,
    config: &mem_config::ProceduralConfig,
) -> Result<bool> {
    if !config.enabled || !config.utility_floor_enabled {
        return Ok(false);
    }
    // Same key as the reward writer, including an absent row. Keep this shared
    // policy lock through result commit; a new reward cannot invalidate a check.
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1,0))")
        .bind(format!(
            "utility:{project}:loop:{}",
            mem_loops::LOOP_MEMORY_CONSOLIDATION
        ))
        .execute(&mut **tx)
        .await?;
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM procedural_utility WHERE project_id=$1 AND producer_kind='loop' AND producer_id=$2 AND update_count >= $3 AND utility < $4)")
        .bind(project).bind(mem_loops::LOOP_MEMORY_CONSOLIDATION).bind(config.min_samples).bind(config.utility_floor).fetch_one(&mut **tx).await?)
}

async fn synthesis(state: &AppState, pool: &PgPool, lease: &JobLease, project: &str) -> Result<()> {
    let cfg = &state.config.consolidation;
    let manual = lease.input.get("manual").and_then(Value::as_bool) == Some(true);
    if !cfg.enabled || (!cfg.auto_trigger && !manual) {
        return skip(state, pool, lease, JobReason::Disabled).await;
    }
    if cfg.dry_run {
        return skip(state, pool, lease, JobReason::DryRun).await;
    }
    if state.config.llm.model.trim().is_empty() {
        return Err(fail(Code::Configuration));
    }
    let value: Value = sqlx::query_scalar(
        "SELECT cluster_snapshot FROM background_synthesis_inputs WHERE job_id=$1",
    )
    .bind(lease.job.id)
    .fetch_one(pool)
    .await?;
    let cluster: AcceptedCluster = serde_json::from_value(value)?;
    let config = fingerprint(&(cfg, &state.config.llm))?;
    let request: mem_record::LoopMemoryProposalCreateRequest = if let Some(output) =
        cached(pool, lease, "synthesis", &config).await?
    {
        output
    } else {
        let mut result = authorized(state, pool, lease).await?;
        revalidate(&mut result, lease, &cluster).await?;
        if utility_suppressed_tx(
            result.transaction(),
            lease.job.project_id,
            &state.config.procedural,
        )
        .await?
        {
            drop(result);
            return skip(state, pool, lease, JobReason::UtilityFloor).await;
        }
        let reserved:Option<i32>=sqlx::query_scalar("INSERT INTO background_daily_budget(project_id,day,attempts) SELECT $1,(clock_timestamp() AT TIME ZONE 'UTC')::date,1 WHERE $2>0 ON CONFLICT(project_id,day) DO UPDATE SET attempts=background_daily_budget.attempts+1 WHERE background_daily_budget.attempts<$2 RETURNING attempts")
            .bind(lease.job.project_id).bind(i32::try_from(cfg.daily_cap).map_err(|_|fail(Code::Configuration))?).fetch_optional(&mut **result.transaction()).await?;
        if reserved.is_none() {
            return Err(fail(Code::Quota));
        }
        sqlx::query("UPDATE background_job_attempts SET uncertain=true WHERE token=$1")
            .bind(lease.token)
            .execute(&mut **result.transaction())
            .await?;
        jobs::checkpoint_result(result).await?;
        let request = crate::consolidate::prepare_consolidation_proposal(
            state,
            project,
            lease
                .input
                .get("run_id")
                .and_then(Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok()),
            &cluster,
        )
        .await
        .context("prepare durable synthesis")?;
        let mut result = authorized(state, pool, lease).await?;
        revalidate(&mut result, lease, &cluster).await?;
        cache(&mut result, lease, "synthesis", &config, &request).await?;
        jobs::checkpoint_result(result).await?;
        request
    };
    let mut result = authorized(state, pool, lease).await?;
    revalidate(&mut result, lease, &cluster).await?;
    if utility_suppressed_tx(
        result.transaction(),
        lease.job.project_id,
        &state.config.procedural,
    )
    .await?
    {
        drop(result);
        return skip(state, pool, lease, JobReason::UtilityFloor).await;
    }
    sqlx::query("UPDATE background_job_attempts SET uncertain=false WHERE token=$1")
        .bind(lease.token)
        .execute(&mut **result.transaction())
        .await?;
    proposal_creation::create_tx(
        result.transaction(),
        &request,
        lease.job.id,
        &context(lease),
    )
    .await?;
    jobs::finish_result(result, Completion::Succeeded, policy(state)).await?;
    Ok(())
}
