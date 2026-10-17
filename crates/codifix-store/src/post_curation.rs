// SPDX-License-Identifier: AGPL-3.0-or-later

//! Transactional stage recipe, independent of provider and service policy.
use crate::{
    Result,
    jobs::{self, JobActor, NewJob},
};
use codifix_record::{BackgroundWorkReference, JobKind};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Privileged maintenance entry point. The caller must already be authorized
/// to administer this database. Execution still checks the principal's current
/// project permissions. This creates fresh work, never historic curation rows.
pub async fn request_repair(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    operation: Uuid,
    principal: Uuid,
    versions: &[Uuid],
) -> Result<BackgroundWorkReference> {
    if principal.is_nil() || operation.is_nil() || versions.is_empty() || versions.len() > 200 {
        return Err(crate::Error::Invalid("repair identities or batch size"));
    }
    for version in versions {
        let current: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memory_entries m JOIN memory_canonical_state cs ON cs.canonical_id=m.canonical_id WHERE m.id=$1 AND m.project_id=$2 AND cs.status='active' AND NOT m.is_tombstone AND m.version_no=(SELECT max(version_no) FROM memory_entries WHERE canonical_id=m.canonical_id))")
            .bind(version).bind(project).fetch_one(&mut **tx).await?;
        if !current {
            return Err(crate::Error::Invalid(
                "repair requires current active versions",
            ));
        }
    }
    let work = enqueue(
        tx,
        project,
        operation,
        versions,
        &JobActor::Principal(principal),
    )
    .await?;
    let mut event = crate::events::TimelineEvent::new(
        project,
        codifix_record::ActivityKind::Reembed,
        "Operator requested enrichment repair; completion is tracked in Jobs".into(),
    );
    event.id = Uuid::new_v5(&operation, b"repair-requested");
    let existing: Option<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT project_id,actor_id,source FROM project_timeline_events WHERE id=$1",
    )
    .bind(event.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(existing) = existing {
        if existing
            != (
                project,
                Some(principal.to_string()),
                Some("operator-repair".into()),
            )
        {
            return Err(crate::Error::Conflict);
        }
        return Ok(work);
    }
    event.operation_id = Some(operation.to_string());
    crate::events::MutationContext {
        actor_id: principal.to_string(),
        actor_name: principal.to_string(),
        source: "operator-repair".into(),
        occurred_at: None,
    }
    .stamp(&mut event);
    crate::events::append(tx, &event).await?;
    Ok(work)
}

pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    operation: Uuid,
    versions: &[Uuid],
    actor: &JobActor,
) -> Result<BackgroundWorkReference> {
    // Serialize recipe creation before inserting any version identities. A
    // duplicate operation cannot silently widen the set consumed by its jobs.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("post-curation:{project}:{operation}"))
        .execute(&mut **tx)
        .await?;
    let known: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM background_jobs WHERE project_id=$1 AND operation_id=$2)",
    )
    .bind(project)
    .bind(operation)
    .fetch_one(&mut **tx)
    .await?;
    if known {
        let existing: Vec<Uuid> = sqlx::query_scalar("SELECT memory_id FROM background_work_versions WHERE project_id=$1 AND operation_id=$2 ORDER BY memory_id")
            .bind(project).bind(operation).fetch_all(&mut **tx).await?;
        let mut requested = versions.to_vec();
        requested.sort_unstable();
        requested.dedup();
        if existing != requested {
            return Err(crate::Error::Conflict);
        }
    }
    for version in versions {
        // Validate before committing the immutable source identity.
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM memory_entries WHERE id=$1 AND project_id=$2)",
        )
        .bind(version)
        .bind(project)
        .fetch_one(&mut **tx)
        .await?;
        if !valid {
            return Err(crate::Error::Invalid("background source version"));
        }
        sqlx::query("INSERT INTO background_work_versions(project_id,operation_id,memory_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(project).bind(operation).bind(version).execute(&mut **tx).await?;
    }
    let stages = [
        ("embedding", JobKind::Embedding),
        ("semantic_dedup", JobKind::SemanticDedup),
        ("validation_due", JobKind::ValidationDue),
        ("consolidation", JobKind::Consolidation),
    ];
    let ids: Vec<Uuid> = stages
        .iter()
        .map(|(name, _)| Uuid::new_v5(&operation, name.as_bytes()))
        .collect();
    for (index, (name, kind)) in stages.iter().enumerate() {
        let dependencies = match index {
            1 => vec![ids[0]],
            3 => vec![ids[1], ids[2]],
            _ => vec![],
        };
        jobs::enqueue(
            tx,
            &NewJob {
                id: ids[index],
                project_id: project,
                operation_id: operation,
                kind: *kind,
                work_key: format!("curation:{operation}:{name}"),
                actor: actor.clone(),
                input: serde_json::json!({"version":1,"curation_run_id":operation}),
                dependencies,
            },
        )
        .await?;
    }
    Ok(BackgroundWorkReference {
        operation_id: operation,
        job_ids: ids,
    })
}
