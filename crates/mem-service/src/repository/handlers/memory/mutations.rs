// SPDX-License-Identifier: AGPL-3.0-or-later
//! SQL-only knowledge mutations. Events are appended after domain writes;
//! pool wrappers commit before returning provisional event metadata to callers.
use mem_record::{
    ActivityKind, ArchiveMemoryResponse, ArchiveRequest, ArchiveResponse, DeleteMemoryResponse,
};
use mem_store::events::{MutationContext, TimelineEvent};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub struct Mutation<T> {
    pub response: T,
    pub event: TimelineEvent,
    pub sequence: i64,
}
async fn append<T>(
    tx: &mut Transaction<'_, Postgres>,
    response: T,
    mut event: TimelineEvent,
    context: &MutationContext,
) -> Result<Mutation<T>, sqlx::Error> {
    context.stamp(&mut event);
    let sequence = mem_store::events::append(tx, &event)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    Ok(Mutation {
        response,
        event,
        sequence,
    })
}
pub async fn archive_bulk_tx(
    tx: &mut Transaction<'_, Postgres>,
    request: &ArchiveRequest,
    context: &MutationContext,
) -> Result<Mutation<ArchiveResponse>, sqlx::Error> {
    if request.dry_run {
        return Err(sqlx::Error::Protocol(
            "use the read-only archive preview".into(),
        ));
    }
    let project: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(&request.project)
        .fetch_one(&mut **tx)
        .await?;
    let count = sqlx::query("UPDATE memory_canonical_state SET status='archived',archived_at=now(),updated_at=now() WHERE project_id=$1 AND status='active' AND confidence<=$2 AND importance<=$3")
        .bind(project).bind(request.max_confidence).bind(request.max_importance).execute(&mut **tx).await?.rows_affected();
    let mut event = TimelineEvent::new(
        project,
        ActivityKind::Archive,
        format!("Archived {count} low-value memory entries"),
    );
    event.details = Some(mem_record::ActivityDetails::Archive {
        archived_count: count,
        max_confidence: request.max_confidence,
        max_importance: request.max_importance,
    });
    append(
        tx,
        ArchiveResponse {
            archived_count: count,
            dry_run: false,
        },
        event,
        context,
    )
    .await
}
pub async fn archive_bulk_as(
    pool: &PgPool,
    request: &ArchiveRequest,
    context: &MutationContext,
) -> Result<Mutation<ArchiveResponse>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = archive_bulk_tx(&mut tx, request, context).await?;
    tx.commit().await?;
    Ok(result)
}
pub async fn archive_one_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    context: &MutationContext,
) -> Result<Mutation<ArchiveMemoryResponse>, sqlx::Error> {
    let row = sqlx::query("UPDATE memory_canonical_state cs SET status='archived',archived_at=COALESCE(cs.archived_at,now()),updated_at=now() FROM memory_entries m JOIN projects p ON p.id=m.project_id WHERE m.id=$1 AND cs.canonical_id=m.canonical_id RETURNING p.slug,m.project_id,m.canonical_id,m.version_no,m.summary")
        .bind(id).fetch_one(&mut **tx).await?;
    let mut event = TimelineEvent::new(
        row.try_get("project_id")?,
        ActivityKind::Archive,
        format!("Archived memory {id}"),
    );
    event.memory_id = Some(id);
    event.canonical_id = Some(row.try_get("canonical_id")?);
    event.version_no = Some(row.try_get("version_no")?);
    let response = ArchiveMemoryResponse {
        memory_id: id,
        project: row.try_get("slug")?,
        summary: row.try_get("summary")?,
        archived: true,
    };
    append(tx, response, event, context).await
}
pub async fn archive_one_as(
    pool: &PgPool,
    id: Uuid,
    context: &MutationContext,
) -> Result<Mutation<ArchiveMemoryResponse>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = archive_one_tx(&mut tx, id, context).await?;
    tx.commit().await?;
    Ok(result)
}
pub async fn tombstone_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    context: &MutationContext,
) -> Result<Mutation<DeleteMemoryResponse>, sqlx::Error> {
    let row = sqlx::query("SELECT m.project_id,m.canonical_id,m.summary,p.slug FROM memory_entries m JOIN projects p ON p.id=m.project_id JOIN memory_canonical_state cs ON cs.canonical_id=m.canonical_id WHERE m.id=$1 FOR UPDATE OF cs")
        .bind(id).fetch_one(&mut **tx).await?;
    let project: Uuid = row.try_get("project_id")?;
    let canonical: Uuid = row.try_get("canonical_id")?;
    let latest: i32 =
        sqlx::query_scalar("SELECT MAX(version_no) FROM memory_entries WHERE canonical_id=$1")
            .bind(canonical)
            .fetch_one(&mut **tx)
            .await?;
    let version = latest
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("version limit".into()))?;
    let memory_id = Uuid::new_v4();
    sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$3,$4,true,'','','implementation','project',0,0,now(),now(),to_tsvector('english',''))")
        .bind(memory_id).bind(project).bind(canonical).bind(version).execute(&mut **tx).await?;
    let mut event = TimelineEvent::new(
        project,
        ActivityKind::DeleteMemory,
        format!("Tombstoned memory {canonical}"),
    );
    event.memory_id = Some(memory_id);
    event.canonical_id = Some(canonical);
    event.version_no = Some(version);
    let response = DeleteMemoryResponse {
        memory_id,
        project: row.try_get("slug")?,
        summary: row.try_get("summary")?,
        deleted: true,
    };
    event.details = Some(mem_record::ActivityDetails::DeleteMemory {
        deleted: true,
        summary: response.summary.clone(),
    });
    append(tx, response, event, context).await
}
pub async fn tombstone_as(
    pool: &PgPool,
    id: Uuid,
    context: &MutationContext,
) -> Result<Mutation<DeleteMemoryResponse>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = tombstone_tx(&mut tx, id, context).await?;
    tx.commit().await?;
    Ok(result)
}
