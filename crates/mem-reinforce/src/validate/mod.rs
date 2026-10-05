// SPDX-License-Identifier: AGPL-3.0-or-later

//! Threshold-triggered validation pipeline: deterministic evidence
//! gathering, a pluggable verdict provider (LLM in the service; an
//! agent-CLI/worktree runner can implement the same trait later), and a
//! strict apply policy. Validation reads never count as memory accesses.

pub mod apply;
pub mod evidence;
pub mod verdict;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::repository::{
    EvidenceRow, ValidationRunCompletion, complete_validation_run, fail_validation_run,
    insert_score_audit, insert_validation_evidence_tx, insert_validation_run,
    mark_correction_pending, mark_needs_review, mark_validated, set_validation_cooldown,
};
use crate::selection::ValidationCandidate;
pub use apply::{Decision, ValidationAction, ValidationPolicy, decide};
pub use evidence::{ValidationContext, gather_context};
pub use verdict::{RawVerdict, ValidatedVerdict, Verdict, parse_verdict_content, validate_verdict};

/// What initiated a validation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationTrigger {
    Threshold,
    Curator,
    Manual,
    Scheduled,
}

impl ValidationTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationTrigger::Threshold => "threshold",
            ValidationTrigger::Curator => "curator",
            ValidationTrigger::Manual => "manual",
            ValidationTrigger::Scheduled => "scheduled",
        }
    }
}

/// Produces a verdict for a gathered context. Implementations must not
/// mutate any state; the pipeline owns all persistence.
#[async_trait]
#[allow(
    clippy::double_must_use,
    reason = "async_trait generates must_use on its boxed futures"
)]
pub trait VerdictProvider: Send + Sync {
    async fn assess(&self, context: &ValidationContext) -> Result<RawVerdict>;

    /// Model identifier recorded on the validation run, if any.
    fn model_name(&self) -> Option<String> {
        None
    }
}

/// Result of one validation run.
#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub run_id: Uuid,
    pub verdict: Verdict,
    pub confidence: f32,
    pub action: ValidationAction,
    pub dry_run: bool,
    /// New memory version id when rewording was auto-applied.
    pub new_memory_id: Option<Uuid>,
    /// Committed activity receipts; callers may broadcast but must not append again.
    pub events: Vec<(mem_store::events::TimelineEvent, i64)>,
}

/// Authenticated execution identity is independent of the memory's original author.
#[derive(Debug, Clone)]
pub struct ValidationExecution {
    pub context: mem_store::events::MutationContext,
    pub actor: mem_store::jobs::JobActor,
}
impl Default for ValidationExecution {
    fn default() -> Self {
        Self {
            context: mem_store::events::MutationContext::internal("memory-validation"),
            actor: mem_store::jobs::JobActor::System("local-curation".into()),
        }
    }
}

/// Cooldown after a failed run: short, so transient provider errors are
/// retried well before the regular cooldown would allow.
const FAILURE_COOLDOWN_HOURS: i64 = 24;

/// Runs the full pipeline for one candidate. Every path leaves an auditable
/// trace: a run row (completed or failed), evidence rows for completed
/// runs, a score-audit entry, and a cooldown so no memory can loop.
pub async fn run_validation(
    pool: &PgPool,
    candidate: &ValidationCandidate,
    provider: &dyn VerdictProvider,
    policy: &ValidationPolicy,
    trigger: ValidationTrigger,
) -> Result<ValidationOutcome> {
    run_validation_with_scope(
        pool,
        candidate,
        provider,
        policy,
        trigger,
        mem_record::ValidationProofScope::SourceFilesFirst,
    )
    .await
}

pub async fn run_validation_with_scope(
    pool: &PgPool,
    candidate: &ValidationCandidate,
    provider: &dyn VerdictProvider,
    policy: &ValidationPolicy,
    trigger: ValidationTrigger,
    proof_scope: mem_record::ValidationProofScope,
) -> Result<ValidationOutcome> {
    run_validation_as(
        pool,
        candidate,
        provider,
        policy,
        trigger,
        proof_scope,
        &ValidationExecution::default(),
    )
    .await
}

pub async fn run_validation_as(
    pool: &PgPool,
    candidate: &ValidationCandidate,
    provider: &dyn VerdictProvider,
    policy: &ValidationPolicy,
    trigger: ValidationTrigger,
    proof_scope: mem_record::ValidationProofScope,
    execution: &ValidationExecution,
) -> Result<ValidationOutcome> {
    let mut tx = pool.begin().await?;
    let run_id = insert_validation_run(
        &mut *tx,
        candidate.canonical_id,
        candidate.memory_id,
        candidate.project_id,
        trigger.as_str(),
        policy.dry_run,
    )
    .await?;
    let owner = mem_store::executions::Execution::acquire(pool, run_id).await?;
    owner
        .register(
            &mut tx,
            mem_store::executions::Kind::Validation,
            Some(candidate.project_id),
        )
        .await?;
    crate::repository::append_validation_activity_tx(
        &mut tx,
        candidate.project_id,
        candidate.canonical_id,
        candidate.memory_id,
        run_id,
        "Validation requested",
        execution,
    )
    .await?;
    tx.commit().await?;
    let outcome = execute_validation(
        pool,
        candidate,
        provider,
        policy,
        run_id,
        proof_scope,
        execution,
    )
    .await;
    if outcome.is_err() {
        let mut tx = pool.begin().await?;
        mem_store::executions::fence(&mut tx, run_id).await?;
        // Provider failures may contain prompts, secrets or response bodies.
        fail_validation_run(
            &mut *tx,
            run_id,
            "Validation did not complete; provider outcome may be uncertain",
        )
        .await?;
        set_validation_cooldown(
            &mut *tx,
            candidate.canonical_id,
            Utc::now() + chrono::Duration::hours(FAILURE_COOLDOWN_HOURS),
        )
        .await?;
        mem_store::executions::finish_tx(&mut tx, run_id).await?;
        crate::repository::append_validation_activity_tx(
            &mut tx,
            candidate.project_id,
            candidate.canonical_id,
            candidate.memory_id,
            run_id,
            "Validation failed; review required",
            execution,
        )
        .await?;
        tx.commit().await?;
    }
    outcome
}

/// Mark disconnected foreground runs failed without repeating a provider call.
/// The owning session (not elapsed time) decides whether recovery is allowed.
pub async fn reconcile_interrupted(pool: &PgPool) -> Result<usize> {
    use sqlx::Row;
    let mut recovered = 0;
    for run_id in
        mem_store::executions::pending(pool, mem_store::executions::Kind::Validation, 100).await?
    {
        let Some(mut repair) = mem_store::executions::recover(pool, run_id).await? else {
            continue;
        };
        let tx = repair.transaction();
        let row = sqlx::query(
            "SELECT canonical_id,memory_id,project_id FROM memory_validation_runs WHERE id=$1",
        )
        .bind(run_id)
        .fetch_one(&mut **tx)
        .await?;
        let canonical: Uuid = row.try_get("canonical_id")?;
        let memory: Uuid = row.try_get("memory_id")?;
        let project: Uuid = row.try_get("project_id")?;
        // Match result/review lock order: execution, canonical, run, timeline.
        sqlx::query(
            "SELECT canonical_id FROM memory_canonical_state WHERE canonical_id=$1 FOR UPDATE",
        )
        .bind(canonical)
        .fetch_optional(&mut **tx)
        .await?;
        let status: String =
            sqlx::query_scalar("SELECT status FROM memory_validation_runs WHERE id=$1 FOR UPDATE")
                .bind(run_id)
                .fetch_one(&mut **tx)
                .await?;
        if status == "running" {
            fail_validation_run(
                &mut **tx,
                run_id,
                "Executor disconnected; provider outcome may be uncertain; review before retry",
            )
            .await?;
            set_validation_cooldown(
                &mut **tx,
                canonical,
                Utc::now() + chrono::Duration::hours(FAILURE_COOLDOWN_HOURS),
            )
            .await?;
            let execution = ValidationExecution {
                context: mem_store::events::MutationContext::internal("validation-recovery"),
                ..Default::default()
            };
            crate::repository::append_validation_activity_tx(
                tx,
                project,
                canonical,
                memory,
                run_id,
                "Validation interrupted; review required",
                &execution,
            )
            .await?;
        }
        repair.commit().await?;
        recovered += 1;
    }
    Ok(recovered)
}

async fn execute_validation(
    pool: &PgPool,
    candidate: &ValidationCandidate,
    provider: &dyn VerdictProvider,
    policy: &ValidationPolicy,
    run_id: Uuid,
    proof_scope: mem_record::ValidationProofScope,
    execution: &ValidationExecution,
) -> Result<ValidationOutcome> {
    let started = std::time::Instant::now();
    let mut context = gather_context(pool, candidate.memory_id, proof_scope, false).await?;
    let raw = provider
        .assess(&context)
        .await
        .context("verdict provider failed")?;
    let mut verdict = validate_verdict(raw, &context).context("verdict failed validation")?;
    if proof_scope == mem_record::ValidationProofScope::HybridFallback
        && matches!(
            verdict.verdict,
            verdict::Verdict::Ambiguous | verdict::Verdict::Unsupported
        )
    {
        context = gather_context(pool, candidate.memory_id, proof_scope, true).await?;
        let raw = provider
            .assess(&context)
            .await
            .context("verdict provider failed during fallback proof search")?;
        verdict = validate_verdict(raw, &context).context("fallback verdict failed validation")?;
    }
    let decision = decide(&verdict, policy);

    let evidence_rows: Vec<EvidenceRow> = verdict
        .evidence
        .iter()
        .map(|item| EvidenceRow {
            kind: item.kind.as_str().to_string(),
            evidence_ref: item.evidence_ref.clone(),
            stance: item.stance.as_str().to_string(),
            excerpt: item.excerpt.clone(),
        })
        .collect();
    let mut tx = pool.begin().await?;
    mem_store::executions::fence(&mut tx, run_id).await?;
    crate::repository::lock_validation_source_tx(
        &mut tx,
        candidate.canonical_id,
        candidate.memory_id,
        candidate.project_id,
    )
    .await?;
    crate::repository::lock_validation_run_tx(&mut tx, run_id).await?;
    insert_validation_evidence_tx(&mut tx, run_id, &evidence_rows).await?;
    let mut pending_events = Vec::new();

    let proposed_candidate_json = if decision.store_proposal {
        Some(serde_json::json!({
            "proposed_summary": verdict.proposed_summary,
            "proposed_text": verdict.proposed_text,
            "previous_memory_id": candidate.memory_id,
        }))
    } else {
        None
    };

    // Apply side effects, unless this is a dry run: dry runs record the
    // run + evidence and change nothing else — except the cooldown, which
    // is pure scheduling state; without it a dry-run deployment would
    // re-validate the same hottest memory every scheduler cycle.
    let cooldown_until = Utc::now() + policy.cooldown;
    let mut new_memory_id = None;
    let mut review_status: Option<&'static str> = None;
    if policy.dry_run {
        set_validation_cooldown(&mut *tx, candidate.canonical_id, cooldown_until).await?;
    } else {
        match decision.action {
            ValidationAction::Revalidated => {
                mark_validated(
                    &mut *tx,
                    candidate.canonical_id,
                    verdict.confidence,
                    run_id,
                    cooldown_until,
                )
                .await?;
            }
            ValidationAction::Reworded => {
                let summary = verdict
                    .proposed_summary
                    .clone()
                    .unwrap_or_else(|| context.memory.summary.clone());
                let text = verdict
                    .proposed_text
                    .clone()
                    .unwrap_or_else(|| context.memory.canonical_text.clone());
                let applied = mem_curate::prepare_validation_revision_tx(
                    &mut tx,
                    candidate.memory_id,
                    &summary,
                    &text,
                    &execution.context,
                    &execution.actor,
                )
                .await
                .context("apply validated rewording")?;
                new_memory_id = Some(applied.0);
                pending_events.push(applied.1);
                mark_validated(
                    &mut *tx,
                    candidate.canonical_id,
                    verdict.confidence,
                    run_id,
                    cooldown_until,
                )
                .await?;
            }
            ValidationAction::CorrectionPending => {
                review_status = Some("pending");
                mark_correction_pending(
                    &mut *tx,
                    candidate.canonical_id,
                    run_id,
                    cooldown_until,
                    decision.invalidated,
                )
                .await?;
            }
            ValidationAction::FlaggedNeedsReview => {
                let reason = decision
                    .needs_review_reason
                    .clone()
                    .unwrap_or_else(|| "weak or contradictory evidence".to_string());
                mark_needs_review(
                    &mut *tx,
                    candidate.canonical_id,
                    &reason,
                    run_id,
                    cooldown_until,
                )
                .await?;
                insert_score_audit(
                    &mut *tx,
                    candidate.canonical_id,
                    candidate.project_id,
                    "needs_review_set",
                    None,
                    None,
                    serde_json::json!({ "reason": reason, "run_id": run_id }),
                )
                .await?;
            }
        }
    }

    complete_validation_run(
        &mut *tx,
        run_id,
        &ValidationRunCompletion {
            verdict: verdict.verdict.as_str(),
            confidence: verdict.confidence,
            action: decision.action.as_str(policy.dry_run).to_string(),
            reasons: serde_json::json!(verdict.reasons),
            proposed_candidate_json,
            review_status,
            model: provider.model_name(),
            details: serde_json::json!({
                "trigger_activation": candidate.activation,
                "volatility": candidate.volatility,
                "evidence_count": evidence_rows.len(),
                "git_log_lines": context.git_log.len(),
                "proof_scope": context.proof_scope,
                "proof_fallback_used": context.proof_fallback_used,
                "proof_snippet_count": context.proof_snippets.len(),
                "duration_ms": started.elapsed().as_millis() as u64,
                "new_memory_id": new_memory_id,
            }),
        },
    )
    .await?;

    insert_score_audit(
        &mut *tx,
        candidate.canonical_id,
        candidate.project_id,
        "validation_completed",
        Some(candidate.activation),
        Some(candidate.activation),
        serde_json::json!({
            "run_id": run_id,
            "verdict": verdict.verdict.as_str(),
            "confidence": verdict.confidence,
            "action": decision.action.as_str(policy.dry_run),
            "dry_run": policy.dry_run,
        }),
    )
    .await?;

    let activity = crate::repository::prepare_validation_activity(
        candidate.project_id,
        candidate.canonical_id,
        new_memory_id.unwrap_or(candidate.memory_id),
        run_id,
        "Validation completed",
        execution,
    );
    pending_events.push(activity);
    mem_store::executions::finish_tx(&mut tx, run_id).await?;
    let mut events = Vec::new();
    for event in pending_events {
        let seq = mem_store::events::append(&mut tx, &event).await?;
        events.push((event, seq));
    }
    tx.commit().await?;
    Ok(ValidationOutcome {
        run_id,
        verdict: verdict.verdict,
        confidence: verdict.confidence,
        action: decision.action,
        dry_run: policy.dry_run,
        new_memory_id,
        events,
    })
}

/// Outcome of resolving a pending correction review.
#[derive(Debug, Clone)]
pub struct ReviewResolution {
    pub run_id: Uuid,
    pub applied: bool,
    pub canonical_id: Uuid,
    pub project_id: Uuid,
    /// New memory version created when the correction was applied.
    pub new_memory_id: Option<Uuid>,
    pub events: Vec<(mem_store::events::TimelineEvent, i64)>,
}

/// Applies or rejects a pending validation correction. Applying writes the
/// proposed content as a new immutable version and marks the memory
/// validated; either resolution clears a standing needs_review flag (a
/// human has now looked at the memory).
pub async fn resolve_review(
    pool: &PgPool,
    run_id: Uuid,
    approve: bool,
) -> Result<ReviewResolution> {
    resolve_review_as(pool, run_id, approve, &ValidationExecution::default()).await
}

pub async fn resolve_review_as(
    pool: &PgPool,
    run_id: Uuid,
    approve: bool,
    execution: &ValidationExecution,
) -> Result<ReviewResolution> {
    let mut tx = pool.begin().await?;
    let result = resolve_review_tx(&mut tx, run_id, approve, execution).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn resolve_review_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    approve: bool,
    execution: &ValidationExecution,
) -> Result<ReviewResolution> {
    use crate::repository::{
        clear_needs_review, fetch_memory_snapshot, fetch_validation_run, set_review_status,
    };

    let run = fetch_validation_run(&mut **tx, run_id)
        .await?
        .with_context(|| format!("validation run {run_id} not found"))?;
    crate::repository::lock_validation_source_tx(
        tx,
        run.canonical_id,
        run.memory_id,
        run.project_id,
    )
    .await?;
    crate::repository::lock_validation_run_tx(tx, run_id).await?;
    let run = fetch_validation_run(&mut **tx, run_id)
        .await?
        .context("validation run vanished")?;
    let mut pending_events = Vec::new();
    if run.review_status.as_deref() != Some("pending") {
        anyhow::bail!(
            "validation run {run_id} is not pending review (status: {})",
            run.review_status.as_deref().unwrap_or("none")
        );
    }

    let mut new_memory_id = None;
    if approve {
        let proposal = run
            .proposed_candidate_json
            .as_ref()
            .context("pending review has no proposed candidate")?;
        let previous_memory_id: Uuid = proposal
            .get("previous_memory_id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse().ok())
            .unwrap_or(run.memory_id);
        anyhow::ensure!(
            previous_memory_id == run.memory_id,
            "review input does not match its run"
        );
        let current = fetch_memory_snapshot(&mut **tx, previous_memory_id)
            .await?
            .with_context(|| format!("memory {previous_memory_id} not found"))?;
        let summary = proposal
            .get("proposed_summary")
            .and_then(|value| value.as_str())
            .unwrap_or(&current.summary);
        let text = proposal
            .get("proposed_text")
            .and_then(|value| value.as_str())
            .unwrap_or(&current.canonical_text);
        let applied = mem_curate::prepare_validation_revision_tx(
            tx,
            previous_memory_id,
            summary,
            text,
            &execution.context,
            &execution.actor,
        )
        .await
        .context("apply reviewed correction")?;
        new_memory_id = Some(applied.0);
        pending_events.push(applied.1);
        set_review_status(&mut **tx, run_id, "applied").await?;
        mark_validated(
            &mut **tx,
            run.canonical_id,
            run.confidence.unwrap_or(1.0),
            run_id,
            Utc::now() + chrono::Duration::days(7),
        )
        .await?;
    } else {
        set_review_status(&mut **tx, run_id, "rejected").await?;
    }

    if clear_needs_review(&mut **tx, run.canonical_id).await? {
        insert_score_audit(
            &mut **tx,
            run.canonical_id,
            run.project_id,
            "needs_review_resolved",
            None,
            None,
            serde_json::json!({ "run_id": run_id, "applied": approve }),
        )
        .await?;
    }
    insert_score_audit(
        &mut **tx,
        run.canonical_id,
        run.project_id,
        "validation_completed",
        None,
        None,
        serde_json::json!({
            "run_id": run_id,
            "review": if approve { "applied" } else { "rejected" },
            "new_memory_id": new_memory_id,
        }),
    )
    .await?;

    pending_events.push(crate::repository::prepare_validation_activity(
        run.project_id,
        run.canonical_id,
        new_memory_id.unwrap_or(run.memory_id),
        run_id,
        "Validation review resolved",
        execution,
    ));
    let mut events = Vec::new();
    for event in pending_events {
        let sequence = mem_store::events::append(tx, &event).await?;
        events.push((event, sequence));
    }
    Ok(ReviewResolution {
        run_id,
        applied: approve,
        canonical_id: run.canonical_id,
        project_id: run.project_id,
        new_memory_id,
        events,
    })
}

/// Applies the exact result of a completed dry-run validation preview without
/// rerunning the verdict provider.
pub async fn apply_preview(pool: &PgPool, run_id: Uuid) -> Result<ReviewResolution> {
    apply_preview_as(pool, run_id, &ValidationExecution::default()).await
}

pub async fn apply_preview_as(
    pool: &PgPool,
    run_id: Uuid,
    execution: &ValidationExecution,
) -> Result<ReviewResolution> {
    let mut tx = pool.begin().await?;
    let result = apply_preview_tx(&mut tx, run_id, execution).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn apply_preview_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run_id: Uuid,
    execution: &ValidationExecution,
) -> Result<ReviewResolution> {
    use crate::repository::{
        fetch_memory_snapshot, fetch_validation_run, mark_needs_review, set_review_status,
    };

    let run = fetch_validation_run(&mut **tx, run_id)
        .await?
        .with_context(|| format!("validation run {run_id} not found"))?;
    crate::repository::lock_validation_source_tx(
        tx,
        run.canonical_id,
        run.memory_id,
        run.project_id,
    )
    .await?;
    crate::repository::lock_validation_run_tx(tx, run_id).await?;
    let run = fetch_validation_run(&mut **tx, run_id)
        .await?
        .context("validation run vanished")?;
    let mut pending_events = Vec::new();
    if run.status != "completed" {
        anyhow::bail!("validation run {run_id} is not completed");
    }
    if !run.dry_run {
        anyhow::bail!("validation run {run_id} is not a preview dry run");
    }
    if run.review_status.as_deref() == Some("applied_preview") {
        anyhow::bail!("validation run {run_id} was already applied from preview");
    }

    let cooldown_until = Utc::now() + chrono::Duration::days(7);
    let action = run.action.as_deref().unwrap_or_default();
    let mut new_memory_id = None;
    match action {
        "would_revalidate" => {
            mark_validated(
                &mut **tx,
                run.canonical_id,
                run.confidence.unwrap_or(1.0),
                run_id,
                cooldown_until,
            )
            .await?;
        }
        "would_reword" | "would_queue_correction" => {
            let proposal = run
                .proposed_candidate_json
                .as_ref()
                .context("preview has no proposed candidate")?;
            let previous_memory_id: Uuid = proposal
                .get("previous_memory_id")
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse().ok())
                .unwrap_or(run.memory_id);
            anyhow::ensure!(
                previous_memory_id == run.memory_id,
                "review input does not match its run"
            );
            let current = fetch_memory_snapshot(&mut **tx, previous_memory_id)
                .await?
                .with_context(|| format!("memory {previous_memory_id} not found"))?;
            let summary = proposal
                .get("proposed_summary")
                .and_then(|value| value.as_str())
                .unwrap_or(&current.summary);
            let text = proposal
                .get("proposed_text")
                .and_then(|value| value.as_str())
                .unwrap_or(&current.canonical_text);
            let applied = mem_curate::prepare_validation_revision_tx(
                tx,
                previous_memory_id,
                summary,
                text,
                &execution.context,
                &execution.actor,
            )
            .await
            .context("apply previewed validation correction")?;
            new_memory_id = Some(applied.0);
            pending_events.push(applied.1);
            mark_validated(
                &mut **tx,
                run.canonical_id,
                run.confidence.unwrap_or(1.0),
                run_id,
                cooldown_until,
            )
            .await?;
        }
        "would_flag_needs_review" => {
            let reason = run
                .reasons_json
                .as_array()
                .and_then(|values| values.iter().find_map(|value| value.as_str()))
                .unwrap_or("weak or contradictory evidence");
            mark_needs_review(&mut **tx, run.canonical_id, reason, run_id, cooldown_until).await?;
        }
        other => anyhow::bail!("validation run action `{other}` cannot be applied from preview"),
    }
    set_review_status(&mut **tx, run_id, "applied_preview").await?;
    insert_score_audit(
        &mut **tx,
        run.canonical_id,
        run.project_id,
        "validation_completed",
        None,
        None,
        serde_json::json!({
            "run_id": run_id,
            "review": "applied_preview",
            "new_memory_id": new_memory_id,
        }),
    )
    .await?;

    pending_events.push(crate::repository::prepare_validation_activity(
        run.project_id,
        run.canonical_id,
        new_memory_id.unwrap_or(run.memory_id),
        run_id,
        "Validation preview applied",
        execution,
    ));
    let mut events = Vec::new();
    for event in pending_events {
        let sequence = mem_store::events::append(tx, &event).await?;
        events.push((event, sequence));
    }
    Ok(ReviewResolution {
        run_id,
        applied: true,
        canonical_id: run.canonical_id,
        project_id: run.project_id,
        new_memory_id,
        events,
    })
}

#[cfg(test)]
pub mod test_support {
    use super::evidence::ValidationContext;
    use crate::repository::MemorySnapshot;
    use chrono::Utc;
    use uuid::Uuid;

    /// Bare context for unit tests: no DB, the given paths allowed as
    /// evidence references.
    pub fn minimal_context(allowed_paths: &[&str]) -> ValidationContext {
        let mut context = ValidationContext {
            memory: MemorySnapshot {
                memory_id: Uuid::new_v4(),
                canonical_id: Uuid::new_v4(),
                project_id: Uuid::new_v4(),
                project_slug: "test".to_string(),
                repo_root: String::new(),
                summary: "Test memory".to_string(),
                canonical_text: "Test memory canonical text.".to_string(),
                memory_type: "implementation".to_string(),
                importance: 3,
                confidence: 0.9,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            tags: Vec::new(),
            sources: Vec::new(),
            proof_snippets: Vec::new(),
            related: Vec::new(),
            prior_runs: Vec::new(),
            git_log: Vec::new(),
            allowed_refs: Default::default(),
            proof_scope: mem_record::ValidationProofScope::SourceFilesFirst,
            proof_fallback_used: false,
        };
        for path in allowed_paths {
            context.insert_allowed_reference(path);
        }
        context
    }
}
