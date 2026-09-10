// SPDX-License-Identifier: AGPL-3.0-or-later

//! Provider preparation writes only private external-operation receipts, never
//! searchable projections. Applying prepared output uses only the
//! caller's transaction, including when borrowed from a durable job fence.
//! Prepared output is private operational data, NOT an HTTP request contract.

use crate::{EmbeddingPurpose, EmbeddingRegistry, EmbeddingService, Vector, split_search_chunks};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedMemoryChunks {
    version: u32,
    project_id: Uuid,
    memory_id: Uuid,
    summary: String,
    canonical_text: String,
    chunks: Vec<String>,
    embeddings: Vec<PreparedEmbedding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedEmbedding {
    space: String,
    provider: String,
    base_url: String,
    model: String,
    dimension: i32,
    vectors: Vec<Vec<f32>>,
}

impl PreparedMemoryChunks {
    pub fn memory_id(&self) -> Uuid {
        self.memory_id
    }
    pub fn project_id(&self) -> Uuid {
        self.project_id
    }
    pub fn embedding_spaces(&self) -> Vec<&str> {
        self.embeddings.iter().map(|e| e.space.as_str()).collect()
    }
}

/// Prepare one immutable version. Project equality is enforced before any
/// provider call. The worker separately checks current permissions/configuration.
pub async fn prepare_automatic(
    pool: &PgPool,
    project: &str,
    memory: Uuid,
    registry: &EmbeddingRegistry,
    enabled: bool,
) -> Result<PreparedMemoryChunks> {
    let selected = if enabled {
        registry.iter_create_enabled().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    prepare_selected(pool, project, memory, &selected).await
}

pub(super) async fn embed_with_receipt(
    pool: &PgPool,
    project: Uuid,
    memory: Uuid,
    service: &EmbeddingService,
    chunks: &[String],
) -> Result<crate::EmbeddingBatch> {
    let id = Uuid::new_v4();
    let input = serde_json::to_vec(&(memory, service.embedding_space_key(), &chunks))?;
    mem_store::effects::begin(
        pool,
        &mem_store::effects::EffectIntent {
            id,
            project_id: Some(project),
            kind: mem_store::effects::EffectKind::ProviderRequest,
            host_id: mem_store::effects::SHARED_PROVIDER_OWNER.into(),
            resource: memory.to_string(),
            input_fingerprint: format!("{:x}", Sha256::digest(input)),
            actor_id: "system:embedding-preparation".into(),
            source: "embedding-preparation".into(),
        },
    )
    .await?;
    let batch = service
        .embed_texts(chunks, EmbeddingPurpose::Document)
        .await;
    mem_store::effects::conclude(
        pool,
        id,
        Uuid::new_v5(&id, b"observed"),
        if batch.is_ok() {
            mem_store::effects::EffectStatus::Confirmed
        } else {
            mem_store::effects::EffectStatus::Uncertain
        },
        if batch.is_ok() {
            mem_store::effects::EffectReason::Observed
        } else {
            mem_store::effects::EffectReason::RemoteIo
        },
    )
    .await
    .context("embedding provider receipt failed; external request was not rolled back")?;
    batch.context("embed prepared chunks")
}

pub(super) async fn prepare_selected(
    pool: &PgPool,
    project: &str,
    memory: Uuid,
    selected: &[(&str, &EmbeddingService)],
) -> Result<PreparedMemoryChunks> {
    let row=sqlx::query("SELECT m.project_id,m.summary,m.canonical_text FROM memory_entries m JOIN projects p ON p.id=m.project_id WHERE m.id=$1 AND p.slug=$2")
        .bind(memory).bind(project).fetch_one(pool).await.context("load memory for embedding preparation")?;
    let summary: String = row.try_get("summary")?;
    let canonical_text: String = row.try_get("canonical_text")?;
    let chunks = split_search_chunks(&summary, &canonical_text);
    let mut embeddings = Vec::with_capacity(selected.len());
    for (_, service) in selected {
        let batch =
            embed_with_receipt(pool, row.try_get("project_id")?, memory, service, &chunks).await?;
        ensure!(
            batch.vectors.len() == chunks.len(),
            "embedding provider returned wrong vector count"
        );
        ensure!(
            batch.dimension > 0,
            "embedding provider returned empty vectors"
        );
        let vectors = batch
            .vectors
            .iter()
            .map(|v| v.as_slice().to_vec())
            .collect::<Vec<_>>();
        ensure!(
            vectors
                .iter()
                .all(|v| v.len() == batch.dimension as usize && v.iter().all(|f| f.is_finite())),
            "embedding provider returned invalid vectors"
        );
        embeddings.push(PreparedEmbedding {
            space: batch.space.space_key,
            provider: batch.space.provider,
            base_url: batch.space.base_url,
            model: batch.space.model,
            dimension: batch.dimension,
            vectors,
        });
    }
    Ok(PreparedMemoryChunks {
        version: 1,
        project_id: row.try_get("project_id")?,
        memory_id: memory,
        summary,
        canonical_text,
        chunks,
        embeddings,
    })
}

/// No provider calls, pool writes or commit here. Caller must rollback on error.
/// Lock and recheck source before deleting anything. Reapplying the same output
/// has stable chunk IDs and cannot create duplicate chunk/embedding rows.
pub async fn apply(
    tx: &mut Transaction<'_, Postgres>,
    output: &PreparedMemoryChunks,
) -> Result<()> {
    ensure!(
        output.version == 1,
        "unsupported prepared embedding version"
    );
    ensure!(
        output.chunks == split_search_chunks(&output.summary, &output.canonical_text),
        "prepared chunk input mismatch"
    );
    for embedding in &output.embeddings {
        ensure!(
            embedding.dimension > 0
                && embedding.vectors.len() == output.chunks.len()
                && embedding
                    .vectors
                    .iter()
                    .all(|v| v.len() == embedding.dimension as usize
                        && v.iter().all(|f| f.is_finite())),
            "invalid prepared embedding vectors"
        );
    }
    let row=sqlx::query("SELECT summary,canonical_text FROM memory_entries WHERE id=$1 AND project_id=$2 FOR UPDATE")
        .bind(output.memory_id).bind(output.project_id).fetch_one(&mut **tx).await.context("revalidate prepared embedding source")?;
    ensure!(
        row.try_get::<String, _>("summary")? == output.summary
            && row.try_get::<String, _>("canonical_text")? == output.canonical_text,
        "prepared embedding source changed"
    );
    sqlx::query("DELETE FROM memory_chunks WHERE memory_entry_id=$1")
        .bind(output.memory_id)
        .execute(&mut **tx)
        .await?;
    for (index, text) in output.chunks.iter().enumerate() {
        let id = Uuid::new_v5(
            &output.memory_id,
            format!("search-chunk-v1:{index}:{text}").as_bytes(),
        );
        sqlx::query("INSERT INTO memory_chunks(id,memory_entry_id,chunk_text,search_text,tsv) VALUES($1,$2,$3,$4,to_tsvector('english',$4))")
            .bind(id).bind(output.memory_id).bind(text).bind(format!("{}\n{text}",output.summary)).execute(&mut **tx).await?;
        for embedding in &output.embeddings {
            sqlx::query("INSERT INTO memory_chunk_embeddings(chunk_id,embedding_space,embedding,embedding_provider,embedding_base_url,embedding_model,embedding_dimension,embedding_updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,clock_timestamp())")
                .bind(id).bind(&embedding.space).bind(Vector::from(embedding.vectors[index].clone()))
                .bind(&embedding.provider).bind(&embedding.base_url).bind(&embedding.model).bind(embedding.dimension).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding_backend::{EmbeddingBackend, EmbeddingSpace};
    use std::sync::Arc;

    struct Fake {
        space: EmbeddingSpace,
        mode: u8,
        journal: Option<(PgPool, Uuid)>,
    }
    #[async_trait::async_trait]
    impl EmbeddingBackend for Fake {
        fn space(&self) -> &EmbeddingSpace {
            &self.space
        }
        async fn embed(&self, input: &[String], _: EmbeddingPurpose) -> Result<Vec<Vector>> {
            if let Some((pool, memory)) = &self.journal {
                let pending:i64 = sqlx::query_scalar("SELECT count(*) FROM external_effects WHERE resource=$1 AND source='embedding-preparation' AND status='pending'")
                    .bind(memory.to_string()).fetch_one(pool).await?;
                ensure!(
                    pending == 1,
                    "provider did not see exactly one committed intent"
                );
            }
            match self.mode {
                1 => anyhow::bail!("synthetic provider unavailable"),
                2 => Ok(vec![]),
                3 => Ok(input
                    .iter()
                    .map(|_| Vector::from(vec![f32::NAN, 0.]))
                    .collect()),
                _ => Ok(input.iter().map(|_| Vector::from(vec![0.4, 0.6])).collect()),
            }
        }
    }
    fn service(mode: u8) -> EmbeddingService {
        EmbeddingService {
            backend: Arc::new(Fake {
                space: EmbeddingSpace::new("fake", "local", "fixture"),
                mode,
                journal: None,
            }),
            batch_size: 8,
        }
    }
    async fn fixture(pool: &PgPool) -> (String, Uuid, Uuid) {
        let project = Uuid::new_v4();
        let memory = Uuid::new_v4();
        let slug = project.to_string();
        sqlx::query(
            "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'fixture','/disposable')",
        )
        .bind(project)
        .bind(&slug)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES($1,$2,$1,1,FALSE,'Known source.','Known summary','implementation','project',3,0.9,now(),now(),to_tsvector('english','Known source.'))")
            .bind(memory).bind(project).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO memory_chunks(id,memory_entry_id,chunk_text,search_text,tsv) VALUES($1,$2,'previous searchable text','previous searchable text',to_tsvector('english','previous searchable text'))")
            .bind(Uuid::new_v4()).bind(memory).execute(pool).await.unwrap();
        (slug, project, memory)
    }
    async fn chunks(pool: &PgPool, memory: Uuid) -> Vec<(Uuid, String)> {
        sqlx::query_as(
            "SELECT id,chunk_text FROM memory_chunks WHERE memory_entry_id=$1 ORDER BY id",
        )
        .bind(memory)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn direct_library_provider_calls_have_private_intent_and_observed_receipts() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (slug, _, memory) = fixture(&pool).await;
        let service = EmbeddingService {
            backend: Arc::new(Fake {
                space: EmbeddingSpace::new("fake", "local", "fixture"),
                mode: 0,
                journal: Some((pool.clone(), memory)),
            }),
            batch_size: 8,
        };
        prepare_selected(&pool, &slug, memory, &[("fake", &service)])
            .await
            .unwrap();
        let receipt:serde_json::Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM external_effects e WHERE resource=$1 AND source='embedding-preparation'").bind(memory.to_string()).fetch_one(&pool).await.unwrap();
        assert_eq!(receipt["status"], "confirmed");
        assert_eq!(
            receipt["host_id"],
            mem_store::effects::SHARED_PROVIDER_OWNER
        );
        assert!(!receipt.to_string().contains("Known source"));
        assert_eq!(receipt["input_fingerprint"].as_str().unwrap().len(), 64);
        assert_eq!(chunks(&pool, memory).await[0].1, "previous searchable text");
        sqlx::query("DELETE FROM external_effect_outcomes WHERE effect_id IN (SELECT id FROM external_effects WHERE resource=$1)").bind(memory.to_string()).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM external_effects WHERE resource=$1")
            .bind(memory.to_string())
            .execute(&pool)
            .await
            .unwrap();
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn provider_failure_or_bad_vectors_never_remove_existing_chunks() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (slug, _, memory) = fixture(&pool).await;
        let before = chunks(&pool, memory).await;
        for mode in [1, 2, 3] {
            let service = service(mode);
            assert!(
                prepare_selected(&pool, &slug, memory, &[("fake", &service)])
                    .await
                    .is_err()
            );
            assert_eq!(chunks(&pool, memory).await, before);
            // Existing public rebuild path must also preserve the old data.
            assert!(
                crate::repository::maintenance::rebuild_selected(
                    &pool,
                    &slug,
                    Some(&[memory]),
                    vec![("fake", &service)],
                    &mem_store::events::MutationContext::internal("test"),
                )
                .await
                .is_err()
            );
            assert_eq!(chunks(&pool, memory).await, before);
        }
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn prepared_output_is_replayable_atomic_and_scoped_to_source() {
        let Some(pool) = mem_test_support::migrated_pool().await else {
            return;
        };
        let (slug, _, memory) = fixture(&pool).await;
        let service = service(0);
        assert!(
            prepare_selected(&pool, "another-project", memory, &[("fake", &service)])
                .await
                .is_err()
        );
        let output = prepare_selected(&pool, &slug, memory, &[("fake", &service)])
            .await
            .unwrap();
        let output: PreparedMemoryChunks =
            serde_json::from_value(serde_json::to_value(output).unwrap()).unwrap();
        let before = chunks(&pool, memory).await;
        let mut tx = pool.begin().await.unwrap();
        apply(&mut tx, &output).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(chunks(&pool, memory).await, before);
        let mut tx = pool.begin().await.unwrap();
        apply(&mut tx, &output).await.unwrap();
        tx.commit().await.unwrap();
        let first = chunks(&pool, memory).await;
        let mut tx = pool.begin().await.unwrap();
        apply(&mut tx, &output).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(chunks(&pool, memory).await, first);
        let mut forged = output.clone();
        forged.canonical_text = "different input".into();
        forged.chunks = split_search_chunks(&forged.summary, &forged.canonical_text);
        let mut tx = pool.begin().await.unwrap();
        assert!(apply(&mut tx, &forged).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(chunks(&pool, memory).await, first);
        sqlx::query("DELETE FROM memory_entries WHERE id=$1")
            .bind(memory)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(apply(&mut tx, &output).await.is_err());
        tx.rollback().await.unwrap();
        assert!(chunks(&pool, memory).await.is_empty());
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn expired_job_fence_rolls_back_real_embedding_outputs() {
        use mem_store::jobs::*;
        let Some(database) = mem_test_support::IsolatedDatabase::new().await else {
            return;
        };
        let pool = database.pool.clone();
        let (slug, project, memory) = fixture(&pool).await;
        let before = chunks(&pool, memory).await;
        let mut tx = pool.begin().await.unwrap();
        enqueue(
            &mut tx,
            &NewJob {
                id: Uuid::new_v4(),
                project_id: project,
                operation_id: Uuid::new_v4(),
                kind: mem_record::JobKind::Embedding,
                work_key: "embedding".into(),
                actor: JobActor::System("fixture".into()),
                input: serde_json::json!({"memory":memory}),
                dependencies: vec![],
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let policy = JobPolicy::default();
        let lease = claim(&pool, policy).await.unwrap().unwrap();
        assert_eq!(lease.job.project_id, project);
        let output = prepare_selected(&pool, &slug, memory, &[("fake", &service(0))])
            .await
            .unwrap();
        let mut result = begin_result(&pool, &lease).await.unwrap();
        apply(result.transaction(), &output).await.unwrap();
        sqlx::query("UPDATE background_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(lease.job.id).execute(&mut **result.transaction()).await.unwrap();
        assert!(matches!(
            finish_result(result, Completion::Succeeded, policy).await,
            Err(mem_store::Error::StaleLease)
        ));
        assert_eq!(chunks(&pool, memory).await, before);
        let mut result = begin_result(&pool, &lease).await.unwrap();
        apply(result.transaction(), &output).await.unwrap();
        finish_result(result, Completion::Succeeded, policy)
            .await
            .unwrap();
        assert_ne!(chunks(&pool, memory).await, before);
        assert_eq!(
            detail(&pool, project, lease.job.id)
                .await
                .unwrap()
                .job
                .status,
            mem_record::JobStatus::Succeeded
        );
        mem_test_support::cleanup_project(&pool, &slug)
            .await
            .unwrap();
        database.cleanup().await.unwrap();
    }
}
