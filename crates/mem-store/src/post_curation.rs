// SPDX-License-Identifier: AGPL-3.0-or-later

//! Transactional stage recipe, independent of provider and service policy.
use crate::{
    Result,
    jobs::{self, JobActor, NewJob},
};
use mem_record::{BackgroundWorkReference, JobKind};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    operation: Uuid,
    versions: &[Uuid],
    actor: &JobActor,
) -> Result<BackgroundWorkReference> {
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
