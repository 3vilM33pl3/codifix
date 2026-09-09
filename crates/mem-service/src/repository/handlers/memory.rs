// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::prelude::*;
use crate::*;
pub mod mutations;
pub mod retention;

fn mutation_error(error: sqlx::Error) -> ApiError {
    if matches!(error, sqlx::Error::RowNotFound) {
        ApiError::not_found("memory or project not found")
    } else {
        ApiError::sql(error)
    }
}

fn mutation_context(principal: &AuthenticatedPrincipal) -> mem_store::events::MutationContext {
    mem_store::events::MutationContext {
        actor_id: principal.id.to_string(),
        actor_name: principal.display_name.clone(),
        source: "memory-mutation".into(),
        occurred_at: None,
    }
}

pub async fn fetch_memory_entry(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<MemoryEntryResponse>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT p.slug, m.id, m.canonical_text, m.summary, m.memory_type,
               cs.importance, cs.confidence, cs.status,
               m.created_at, m.updated_at,
               m.canonical_id, m.version_no, m.is_tombstone
        FROM memory_entries m
        JOIN memory_canonical_state cs ON cs.canonical_id = m.canonical_id
        JOIN projects p ON p.id = m.project_id
        WHERE m.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let tags = sqlx::query("SELECT tag FROM memory_tags WHERE memory_entry_id = $1 ORDER BY tag")
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.try_get::<String, _>("tag"))
        .collect::<Result<Vec<_>, _>>()?;

    let sources = sqlx::query(
        r#"
        SELECT ms.id, ms.task_id, ms.file_path, ms.git_commit, ms.symbol_name, ms.symbol_kind,
               ms.source_kind, ms.excerpt,
               v.status AS provenance_status,
               v.checked_at AS provenance_checked_at,
               v.reason AS provenance_reason,
               v.resolved_path AS provenance_resolved_path
        FROM memory_sources ms
        LEFT JOIN memory_source_verifications v ON v.source_id = ms.id
        WHERE ms.memory_entry_id = $1
        ORDER BY ms.created_at ASC
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(MemorySourceRecord {
            id: row.try_get("id")?,
            task_id: row.try_get("task_id")?,
            file_path: row.try_get("file_path")?,
            git_commit: row.try_get("git_commit")?,
            symbol_name: row.try_get("symbol_name")?,
            symbol_kind: row.try_get("symbol_kind")?,
            source_kind: parse_source_kind(&row.try_get::<String, _>("source_kind")?),
            excerpt: row.try_get("excerpt")?,
            provenance: source_provenance_from_row(&row)?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;

    let related_memories = sqlx::query(
        r#"
        SELECT mr.relation_type, m.id, m.summary, m.memory_type,
               memory_state_confidence(m.canonical_id) AS confidence
        FROM memory_relations mr
        JOIN memory_entries m ON m.id = memory_latest_version_id(mr.dst_canonical_id)
        WHERE mr.src_canonical_id = (SELECT canonical_id FROM memory_entries WHERE id = $1)
        ORDER BY m.updated_at DESC, m.id
        LIMIT 12
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(RelatedMemorySummary {
            memory_id: row.try_get("id")?,
            relation_type: parse_relation_type(&row.try_get::<String, _>("relation_type")?),
            summary: row.try_get("summary")?,
            memory_type: parse_memory_type(&row.try_get::<String, _>("memory_type")?),
            confidence: row.try_get("confidence")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;

    let embedding_spaces = fetch_memory_embedding_spaces(pool, id).await?;

    Ok(Some(MemoryEntryResponse {
        id,
        project: row.try_get("slug")?,
        canonical_text: row.try_get("canonical_text")?,
        summary: row.try_get("summary")?,
        memory_type: parse_memory_type(&row.try_get::<String, _>("memory_type")?),
        importance: row.try_get("importance")?,
        confidence: row.try_get("confidence")?,
        status: match row.try_get::<String, _>("status")?.as_str() {
            "archived" => mem_record::MemoryStatus::Archived,
            _ => mem_record::MemoryStatus::Active,
        },
        tags,
        sources,
        related_memories,
        embedding_spaces,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        canonical_id: row.try_get("canonical_id")?,
        version_no: row.try_get("version_no")?,
        is_tombstone: row.try_get("is_tombstone")?,
    }))
}

pub(crate) fn source_provenance_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<SourceProvenanceRecord>, sqlx::Error> {
    let Some(status) = row.try_get::<Option<String>, _>("provenance_status")? else {
        return Ok(None);
    };
    Ok(Some(SourceProvenanceRecord {
        status: parse_source_provenance_status(&status),
        checked_at: row.try_get("provenance_checked_at")?,
        reason: row.try_get("provenance_reason")?,
        resolved_path: row.try_get("provenance_resolved_path")?,
    }))
}

pub(crate) fn parse_source_provenance_status(value: &str) -> SourceProvenanceStatus {
    match value {
        "verified" => SourceProvenanceStatus::Verified,
        "missing_file" => SourceProvenanceStatus::MissingFile,
        "missing_symbol" => SourceProvenanceStatus::MissingSymbol,
        "stale" => SourceProvenanceStatus::Stale,
        _ => SourceProvenanceStatus::Unverifiable,
    }
}

pub(crate) async fn fetch_memory_embedding_spaces(
    pool: &PgPool,
    memory_id: Uuid,
) -> Result<Vec<mem_record::MemoryEmbeddingSpace>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT mce.embedding_provider,
               mce.embedding_model,
               mce.embedding_base_url,
               COUNT(*)::bigint         AS chunk_count,
               MAX(mce.embedding_updated_at) AS last_updated
        FROM memory_chunk_embeddings mce
        JOIN memory_chunks mc ON mc.id = mce.chunk_id
        WHERE mc.memory_entry_id = $1
        GROUP BY mce.embedding_provider, mce.embedding_model, mce.embedding_base_url
        ORDER BY last_updated DESC NULLS LAST,
                 mce.embedding_provider,
                 mce.embedding_model
        "#,
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(mem_record::MemoryEmbeddingSpace {
                provider: row.try_get("embedding_provider")?,
                model: row.try_get("embedding_model")?,
                base_url: row.try_get("embedding_base_url")?,
                chunk_count: row.try_get("chunk_count")?,
                last_updated: row.try_get("last_updated")?,
            })
        })
        .collect()
}

pub(crate) async fn get_memory(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryEntryResponse>, ApiError> {
    let detail = fetch_memory_entry(&state.pool()?, id)
        .await
        .map_err(ApiError::sql)?
        .ok_or_else(|| ApiError::not_found("memory entry not found"))?;
    crate::reinforcement::record_direct_read(&state, detail.id);
    Ok(Json(detail))
}

pub(crate) async fn get_memory_history(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MemoryHistoryResponse>, ApiError> {
    let pool = &state.pool()?;
    // Walk back to the canonical_id of the provided version, then pull every
    // sibling version in chronological order. The caller can pass any
    // version's id (including a tombstone) and get the same chain.
    let anchor = sqlx::query(
        r#"
        SELECT m.canonical_id, p.slug
        FROM memory_entries m
        JOIN projects p ON p.id = m.project_id
        WHERE m.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::sql)?
    .ok_or_else(|| ApiError::not_found("memory entry not found"))?;

    let canonical_id: Uuid = anchor.try_get("canonical_id").map_err(ApiError::sql)?;
    let project: String = anchor.try_get("slug").map_err(ApiError::sql)?;

    let version_ids: Vec<Uuid> = sqlx::query(
        r#"
        SELECT id
        FROM memory_entries
        WHERE canonical_id = $1
        ORDER BY version_no ASC
        "#,
    )
    .bind(canonical_id)
    .fetch_all(pool)
    .await
    .map_err(ApiError::sql)?
    .into_iter()
    .map(|row| row.try_get::<Uuid, _>("id"))
    .collect::<Result<Vec<_>, _>>()
    .map_err(ApiError::sql)?;

    let mut versions = Vec::with_capacity(version_ids.len());
    for version_id in version_ids {
        if let Some(entry) = fetch_memory_entry(pool, version_id)
            .await
            .map_err(ApiError::sql)?
        {
            versions.push(entry);
        }
    }

    Ok(Json(MemoryHistoryResponse {
        canonical_id,
        project,
        versions,
    }))
}

pub(crate) async fn archive(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Json(request): Json<ArchiveRequest>,
) -> Result<Json<ArchiveResponse>, ApiError> {
    request.validate().map_err(ApiError::validation)?;
    let pool = state.pool()?;
    if request.dry_run {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_canonical_state cs JOIN projects p ON p.id=cs.project_id WHERE p.slug=$1 AND cs.status='active' AND cs.confidence<=$2 AND cs.importance<=$3")
            .bind(&request.project).bind(request.max_confidence).bind(request.max_importance).fetch_one(&pool).await.map_err(mutation_error)?;
        return Ok(Json(ArchiveResponse {
            archived_count: count as u64,
            dry_run: true,
        }));
    }
    let mutation = mutations::archive_bulk_as(&pool, &request, &mutation_context(&principal))
        .await
        .map_err(mutation_error)?;
    crate::repository::events::broadcast_committed(
        &state,
        request.project,
        mutation.event,
        mutation.sequence,
    );
    Ok(Json(mutation.response))
}

pub(crate) async fn archive_memory(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Path(id): Path<Uuid>,
) -> Result<Json<ArchiveMemoryResponse>, ApiError> {
    if id.is_nil() {
        return Err(ApiError::validation(ValidationError::new(
            "memory id must be non-nil",
        )));
    }
    let mutation = mutations::archive_one_as(&state.pool()?, id, &mutation_context(&principal))
        .await
        .map_err(mutation_error)?;
    crate::repository::events::broadcast_committed(
        &state,
        mutation.response.project.clone(),
        mutation.event,
        mutation.sequence,
    );
    Ok(Json(mutation.response))
}

pub(crate) async fn delete_memory(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Json(request): Json<DeleteMemoryRequest>,
) -> Result<Json<DeleteMemoryResponse>, ApiError> {
    request.validate().map_err(ApiError::validation)?;
    let mutation = mutations::tombstone_as(
        &state.pool()?,
        request.memory_id,
        &mutation_context(&principal),
    )
    .await
    .map_err(mutation_error)?;
    crate::repository::events::broadcast_committed(
        &state,
        mutation.response.project.clone(),
        mutation.event,
        mutation.sequence,
    );
    Ok(Json(mutation.response))
}

pub(crate) async fn prune_history(
    State(state): State<AppState>,
    axum::Extension(principal): axum::Extension<AuthenticatedPrincipal>,
    Json(request): Json<PruneHistoryRequest>,
) -> Result<Json<PruneHistoryResponse>, ApiError> {
    // Fill missing thresholds from server config so the caller can rely on
    // either source without duplicating the logic in every client.
    let tombstone_after = request
        .tombstone_after
        .or(state.config.retention.tombstone_after);
    let superseded_after = request
        .superseded_after
        .or(state.config.retention.superseded_after);
    let effective = PruneHistoryRequest {
        project: request.project.clone(),
        tombstone_after,
        superseded_after,
        dry_run: request.dry_run,
    };
    effective.validate().map_err(ApiError::validation)?;

    let mutation = retention::prune_as(
        &state.pool()?,
        &effective,
        state.config.retention.operational_events_after,
        &mutation_context(&principal),
    )
    .await
    .map_err(ApiError::sql)?;
    for (project, event, sequence) in mutation.events {
        crate::repository::events::broadcast_committed(&state, project, event, sequence);
    }
    Ok(Json(mutation.response))
}
