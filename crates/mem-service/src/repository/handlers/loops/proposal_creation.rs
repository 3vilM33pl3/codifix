// SPDX-License-Identifier: AGPL-3.0-or-later

//! Atomic proposal creation for direct callers and leased worker transactions.
//! Callers supply a stable operation ID on retries and check current authority.
//! Receipts fingerprint original input; human edits never change that identity.

use anyhow::{Result, ensure};
use mem_record::{
    ActivityKind, LoopMemoryProposalCreateRequest, LoopMemoryProposalDecisionResponse,
};
use mem_store::events::{MutationContext, TimelineEvent};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub struct ProposalMutation {
    pub response: LoopMemoryProposalDecisionResponse,
    /// Broadcast only after the caller commits. None on duplicate delivery.
    pub event: Option<(TimelineEvent, i64)>,
}

pub async fn create(
    pool: &PgPool,
    request: &LoopMemoryProposalCreateRequest,
    operation: Uuid,
    context: &MutationContext,
) -> Result<ProposalMutation> {
    let mut tx = pool.begin().await?;
    let mutation = create_tx(&mut tx, request, operation, context).await?;
    tx.commit().await?;
    Ok(mutation)
}

/// No pool writes or commit. Append is last, after proposal/receipt/run locks.
/// On error the caller must roll back, including when using a JobResult fence.
pub async fn create_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &LoopMemoryProposalCreateRequest,
    operation: Uuid,
    context: &MutationContext,
) -> Result<ProposalMutation> {
    request.validate()?;
    ensure!(!operation.is_nil(), "nil proposal operation");
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            request,
            &context.actor_id,
            &context.source,
        ))?)
    );
    // Serialize first delivery even before its receipt exists.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("proposal-creation:{operation}"))
        .execute(&mut **tx)
        .await?;
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(&request.project)
        .fetch_one(&mut **tx)
        .await?;
    if let Some(receipt) = sqlx::query("SELECT project_id,proposal_id,input_digest FROM proposal_creation_receipts WHERE operation_id=$1")
        .bind(operation).fetch_optional(&mut **tx).await? {
        ensure!(receipt.try_get::<Uuid,_>("project_id")? == project
            && receipt.try_get::<String,_>("input_digest")? == digest,
            "proposal operation input mismatch");
        let proposal_id: Uuid = receipt.try_get("proposal_id")?;
        let row = sqlx::query("SELECT mp.*,p.slug AS project FROM memory_proposals mp JOIN projects p ON p.id=mp.project_id WHERE mp.id=$1")
            .bind(proposal_id).fetch_one(&mut **tx).await?;
        return Ok(ProposalMutation {
            response: LoopMemoryProposalDecisionResponse {
                proposal: super::row_to_memory_proposal(row).map_err(|e| anyhow::anyhow!(e.message))?,
                memory_id: None,
            },
            event: None,
        });
    }
    if let Some(run) = request.run_id {
        let run_project: Option<Uuid> =
            sqlx::query_scalar("SELECT project_id FROM loop_runs WHERE id=$1 FOR UPDATE")
                .bind(run)
                .fetch_one(&mut **tx)
                .await?;
        ensure!(
            run_project == Some(project),
            "proposal run belongs to another project"
        );
    }
    if let Some(target) = request.target_memory_id {
        let target_project: Uuid =
            sqlx::query_scalar("SELECT project_id FROM memory_entries WHERE id=$1 FOR KEY SHARE")
                .bind(target)
                .fetch_one(&mut **tx)
                .await?;
        ensure!(
            target_project == project,
            "proposal target belongs to another project"
        );
    }
    let id = Uuid::new_v5(&operation, b"memory-proposal-v1");
    let row = sqlx::query("INSERT INTO memory_proposals(id,run_id,project_id,loop_id,proposal_type,target_memory_id,candidate_json,evidence_json,confidence,risk_notes,status,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending',now()) RETURNING *, (SELECT slug FROM projects WHERE id=memory_proposals.project_id) AS project")
        .bind(id).bind(request.run_id).bind(project).bind(&request.loop_id).bind(&request.proposal_type)
        .bind(request.target_memory_id).bind(&request.candidate).bind(&request.evidence)
        .bind(request.confidence).bind(&request.risk_notes).fetch_one(&mut **tx).await?;
    let proposal = super::row_to_memory_proposal(row).map_err(|e| anyhow::anyhow!(e.message))?;
    sqlx::query("INSERT INTO proposal_creation_receipts(operation_id,project_id,proposal_id,input_digest) VALUES($1,$2,$3,$4)")
        .bind(operation).bind(project).bind(id).bind(digest).execute(&mut **tx).await?;
    if let Some(run) = request.run_id {
        // The owning run lock also serializes the existing trace append helper.
        sqlx::query("INSERT INTO run_traces(id,run_id,sequence,trace_type,title,payload_json,redacted,created_at) SELECT $1,$2,COALESCE(MAX(sequence),0)+1,'memory_proposal','Memory proposal created',$3,false,now() FROM run_traces WHERE run_id=$2")
            .bind(Uuid::new_v5(&operation,b"proposal-created-trace-v1")).bind(run)
            .bind(serde_json::json!({"proposal_id":id,"proposal_type":request.proposal_type,"status":"pending","actor_id":context.actor_id}))
            .execute(&mut **tx).await?;
        sqlx::query("UPDATE loop_runs SET trace_count=trace_count+1,updated_at=now() WHERE id=$1")
            .bind(run)
            .execute(&mut **tx)
            .await?;
    }
    let mut event = TimelineEvent::new(
        project,
        ActivityKind::ProposalCreated,
        format!(
            "Loop {} proposed {} ({id})",
            request.loop_id, request.proposal_type
        ),
    );
    event.id = Uuid::new_v5(&operation, b"proposal-created-event-v1");
    event.operation_id = Some(operation.to_string());
    context.stamp(&mut event);
    let sequence = mem_store::events::append(tx, &event).await?;
    Ok(ProposalMutation {
        response: LoopMemoryProposalDecisionResponse {
            proposal,
            memory_id: None,
        },
        event: Some((event, sequence)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture(pool: &PgPool) -> (Uuid, LoopMemoryProposalCreateRequest) {
        let project = Uuid::new_v4();
        let run = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'R1 fixture','/disposable')",
        )
        .bind(project)
        .bind(project.to_string())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status) VALUES($1,'memory.consolidation',1,$2,'project',$3,'manual','completed')")
            .bind(run).bind(project).bind(project.to_string()).execute(pool).await.unwrap();
        (
            project,
            LoopMemoryProposalCreateRequest {
                project: project.to_string(),
                loop_id: "memory.consolidation".into(),
                proposal_type: "consolidate".into(),
                run_id: Some(run),
                target_memory_id: None,
                candidate: serde_json::json!({"summary":"private proposed output"}),
                evidence: serde_json::json!([]),
                confidence: 0.7,
                risk_notes: None,
            },
        )
    }

    async fn counts(pool: &PgPool, project: Uuid, run: Uuid) -> (i64, i64, i64, i32) {
        sqlx::query_as("SELECT (SELECT count(*) FROM memory_proposals WHERE project_id=$1), (SELECT count(*) FROM proposal_creation_receipts WHERE project_id=$1), (SELECT count(*) FROM project_timeline_events WHERE project_id=$1), (SELECT trace_count FROM loop_runs WHERE id=$2)")
            .bind(project).bind(run).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn creation_replay_preserves_human_edits_and_rejects_changed_inputs() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (project, request) = fixture(&pool).await;
        let context = MutationContext::internal("consolidation");
        let operation = Uuid::new_v4();
        let (a, b) = tokio::join!(
            create(&pool, &request, operation, &context),
            create(&pool, &request, operation, &context)
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(a.response.proposal.id, b.response.proposal.id);
        assert_ne!(a.event.is_some(), b.event.is_some());
        assert_eq!(
            counts(&pool, project, request.run_id.unwrap()).await,
            (1, 1, 1, 1)
        );
        let id = a.response.proposal.id;
        sqlx::query(
            "UPDATE memory_proposals SET candidate_json='{}',status='approved' WHERE id=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let replay = create(&pool, &request, operation, &context).await.unwrap();
        assert!(replay.event.is_none());
        assert_eq!(replay.response.proposal.status, "approved");
        assert_eq!(replay.response.proposal.candidate, serde_json::json!({}));
        let mut changed = request.clone();
        changed.candidate = serde_json::json!({"summary":"different output"});
        assert!(create(&pool, &changed, operation, &context).await.is_err());
        assert!(
            create(
                &pool,
                &request,
                operation,
                &MutationContext::internal("another-actor")
            )
            .await
            .is_err()
        );
        assert_eq!(
            counts(&pool, project, request.run_id.unwrap()).await,
            (1, 1, 1, 1)
        );
        let event: (String, String) = sqlx::query_as(
            "SELECT actor_id,summary FROM project_timeline_events WHERE project_id=$1",
        )
        .bind(project)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event.0, context.actor_id);
        assert!(!event.1.contains("private proposed output"));
        mem_test_support::cleanup_project(&pool, &request.project)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn timeline_failure_rolls_back_proposal_receipt_and_run_trace() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (project, mut request) = fixture(&pool).await;
        // Valid DB identifier string, but its final event exceeds the event
        // append boundary. Failure occurs after proposal, receipt and trace.
        request.loop_id = "x".repeat(70_000);
        assert!(
            create(
                &pool,
                &request,
                Uuid::new_v4(),
                &MutationContext::internal("fixture")
            )
            .await
            .is_err()
        );
        assert_eq!(
            counts(&pool, project, request.run_id.unwrap()).await,
            (0, 0, 0, 0)
        );
        let traces: i64 = sqlx::query_scalar("SELECT count(*) FROM run_traces WHERE run_id=$1")
            .bind(request.run_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(traces, 0);
        mem_test_support::cleanup_project(&pool, &request.project)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wrong_project_run_is_rejected_and_lease_loss_rolls_back_proposal() {
        use mem_record::{JobKind, JobStatus};
        use mem_store::jobs::{self, Completion, JobActor, JobPolicy, NewJob};
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (project, request) = fixture(&pool).await;
        let (_, other) = fixture(&pool).await;
        let context = MutationContext::internal("consolidation");
        let operation = Uuid::new_v4();
        let mut wrong = request.clone();
        wrong.run_id = other.run_id;
        assert!(create(&pool, &wrong, operation, &context).await.is_err());
        let policy = JobPolicy::default();
        let job = NewJob {
            id: Uuid::new_v4(),
            project_id: project,
            operation_id: operation,
            kind: JobKind::ConsolidationSynthesis,
            work_key: operation.to_string(),
            actor: JobActor::System("consolidation".into()),
            input: serde_json::json!({"version":1}),
            dependencies: vec![],
        };
        let mut tx = pool.begin().await.unwrap();
        jobs::enqueue(&mut tx, &job).await.unwrap();
        tx.commit().await.unwrap();
        let lease = jobs::claim(&pool, policy).await.unwrap().unwrap();
        assert_eq!(lease.job.id, job.id);
        let mut result = jobs::begin_result(&pool, &lease).await.unwrap();
        create_tx(result.transaction(), &request, operation, &context)
            .await
            .unwrap();
        sqlx::query("UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(job.id).execute(&mut **result.transaction()).await.unwrap();
        assert!(matches!(
            jobs::finish_result(result, Completion::Succeeded, policy).await,
            Err(mem_store::Error::StaleLease)
        ));
        assert_eq!(
            counts(&pool, project, request.run_id.unwrap()).await,
            (0, 0, 0, 0)
        );
        let mut result = jobs::begin_result(&pool, &lease).await.unwrap();
        create_tx(result.transaction(), &request, operation, &context)
            .await
            .unwrap();
        jobs::finish_result(result, Completion::Succeeded, policy)
            .await
            .unwrap();
        assert_eq!(
            jobs::detail(&pool, project, job.id)
                .await
                .unwrap()
                .job
                .status,
            JobStatus::Succeeded
        );
        assert_eq!(
            counts(&pool, project, request.run_id.unwrap()).await,
            (1, 1, 1, 1)
        );
        mem_test_support::cleanup_project(&pool, &request.project)
            .await
            .unwrap();
        mem_test_support::cleanup_project(&pool, &other.project)
            .await
            .unwrap();
    }
}
