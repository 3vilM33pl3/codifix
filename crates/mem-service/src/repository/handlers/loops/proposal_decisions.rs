// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reviewed proposal writes, learning, traces, enrichment and activity share one commit.
use super::*;
use mem_record::DeviceContributionDecision;
use mem_store::{
    events::{MutationContext, TimelineEvent},
    jobs::JobActor,
};
use proposal_creation::ProposalMutation;

pub(crate) async fn edit(
    pool: &PgPool,
    id: Uuid,
    request: &LoopMemoryProposalDecisionRequest,
    context: &MutationContext,
) -> Result<ProposalMutation, ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let mutation = edit_tx(&mut tx, id, request, context).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(mutation)
}

pub(crate) async fn edit_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    request: &LoopMemoryProposalDecisionRequest,
    context: &MutationContext,
) -> Result<ProposalMutation, ApiError> {
    if request.edited_candidate.is_none()
        && request.edited_evidence.is_none()
        && request.edited_risk_notes.is_none()
    {
        return Err(ApiError::validation(ValidationError::new(
            "at least one edited proposal field is required",
        )));
    }
    let locked = lock_memory_proposal(tx, id).await?;
    if !matches!(locked.record.status.as_str(), "pending" | "edited") {
        return Err(ApiError::validation(ValidationError::new(
            "resolved proposals cannot be edited",
        )));
    }
    sqlx::query(
        "UPDATE memory_proposals SET candidate_json=COALESCE($2,candidate_json),
        evidence_json=COALESCE($3,evidence_json),risk_notes=COALESCE($4,risk_notes),
        status='pending',resolved_at=NULL,was_edited=TRUE WHERE id=$1",
    )
    .bind(id)
    .bind(&request.edited_candidate)
    .bind(&request.edited_evidence)
    .bind(&request.edited_risk_notes)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    let proposal = lock_memory_proposal(tx, id).await?;
    finish(tx, proposal, None, "edited", context).await
}

pub(crate) async fn decide(
    pool: &PgPool,
    procedural: &mem_config::ProceduralConfig,
    id: Uuid,
    status: &str,
    request: &LoopMemoryProposalDecisionRequest,
    context: &MutationContext,
    actor: &JobActor,
) -> Result<ProposalMutation, ApiError> {
    let mut tx = pool.begin().await.map_err(ApiError::sql)?;
    let mutation = decide_tx(&mut tx, procedural, id, status, request, context, actor).await?;
    tx.commit().await.map_err(ApiError::sql)?;
    Ok(mutation)
}

pub(crate) async fn decide_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    procedural: &mem_config::ProceduralConfig,
    id: Uuid,
    status: &str,
    _request: &LoopMemoryProposalDecisionRequest,
    context: &MutationContext,
    actor: &JobActor,
) -> Result<ProposalMutation, ApiError> {
    if !matches!(status, "approved" | "rejected") {
        return Err(ApiError::validation(ValidationError::new(
            "proposal decision must be approved or rejected",
        )));
    }
    let proposal = lock_memory_proposal(tx, id).await?;
    if status == "rejected" && proposal.record.status == "rejected" {
        return Ok(ProposalMutation {
            response: LoopMemoryProposalDecisionResponse {
                proposal: proposal.record,
                memory_id: None,
            },
            event: None,
        });
    }
    if !matches!(proposal.record.status.as_str(), "pending" | "edited") {
        return Err(ApiError::validation(ValidationError::new(
            "proposal is already resolved",
        )));
    }
    let memory_id = if status == "approved" {
        apply_memory_proposal(tx, &proposal).await?
    } else {
        None
    };
    if let Some(memory) = memory_id
        && matches!(
            proposal.record.proposal_type.as_str(),
            "add" | "update" | "consolidate"
        )
    {
        mem_curate::rebuild_memory_chunks_tx(tx, memory)
            .await
            .map_err(ApiError::sql)?;
        mem_store::post_curation::enqueue(tx, proposal.project_id, id, &[memory], actor)
            .await
            .map_err(|e| ApiError::io(e.into()))?;
    }
    let reward = if status == "rejected" {
        mem_reinforce::RewardEvent::ProposalRejected
    } else if proposal.was_edited {
        mem_reinforce::RewardEvent::ProposalEditedApproved
    } else {
        mem_reinforce::RewardEvent::ProposalApproved
    };
    mem_store::device_contributions::record_decision(
        tx,
        id,
        if status == "approved" {
            DeviceContributionDecision::Approved
        } else {
            DeviceContributionDecision::Rejected
        },
        memory_id,
        &context.actor_id,
    )
    .await
    .map_err(|error| ApiError::io(error.into()))?;
    emit_proposal_reward(tx, procedural, &proposal, reward).await?;
    sqlx::query("UPDATE memory_proposals SET status=$2,resolved_at=now() WHERE id=$1")
        .bind(id)
        .bind(status)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
    let proposal = lock_memory_proposal(tx, id).await?;
    finish(tx, proposal, memory_id, status, context).await
}

async fn finish(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    proposal: LockedMemoryProposal,
    memory_id: Option<Uuid>,
    action: &str,
    context: &MutationContext,
) -> Result<ProposalMutation, ApiError> {
    if let Some(run) = proposal.record.run_id {
        append_loop_trace_tx(
            tx,
            run,
            "memory_proposal",
            &format!("Memory proposal {action}"),
            json!({"proposal_id":proposal.record.id,"proposal_type":proposal.record.proposal_type,
                "status":proposal.record.status,"memory_id":memory_id,"actor_id":context.actor_id}),
            false,
        )
        .await?;
    }
    let mut event = TimelineEvent::new(
        proposal.project_id,
        ActivityKind::ProposalDecided,
        format!("Proposal {} {action}", proposal.record.id),
    );
    event.operation_id = Some(proposal.record.id.to_string());
    event.memory_id = memory_id;
    if let Some(memory) = memory_id {
        let subject: (Uuid, i32) = sqlx::query_as(
            "SELECT canonical_id,version_no FROM memory_entries WHERE id=$1 AND project_id=$2",
        )
        .bind(memory)
        .bind(proposal.project_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::sql)?;
        event.canonical_id = Some(subject.0);
        event.version_no = Some(subject.1);
    }
    context.stamp(&mut event);
    let sequence = mem_store::events::append(tx, &event)
        .await
        .map_err(|e| ApiError::io(e.into()))?;
    Ok(ProposalMutation {
        response: LoopMemoryProposalDecisionResponse {
            proposal: proposal.record,
            memory_id,
        },
        event: Some((event, sequence)),
    })
}

/// Freeze version references when a proposal is created; compare the same map
/// again at approval. Never silently redirect reviewed evidence to newer bytes.
pub(crate) async fn pin_consolidation_members_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    project: Uuid,
    candidate: &serde_json::Value,
    require_pins: bool,
) -> Result<serde_json::Value, ApiError> {
    let members = candidate
        .get("member_canonical_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ApiError::validation(ValidationError::new("consolidation members are required"))
        })?;
    if members.is_empty() || members.len() > 200 {
        return Err(ApiError::validation(ValidationError::new(
            "consolidation requires 1 to 200 members",
        )));
    }
    let mut ids: Vec<Uuid> = members
        .iter()
        .map(|value| {
            value.as_str().and_then(|s| s.parse().ok()).ok_or_else(|| {
                ApiError::validation(ValidationError::new("invalid consolidation member"))
            })
        })
        .collect::<Result<_, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    let locked: Vec<Uuid> = sqlx::query_scalar(
        "SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1
        AND canonical_id=ANY($2) AND status='active' ORDER BY canonical_id FOR UPDATE",
    )
    .bind(project)
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::sql)?;
    if locked.len() != ids.len() {
        return Err(ApiError::validation(ValidationError::new(
            "consolidation member is missing, archived or outside the project",
        )));
    }
    let mut versions = serde_json::Map::new();
    for canonical in ids {
        let latest: Option<(Uuid, bool)> = sqlx::query_as("SELECT id,is_tombstone FROM memory_entries WHERE canonical_id=$1 AND project_id=$2 ORDER BY version_no DESC LIMIT 1")
            .bind(canonical).bind(project).fetch_optional(&mut **tx).await.map_err(ApiError::sql)?;
        let Some((version, false)) = latest else {
            return Err(ApiError::validation(ValidationError::new(
                "consolidation member was deleted",
            )));
        };
        versions.insert(canonical.to_string(), json!(version));
    }
    let actual = serde_json::Value::Object(versions);
    if let Some(expected) = candidate.get("member_version_ids") {
        if expected != &actual {
            return Err(ApiError::validation(ValidationError::new(
                "consolidation input versions changed; create a fresh proposal",
            )));
        }
    } else if require_pins {
        return Err(ApiError::validation(ValidationError::new(
            "legacy consolidation has no reviewed version pins; create a fresh proposal",
        )));
    }
    let mut candidate = candidate.clone();
    candidate["member_version_ids"] = actual;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn decisions_and_edits_cannot_outlive_failed_timeline_or_duplicate_learning() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let project = Uuid::new_v4();
        let run = Uuid::new_v4();
        let slug = project.to_string();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'R1 fixture','/synthetic')",
        )
        .bind(project)
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status)
            VALUES($1,'memory.consolidation',1,$2,'project',$3,'manual','completed')")
            .bind(run).bind(project).bind(&slug).execute(&pool).await.unwrap();
        let context = MutationContext::internal("proposal-test");
        let actor = JobActor::System("local-curation".into());
        let request = LoopMemoryProposalCreateRequest {
            project: slug.clone(),
            loop_id: "memory.consolidation".into(),
            proposal_type: "add".into(),
            run_id: Some(run),
            target_memory_id: None,
            candidate: json!({"summary":"Reviewed memory","canonical_text":"Reviewed source"}),
            evidence: json!([]),
            confidence: 0.9,
            risk_notes: None,
        };
        let created = proposal_creation::create(&pool, &request, Uuid::new_v4(), &context)
            .await
            .unwrap();
        let id = created.response.proposal.id;
        let procedural = mem_config::ProceduralConfig {
            enabled: true,
            ..Default::default()
        };
        let mut bad = context.clone();
        bad.actor_name = "bad\0actor".into();
        let decision = LoopMemoryProposalDecisionRequest {
            reviewer: None,
            reason: None,
            edited_candidate: None,
            edited_evidence: None,
            edited_risk_notes: None,
        };
        assert!(
            decide(&pool, &procedural, id, "approved", &decision, &bad, &actor)
                .await
                .is_err()
        );
        assert!(
            decide(&pool, &procedural, id, "rejected", &decision, &bad, &actor)
                .await
                .is_err()
        );
        let edit_request = LoopMemoryProposalDecisionRequest {
            edited_candidate: Some(json!({"canonical_text":"Changed"})),
            ..decision.clone()
        };
        assert!(edit(&pool, id, &edit_request, &bad).await.is_err());
        for table in [
            "memory_entries",
            "background_jobs",
            "procedural_utility_audit",
        ] {
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE project_id=$1"))
                    .bind(project)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let candidate: serde_json::Value =
            sqlx::query_scalar("SELECT candidate_json FROM memory_proposals WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(candidate, request.candidate);
        let traces: i32 = sqlx::query_scalar("SELECT trace_count FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(traces, 1);
        let approved = decide(
            &pool,
            &procedural,
            id,
            "approved",
            &decision,
            &context,
            &actor,
        )
        .await
        .unwrap();
        assert!(approved.response.memory_id.is_some());
        assert!(
            decide(
                &pool,
                &procedural,
                id,
                "approved",
                &decision,
                &context,
                &actor
            )
            .await
            .is_err()
        );
        assert!(edit(&pool, id, &edit_request, &context).await.is_err());
        let jobs: i64 =
            sqlx::query_scalar("SELECT count(*) FROM background_jobs WHERE project_id=$1")
                .bind(project)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(jobs, 4);
        let second = proposal_creation::create(&pool, &request, Uuid::new_v4(), &context)
            .await
            .unwrap()
            .response
            .proposal
            .id;
        decide(
            &pool,
            &procedural,
            second,
            "rejected",
            &decision,
            &context,
            &actor,
        )
        .await
        .unwrap();
        assert!(
            decide(
                &pool,
                &procedural,
                second,
                "rejected",
                &decision,
                &context,
                &actor
            )
            .await
            .unwrap()
            .event
            .is_none()
        );
        let audits: i64 =
            sqlx::query_scalar("SELECT count(*) FROM procedural_utility_audit WHERE project_id=$1")
                .bind(project)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(audits, 2);
        let member = approved.response.memory_id.unwrap();
        let mut consolidation = request.clone();
        consolidation.proposal_type = "consolidate".into();
        consolidation.candidate =
            json!({"canonical_text":"Derived insight", "member_canonical_ids":[member]});
        let pinned = proposal_creation::create(&pool, &consolidation, Uuid::new_v4(), &context)
            .await
            .unwrap();
        assert_eq!(
            pinned.response.proposal.candidate["member_version_ids"][member.to_string()],
            json!(member)
        );
        mem_curate::apply_validation_revision(&pool, member, "Changed source", "Changed source")
            .await
            .unwrap();
        assert!(
            decide(
                &pool,
                &procedural,
                pinned.response.proposal.id,
                "approved",
                &decision,
                &context,
                &actor
            )
            .await
            .is_err()
        );
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
    }
}
