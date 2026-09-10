// SPDX-License-Identifier: AGPL-3.0-or-later

//! Maintenance prepares provider output before opening its result transaction.
//! A failed provider, stale input or failed event leaves the prior index intact.

use crate::{EmbeddingRegistry, EmbeddingService, EmbeddingSpace, Vector, split_search_chunks};
use anyhow::{Result, ensure};
use mem_record::{ActivityDetails, ActivityKind};
use mem_store::events::{MutationContext, TimelineEvent};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub struct MaintenanceResult {
    pub count: u64,
    pub receipt: Option<(TimelineEvent, i64)>,
}

async fn project_id(pool: &PgPool, slug: &str) -> Result<Option<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
            .bind(slug)
            .fetch_optional(pool)
            .await?,
    )
}

async fn append(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    kind: ActivityKind,
    count: u64,
    summary: &str,
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    let mut event = TimelineEvent::new(project, kind, format!("{summary}: {count}"));
    event.details = Some(if event.kind == ActivityKind::Reindex {
        ActivityDetails::Reindex {
            reindexed_entries: count,
        }
    } else {
        ActivityDetails::Reembed {
            reembedded_chunks: count,
        }
    });
    context.stamp(&mut event);
    let seq = mem_store::events::append(tx, &event).await?;
    Ok(MaintenanceResult {
        count,
        receipt: Some((event, seq)),
    })
}

pub async fn rebuild_chunks_as(
    pool: &PgPool,
    project: &str,
    registry: &EmbeddingRegistry,
    backend: Option<&str>,
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    if backend.is_some() {
        return reembed_project_chunks_as(pool, project, registry, backend, context).await;
    }
    rebuild_selected(pool, project, None, registry.iter().collect(), context).await
}

pub(crate) async fn rebuild_selected(
    pool: &PgPool,
    project: &str,
    ids: Option<&[Uuid]>,
    selected: Vec<(&str, &EmbeddingService)>,
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    let Some(project_id) = project_id(pool, project).await? else {
        return Ok(MaintenanceResult {
            count: 0,
            receipt: None,
        });
    };
    let memories: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM memory_entries WHERE project_id = $1
        AND ($2::uuid[] IS NULL OR id = ANY($2)) ORDER BY canonical_id, id",
    )
    .bind(project_id)
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut prepared = Vec::with_capacity(memories.len());
    for memory in &memories {
        prepared.push(crate::rebuild::prepare_selected(pool, project, *memory, &selected).await?);
    }
    let mut tx = pool.begin().await?;
    lock_inputs(&mut tx, project_id, &memories).await?;
    for output in &prepared {
        crate::rebuild::apply(&mut tx, output).await?;
    }
    let result = append(
        &mut tx,
        project_id,
        ActivityKind::Reindex,
        u64::try_from(prepared.len())?,
        "Rebuilt memory indexes",
        context,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

async fn lock_inputs(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    memories: &[Uuid],
) -> Result<()> {
    sqlx::query("SELECT canonical_id FROM memory_canonical_state WHERE project_id = $1 AND canonical_id IN
        (SELECT canonical_id FROM memory_entries WHERE id = ANY($2)) ORDER BY canonical_id FOR UPDATE")
        .bind(project).bind(memories).fetch_all(&mut **tx).await?;
    let locked = sqlx::query("SELECT id FROM memory_entries WHERE project_id = $1 AND id = ANY($2) ORDER BY canonical_id, id FOR UPDATE")
        .bind(project).bind(memories).fetch_all(&mut **tx).await?;
    ensure!(
        locked.len() == memories.len(),
        "embedding inputs changed; retry maintenance"
    );
    Ok(())
}

struct Source {
    memory: Uuid,
    summary: String,
    text: String,
    existing: Vec<(Uuid, String)>,
}
struct Chunk {
    id: Uuid,
    memory: Uuid,
    text: String,
    search: String,
    new: bool,
}
struct EmbeddedChunk {
    id: Uuid,
    space: EmbeddingSpace,
    dimension: i32,
    vector: Vector,
}

pub async fn reembed_project_chunks_as(
    pool: &PgPool,
    project: &str,
    registry: &EmbeddingRegistry,
    backend: Option<&str>,
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    let selected: Vec<(&str, &EmbeddingService)> = match backend {
        Some(name) => vec![(
            name,
            registry
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("unknown embedding backend"))?,
        )],
        None => registry.iter().collect(),
    };
    let Some(project_id) = project_id(pool, project).await? else {
        return Ok(MaintenanceResult {
            count: 0,
            receipt: None,
        });
    };
    let rows = sqlx::query("SELECT m.id, m.summary, m.canonical_text FROM memory_entries m
        JOIN memory_canonical_state cs ON cs.canonical_id = m.canonical_id
        WHERE m.project_id = $1 AND cs.status = 'active' AND NOT m.is_tombstone ORDER BY m.canonical_id, m.id")
        .bind(project_id).fetch_all(pool).await?;
    let mut sources = Vec::new();
    let mut chunks = Vec::new();
    for row in rows {
        let memory: Uuid = row.try_get("id")?;
        let summary: String = row.try_get("summary")?;
        let text: String = row.try_get("canonical_text")?;
        let existing: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, search_text FROM memory_chunks WHERE memory_entry_id = $1 ORDER BY id",
        )
        .bind(memory)
        .fetch_all(pool)
        .await?;
        if existing.is_empty() && !selected.is_empty() {
            for (index, chunk) in split_search_chunks(&summary, &text).into_iter().enumerate() {
                chunks.push(Chunk {
                    id: Uuid::new_v5(
                        &memory,
                        format!("search-chunk-v1:{index}:{chunk}").as_bytes(),
                    ),
                    memory,
                    search: format!("{summary}\n{chunk}"),
                    text: chunk,
                    new: true,
                });
            }
        } else {
            for (id, search) in &existing {
                chunks.push(Chunk {
                    id: *id,
                    memory,
                    text: String::new(),
                    search: search.clone(),
                    new: false,
                });
            }
        }
        sources.push(Source {
            memory,
            summary,
            text,
            existing,
        });
    }
    chunks.sort_by_key(|c| c.id);
    let mut outputs = Vec::new();
    for (_, service) in selected {
        let ids: Vec<Uuid> = chunks.iter().map(|c| c.id).collect();
        let present: Vec<Uuid> = sqlx::query_scalar(
            "SELECT chunk_id FROM memory_chunk_embeddings
            WHERE chunk_id = ANY($1) AND embedding_space = $2 AND embedding_dimension IS NOT NULL",
        )
        .bind(&ids)
        .bind(service.embedding_space_key())
        .fetch_all(pool)
        .await?;
        let present: std::collections::HashSet<_> = present.into_iter().collect();
        let missing: Vec<_> = chunks.iter().filter(|c| !present.contains(&c.id)).collect();
        for batch in missing.chunks(service.batch_size()) {
            let texts: Vec<String> = batch.iter().map(|c| c.search.clone()).collect();
            let embedded =
                crate::rebuild::embed_with_receipt(pool, project_id, project_id, service, &texts)
                    .await?;
            ensure!(
                embedded.vectors.len() == batch.len() && embedded.dimension > 0,
                "invalid embedding batch shape"
            );
            for (chunk, vector) in batch.iter().zip(embedded.vectors) {
                ensure!(
                    vector.as_slice().len() == embedded.dimension as usize
                        && vector.as_slice().iter().all(|x| x.is_finite()),
                    "invalid embedding vector"
                );
                outputs.push(EmbeddedChunk {
                    id: chunk.id,
                    space: embedded.space.clone(),
                    dimension: embedded.dimension,
                    vector,
                });
            }
        }
    }
    let mut tx = pool.begin().await?;
    let ids: Vec<Uuid> = sources.iter().map(|s| s.memory).collect();
    lock_inputs(&mut tx, project_id, &ids).await?;
    for source in &sources {
        let actual: (String, String, String, bool) = sqlx::query_as("SELECT m.summary, m.canonical_text, cs.status, m.is_tombstone
            FROM memory_entries m JOIN memory_canonical_state cs ON cs.canonical_id = m.canonical_id WHERE m.id = $1")
            .bind(source.memory).fetch_one(&mut *tx).await?;
        ensure!(
            actual
                == (
                    source.summary.clone(),
                    source.text.clone(),
                    "active".into(),
                    false
                ),
            "reembedding input changed"
        );
        let existing: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, search_text FROM memory_chunks WHERE memory_entry_id = $1 ORDER BY id",
        )
        .bind(source.memory)
        .fetch_all(&mut *tx)
        .await?;
        ensure!(existing == source.existing, "reembedding chunks changed");
    }
    for chunk in chunks.iter().filter(|c| c.new) {
        sqlx::query(
            "INSERT INTO memory_chunks (id, memory_entry_id, chunk_text, search_text, tsv)
            VALUES ($1,$2,$3,$4,to_tsvector('english',$4))",
        )
        .bind(chunk.id)
        .bind(chunk.memory)
        .bind(&chunk.text)
        .bind(&chunk.search)
        .execute(&mut *tx)
        .await?;
    }
    for output in &outputs {
        sqlx::query("INSERT INTO memory_chunk_embeddings (chunk_id,embedding_space,embedding,embedding_provider,
            embedding_base_url,embedding_model,embedding_dimension,embedding_updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,clock_timestamp()) ON CONFLICT (chunk_id,embedding_space) DO UPDATE SET
            embedding=EXCLUDED.embedding,embedding_provider=EXCLUDED.embedding_provider,embedding_base_url=EXCLUDED.embedding_base_url,
            embedding_model=EXCLUDED.embedding_model,embedding_dimension=EXCLUDED.embedding_dimension,embedding_updated_at=EXCLUDED.embedding_updated_at")
            .bind(output.id).bind(&output.space.space_key).bind(output.vector.clone()).bind(&output.space.provider)
            .bind(&output.space.base_url).bind(&output.space.model).bind(output.dimension).execute(&mut *tx).await?;
    }
    let result = append(
        &mut tx,
        project_id,
        ActivityKind::Reembed,
        u64::try_from(outputs.len())?,
        "Backfilled embeddings",
        context,
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn prune_project_embeddings_as(
    pool: &PgPool,
    project: &str,
    registry: &EmbeddingRegistry,
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    let Some(project_id) = project_id(pool, project).await? else {
        return Ok(MaintenanceResult {
            count: 0,
            receipt: None,
        });
    };
    let keep: Vec<String> = registry
        .iter()
        .map(|(_, e)| e.embedding_space_key())
        .collect();
    let mut tx = pool.begin().await?;
    let result = prune_project_embeddings_tx(&mut tx, project_id, &keep, context).await?;
    tx.commit().await?;
    Ok(result)
}

pub async fn prune_project_embeddings_tx(
    tx: &mut Transaction<'_, Postgres>,
    project: Uuid,
    keep: &[String],
    context: &MutationContext,
) -> Result<MaintenanceResult> {
    let count = sqlx::query("DELETE FROM memory_chunk_embeddings e USING memory_chunks c, memory_entries m, memory_canonical_state cs
        WHERE e.chunk_id=c.id AND c.memory_entry_id=m.id AND cs.canonical_id=m.canonical_id AND m.project_id=$1
        AND cs.status='active' AND NOT m.is_tombstone AND e.embedding_space <> ALL($2)")
        .bind(project).bind(keep).execute(&mut **tx).await?.rows_affected();
    append(
        tx,
        project,
        ActivityKind::Reembed,
        count,
        "Pruned inactive embeddings",
        context,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddingPurpose;
    use crate::embedding_backend::EmbeddingBackend;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct FailingBackend {
        space: EmbeddingSpace,
        calls: Arc<AtomicUsize>,
    }
    #[async_trait::async_trait]
    impl EmbeddingBackend for FailingBackend {
        fn space(&self) -> &EmbeddingSpace {
            &self.space
        }
        async fn embed(&self, _: &[String], _: EmbeddingPurpose) -> Result<Vec<Vector>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("synthetic unavailable provider")
        }
    }

    #[tokio::test]
    async fn late_provider_failure_leaves_all_spaces_and_missing_chunks_untouched() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let project = Uuid::new_v4();
        let memory = Uuid::new_v4();
        let slug = project.to_string();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/synthetic')",
        )
        .bind(project)
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,summary,canonical_text,
            memory_type,scope,importance,confidence,created_at,updated_at,search_document)
            VALUES($1,$2,$1,1,false,'Summary','Source','implementation','project',3,0.9,now(),now(),to_tsvector('english','Source'))")
            .bind(memory).bind(project).execute(&pool).await.unwrap();
        let mut registry =
            crate::test_support::static_embedding_registry(&[("first", 1.), ("failed", 2.)]);
        let calls = Arc::new(AtomicUsize::new(0));
        registry.entries[1].service.backend = Arc::new(FailingBackend {
            space: EmbeddingSpace::new("fake", "local", "failed"),
            calls: calls.clone(),
        });
        let context = MutationContext::internal("maintenance-test");
        assert!(
            reembed_project_chunks_as(&pool, &slug, &registry, None, &context)
                .await
                .is_err()
        );
        assert!(
            rebuild_chunks_as(&pool, &slug, &registry, None, &context)
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM memory_chunks WHERE memory_entry_id=$1")
                .bind(memory)
                .fetch_one(&pool)
                .await
                .unwrap();
        let events: i64 =
            sqlx::query_scalar("SELECT count(*) FROM project_timeline_events WHERE project_id=$1")
                .bind(project)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((count, events), (0, 0));
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
    }
}
