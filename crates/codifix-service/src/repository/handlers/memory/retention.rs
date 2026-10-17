// SPDX-License-Identifier: AGPL-3.0-or-later
use codifix_record::{ActivityKind, PruneHistoryRequest, PruneHistoryResponse};
use codifix_store::events::{MutationContext, TimelineEvent};
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

pub struct RetentionMutation {
    pub response: PruneHistoryResponse,
    pub events: Vec<(String, TimelineEvent, i64)>,
}

pub async fn prune_as(
    pool: &PgPool,
    request: &PruneHistoryRequest,
    operational_after: Option<Duration>,
    context: &MutationContext,
) -> Result<RetentionMutation, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mutation = prune_tx(&mut tx, request, operational_after, context).await?;
    tx.commit().await?;
    Ok(mutation)
}

pub async fn prune_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &PruneHistoryRequest,
    operational_after: Option<Duration>,
    context: &MutationContext,
) -> Result<RetentionMutation, sqlx::Error> {
    request
        .validate()
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let projects: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id,slug FROM projects WHERE ($1::text IS NULL OR slug=$1) ORDER BY id",
    )
    .bind(&request.project)
    .fetch_all(&mut **tx)
    .await?;
    let mut response = PruneHistoryResponse {
        project: request.project.clone(),
        canonicals_tombstoned_deleted: 0,
        superseded_versions_pruned: 0,
        dry_run: request.dry_run,
    };
    let mut pending = Vec::new();
    let operation = Uuid::new_v4();
    for (project, slug) in projects {
        // All stream locks precede timeline allocation. A concurrent version
        // insertion cannot invalidate the latest-version retention decision.
        if !request.dry_run {
            sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id=$1 ORDER BY canonical_id FOR UPDATE")
                .bind(project).fetch_all(&mut **tx).await?;
        }
        let mut tombstones = 0;
        let mut superseded = 0;
        let mut operational = 0;
        if let Some(after) = request.tombstone_after {
            let ids: Vec<Uuid> = sqlx::query_scalar("SELECT canonical_id FROM (SELECT DISTINCT ON(canonical_id) canonical_id,is_tombstone,updated_at FROM memory_entries WHERE project_id=$1 ORDER BY canonical_id,version_no DESC) latest WHERE is_tombstone AND updated_at < now()-make_interval(secs=>$2)")
                .bind(project).bind(after.as_secs_f64()).fetch_all(&mut **tx).await?;
            tombstones = ids.len() as u64;
            if !request.dry_run && !ids.is_empty() {
                sqlx::query(
                    "DELETE FROM memory_entries WHERE project_id=$1 AND canonical_id=ANY($2)",
                )
                .bind(project)
                .bind(ids)
                .execute(&mut **tx)
                .await?;
            }
        }
        if let Some(after) = request.superseded_after {
            let ids: Vec<Uuid> = sqlx::query_scalar("SELECT m.id FROM memory_entries m WHERE project_id=$1 AND NOT is_tombstone AND updated_at<now()-make_interval(secs=>$2) AND version_no<(SELECT MAX(version_no) FROM memory_entries newer WHERE newer.canonical_id=m.canonical_id)")
                .bind(project).bind(after.as_secs_f64()).fetch_all(&mut **tx).await?;
            superseded = ids.len() as u64;
            if !request.dry_run && !ids.is_empty() {
                sqlx::query("DELETE FROM memory_entries WHERE project_id=$1 AND id=ANY($2)")
                    .bind(project)
                    .bind(ids)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        if let Some(after) = operational_after
            && !request.dry_run
        {
            // Never prune another project's activity through a scoped request.
            operational = sqlx::query("DELETE FROM project_timeline_events WHERE project_id=$1 AND kind IN ('query','watcher_health') AND recorded_at<now()-make_interval(secs=>$2)")
                .bind(project).bind(after.as_secs_f64()).execute(&mut **tx).await?.rows_affected();
        }
        response.canonicals_tombstoned_deleted += tombstones;
        response.superseded_versions_pruned += superseded;
        if !request.dry_run && tombstones + superseded + operational > 0 {
            let mut event = TimelineEvent::new(
                project,
                ActivityKind::DeleteMemory,
                format!(
                    "Retention pruned {tombstones} tombstoned streams, {superseded} superseded versions and {operational} operational events"
                ),
            );
            event.operation_id = Some(operation.to_string());
            context.stamp(&mut event);
            pending.push((slug, event));
        }
    }
    let mut events = Vec::with_capacity(pending.len());
    for (slug, event) in pending {
        let sequence = codifix_store::events::append(tx, &event)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        events.push((slug, event, sequence));
    }
    Ok(RetentionMutation { response, events })
}
