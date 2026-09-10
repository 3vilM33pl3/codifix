// SPDX-License-Identifier: AGPL-3.0-or-later

//! A run has committed phases, not a transaction spanning filesystem/provider work.
//! Each phase leaves a journal; terminal completion and required jobs commit together.
use super::*;
use mem_store::{
    events::{MutationContext, TimelineEvent},
    jobs::{self, NewJob},
};

/// Legacy rows have no invented ownership. A finished run may receive an
/// explicitly submitted follow-up proposal, but a disconnected executor may not.
pub(crate) async fn fence_run_execution_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run: Uuid,
    allow_finished: bool,
) -> Result<bool, ApiError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM foreground_executions WHERE id=$1")
            .bind(run)
            .fetch_optional(&mut **tx)
            .await
            .map_err(ApiError::sql)?;
    if status.is_none() || (allow_finished && status.as_deref() == Some("finished")) {
        return Ok(false);
    }
    mem_store::executions::fence(tx, run).await.map_err(|_| {
        ApiError::service_unavailable(
            "loop executor no longer owns this run; inspect its recovery status",
        )
    })?;
    let cancelled:bool = sqlx::query_scalar("SELECT cancel_requested_at IS NOT NULL OR status IN ('cancelled','failed') FROM loop_runs WHERE id=$1 FOR UPDATE")
        .bind(run).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    if cancelled {
        return Err(ApiError::status_message(
            axum::http::StatusCode::CONFLICT,
            "loop run was cancelled or failed",
        ));
    }
    Ok(true)
}

pub(crate) async fn reconcile_interrupted_runs(pool: &PgPool) -> Result<(), ApiError> {
    let ids = mem_store::executions::pending(pool, mem_store::executions::Kind::Loop, 100)
        .await
        .map_err(|e| ApiError::io(e.into()))?;
    for run in ids {
        let Some(mut repair) = mem_store::executions::recover(pool, run)
            .await
            .map_err(|e| ApiError::io(e.into()))?
        else {
            continue;
        };
        let project = repair.project_id;
        let tx = repair.transaction();
        let status:String = sqlx::query_scalar("UPDATE loop_runs SET status=CASE WHEN status IN ('queued','running') THEN 'failed' ELSE status END,finished_at=COALESCE(finished_at,now()),updated_at=now(),output_json=output_json || jsonb_build_object('interrupted',true,'external_effects_may_be_uncertain',true) WHERE id=$1 RETURNING status")
            .bind(run).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
        append_loop_trace_tx(tx,run,"interrupted","Executor disconnected; inspect committed phases before retry",json!({"status":status,"external_effects_may_be_uncertain":true,"actor_id":"system:loop-recovery"}),false).await?;
        append_run_event(
            tx,
            project,
            run,
            if status == "failed" {
                ActivityKind::LoopRunFailed
            } else {
                ActivityKind::LoopRunFinished
            },
            &MutationContext::internal("loop-recovery"),
        )
        .await?;
        repair.commit().await.map_err(|e| ApiError::io(e.into()))?;
    }
    Ok(())
}

/// Protect current loop policy (including newly inserted overrides) through the
/// worker's result commit. Use the run lock before any proposal/canonical locks.
pub(crate) async fn job_run_allowed_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Uuid,
    run: Uuid,
    manual: bool,
) -> Result<bool, ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended('loop-policy',0))")
        .execute(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    let row = sqlx::query("SELECT loop_id,repo_root,status,cancel_requested_at FROM loop_runs WHERE id=$1 AND project_id=$2 FOR UPDATE")
        .bind(run).bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
    let Some(row) = row else { return Ok(false) };
    let status: String = row.try_get("status").map_err(ApiError::sql)?;
    let cancelled: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("cancel_requested_at").map_err(ApiError::sql)?;
    if cancelled.is_some() || matches!(status.as_str(), "blocked" | "failed" | "cancelled") {
        return Ok(false);
    };
    let loop_id: String = row.try_get("loop_id").map_err(ApiError::sql)?;
    let repo: Option<String> = row.try_get("repo_root").map_err(ApiError::sql)?;
    let def = sqlx::query("SELECT * FROM loop_definitions WHERE loop_id=$1 AND is_current")
        .bind(&loop_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    let Some(def) = def else { return Ok(false) };
    let def = row_to_loop_definition(def)?;
    let kill: bool =
        sqlx::query_scalar("SELECT kill_switch_enabled FROM loop_global_state WHERE id=TRUE")
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::sql)?;
    let settings=sqlx::query("SELECT ls.*,p.slug AS project FROM loop_settings ls LEFT JOIN projects p ON p.id=ls.project_id
        WHERE ls.loop_id=$1 AND (ls.scope_type IN ('user','workspace') OR ls.project_id=$2 OR ($3::text IS NOT NULL AND ls.repo_root=$3))")
        .bind(&loop_id).bind(project).bind(repo).fetch_all(&mut **tx).await.map_err(ApiError::sql)?;
    let settings = settings
        .into_iter()
        .map(row_to_loop_setting)
        .collect::<Result<Vec<_>, _>>()?;
    let effective = resolve_effective_settings(&def, &settings, kill, manual, chrono::Utc::now());
    let action = evaluate_action(&effective.mode, LoopActionKind::WriteMemoryProposal);
    Ok(effective.blocked_reasons.is_empty()
        && budget_blocked(effective.budgets.as_ref()).is_none()
        && action.allowed
        && !action.requires_approval)
}

pub(super) async fn append_run_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Option<Uuid>,
    run: Uuid,
    kind: ActivityKind,
    context: &MutationContext,
) -> Result<(), ApiError> {
    if let Some(project) = project {
        let mut event = TimelineEvent::new(project, kind, format!("Loop run {run} state recorded"));
        event.operation_id = Some(run.to_string());
        context.stamp(&mut event);
        mem_store::events::append(tx, &event)
            .await
            .map_err(|e| ApiError::io(e.into()))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn store_run_phase(
    pool: &PgPool,
    run: Uuid,
    key: &str,
    summary: &str,
    output: &serde_json::Value,
    status: Option<&str>,
    blocked: Option<&str>,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    fence_run_execution_tx(&mut tx, run, false).await?;
    sqlx::query("UPDATE loop_runs SET output_summary=$2,
        output_json=output_json || jsonb_build_object($3::text,$4::jsonb),
        status=CASE WHEN cancel_requested_at IS NULL AND $5 IS DISTINCT FROM 'succeeded' THEN COALESCE($5,status) ELSE status END,
        blocked_reasons_json=CASE WHEN $6::text IS NULL OR blocked_reasons_json ? $6 THEN blocked_reasons_json ELSE blocked_reasons_json || jsonb_build_array($6::text) END,
        updated_at=now() WHERE id=$1")
        .bind(run).bind(summary).bind(key).bind(output).bind(status).bind(blocked)
        .execute(&mut *tx).await.map_err(ApiError::sql)?;
    append_loop_trace_tx(&mut tx,run,"phase_committed","Loop phase committed",
        json!({"phase":key,"output_fingerprint":hex_sha256(&serde_json::to_vec(output).map_err(|e|ApiError::io(e.into()))?)}),false).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish_run(
    pool: &PgPool,
    project: Option<Uuid>,
    run: Uuid,
    loop_id: &str,
    request: &LoopRunRequest,
    manual: bool,
    execution: &LoopExecution,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    finish_run_tx(&mut tx, project, run, loop_id, request, manual, execution).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish_run_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Option<Uuid>,
    run: Uuid,
    loop_id: &str,
    request: &LoopRunRequest,
    manual: bool,
    execution: &LoopExecution,
) -> Result<(), ApiError> {
    let owned = fence_run_execution_tx(tx, run, false).await?;
    let status: String = sqlx::query_scalar("UPDATE loop_runs SET
        status=CASE WHEN status IN ('queued','running') AND cancel_requested_at IS NULL THEN 'succeeded' ELSE status END,
        finished_at=COALESCE(finished_at,now()),updated_at=now() WHERE id=$1 RETURNING status")
        .bind(run).fetch_one(&mut **tx).await.map_err(ApiError::sql)?;
    let mut work_id = None;
    if status == "succeeded"
        && loop_id == mem_loops::LOOP_MEMORY_CONSOLIDATION
        && let Some(project_id) = project
    {
        let id = Uuid::new_v5(&run, b"consolidation");
        jobs::enqueue(
            tx,
            &NewJob {
                id,
                project_id,
                operation_id: run,
                kind: mem_record::JobKind::Consolidation,
                work_key: format!("loop:{run}:consolidation"),
                actor: execution.actor.clone(),
                input: json!({"version":1,"run_id":run,"manual":manual,"dry_run":request.dry_run}),
                dependencies: vec![],
            },
        )
        .await
        .map_err(|e| ApiError::io(e.into()))?;
        work_id = Some(id);
        sqlx::query("UPDATE loop_runs SET output_json=(output_json - 'background_state') || jsonb_build_object('background_job_id',$2::uuid::text) WHERE id=$1")
            .bind(run).bind(id).execute(&mut **tx).await.map_err(ApiError::sql)?;
    }
    append_loop_trace_tx(
        tx,
        run,
        "completed",
        "Loop control-plane work committed",
        json!({"status":status,"background_job_id":work_id,"actor_id":execution.context.actor_id}),
        false,
    )
    .await?;
    if owned {
        mem_store::executions::finish_tx(tx, run)
            .await
            .map_err(|e| ApiError::io(e.into()))?;
    }
    append_run_event(
        tx,
        project,
        run,
        ActivityKind::LoopRunFinished,
        &execution.context,
    )
    .await?;
    Ok(())
}
