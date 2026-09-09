// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use mem_curate::{curate, preview_curate, refresh_semantic_relations, store_capture};
use mem_record::{
    CaptureCandidateInput, CaptureCandidateSourceInput, CaptureTaskRequest, CurateRequest,
    MemoryType, ReplacementPolicy, SourceKind,
};
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn curation_events_and_version_pinned_job_intents_commit_or_rollback_together() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("curation-job-intents");
    let capture = store_capture(
        &pool,
        &capture_request(
            &project,
            "Durable intent",
            "Background work must retain the immutable memory version identity.",
            "Durable source",
        ),
    )
    .await
    .unwrap();
    let request = CurateRequest {
        project: project.clone(),
        batch_size: Some(10),
        raw_capture_id: Some(capture.raw_capture_id),
        replacement_policy: None,
        dry_run: false,
    };
    let mut context = mem_store::events::MutationContext::internal("fixture");
    // Late event failure, after memory, curation run and all stage intents.
    context.source = "\0".into();
    assert!(
        mem_curate::curate_as(&pool, &request, &context)
            .await
            .is_err()
    );
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM curation_runs c JOIN projects p ON p.id=c.project_id WHERE p.slug=$1),(SELECT count(*) FROM background_jobs j JOIN projects p ON p.id=j.project_id WHERE p.slug=$1),(SELECT count(*) FROM background_work_versions v JOIN projects p ON p.id=v.project_id WHERE p.slug=$1)")
        .bind(&project).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    let still_pending: bool =
        sqlx::query_scalar("SELECT curated_at IS NULL FROM raw_captures WHERE id=$1")
            .bind(capture.raw_capture_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(still_pending);
    let response = curate(&pool, &request).await.unwrap();
    let work = response.background_work.unwrap();
    assert_eq!(work.operation_id, response.run_id);
    assert_eq!(work.job_ids.len(), 4);
    let inputs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT memory_id FROM background_work_versions WHERE operation_id=$1 ORDER BY memory_id",
    )
    .bind(work.operation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected = response.memory_ids;
    expected.sort();
    assert_eq!(inputs, expected);
    let validation_deps: i64 =
        sqlx::query_scalar("SELECT count(*) FROM background_job_dependencies WHERE job_id=$1")
            .bind(work.job_ids[2])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(validation_deps, 0);
    let preview = preview_curate(
        &pool,
        &CurateRequest {
            dry_run: true,
            ..request
        },
    )
    .await
    .unwrap();
    assert!(preview.background_work.is_none());
    mem_test_support::cleanup_project(&pool, &project)
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_curate_and_replacement_proposal_flow_persists_rows() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("curate-db");
    mem_test_support::cleanup_project(&pool, &project)
        .await
        .expect("cleanup old test project");

    let first = capture_request(
        &project,
        "Initial memory",
        "The query pipeline stores source provenance in memory_sources for graph-aware retrieval.",
        "Query provenance pipeline",
    );
    let capture = store_capture(&pool, &first)
        .await
        .expect("store first capture");
    let response = curate(
        &pool,
        &CurateRequest {
            project: project.clone(),
            batch_size: Some(10),
            raw_capture_id: Some(capture.raw_capture_id),
            replacement_policy: Some(ReplacementPolicy::Balanced),
            dry_run: false,
        },
    )
    .await
    .expect("curate first capture");
    assert_eq!(response.output_count, 1);
    assert_eq!(response.proposal_count, 0);

    let second = capture_request(
        &project,
        "Candidate update",
        "The query pipeline stores source provenance in memory_sources for graph-aware retrieval and query diagnostics.",
        "Diagnostics candidate",
    );
    let second_capture = store_capture(&pool, &second)
        .await
        .expect("store second capture");
    let preview = preview_curate(
        &pool,
        &CurateRequest {
            project: project.clone(),
            batch_size: Some(10),
            raw_capture_id: Some(second_capture.raw_capture_id),
            replacement_policy: Some(ReplacementPolicy::Balanced),
            dry_run: true,
        },
    )
    .await
    .expect("preview second capture");
    assert_eq!(preview.proposal_count, 1);

    let response = curate(
        &pool,
        &CurateRequest {
            project: project.clone(),
            batch_size: Some(10),
            raw_capture_id: Some(second_capture.raw_capture_id),
            replacement_policy: Some(ReplacementPolicy::Balanced),
            dry_run: false,
        },
    )
    .await
    .expect("curate second capture");
    assert_eq!(response.output_count, 0);
    assert_eq!(response.proposal_count, 1);

    let memory_count: i64 = sqlx::query(
        r#"
        SELECT COUNT(*)::bigint AS count
        FROM memory_entries me
        JOIN projects p ON p.id = me.project_id
        WHERE p.slug = $1
        "#,
    )
    .bind(&project)
    .fetch_one(&pool)
    .await
    .expect("count memory rows")
    .try_get("count")
    .expect("decode memory count");
    assert_eq!(memory_count, 1);

    let proposal_count: i64 = sqlx::query(
        r#"
        SELECT COUNT(*)::bigint AS count
        FROM memory_replacement_proposals mrp
        JOIN projects p ON p.id = mrp.project_id
        WHERE p.slug = $1 AND mrp.status = 'pending'
        "#,
    )
    .bind(&project)
    .fetch_one(&pool)
    .await
    .expect("count proposal rows")
    .try_get("count")
    .expect("decode proposal count");
    assert_eq!(proposal_count, 1);

    let proposal: Uuid = sqlx::query_scalar("SELECT mrp.id FROM memory_replacement_proposals mrp JOIN projects p ON p.id=mrp.project_id WHERE p.slug=$1 AND mrp.status='pending'")
        .bind(&project).fetch_one(&pool).await.unwrap();
    let broken = mem_store::events::MutationContext {
        source: "\0".into(),
        ..mem_store::events::MutationContext::internal("test")
    };
    let actor = mem_store::jobs::JobActor::System("local-curation".into());
    let mut tx = pool.begin().await.unwrap();
    assert!(
        mem_curate::approve_replacement_proposal_tx(&mut tx, &project, proposal, &broken, &actor)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        mem_curate::reject_replacement_proposal_tx(&mut tx, &project, proposal, &broken, &actor)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let unchanged: (String, i64) = sqlx::query_as("SELECT status,(SELECT count(*) FROM background_jobs WHERE operation_id=$1) FROM memory_replacement_proposals WHERE id=$1")
        .bind(proposal).fetch_one(&pool).await.unwrap();
    assert_eq!(unchanged, ("pending".into(), 0));
    let approved = mem_curate::approve_replacement_proposal(&pool, &project, proposal)
        .await
        .unwrap();
    assert!(approved.new_memory_id.is_some());
    let jobs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM background_jobs WHERE operation_id=$1")
            .bind(proposal)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(jobs, 4);
    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project_timeline_events WHERE operation_id=$1")
            .bind(proposal.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(events, 1);
    let mut tx = pool.begin().await.unwrap();
    assert!(
        mem_curate::apply_validation_revision_tx(
            &mut tx,
            approved.target_memory_id,
            "revised",
            "revised body",
            &broken,
            &actor
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    let next_version = mem_curate::apply_validation_revision(
        &pool,
        approved.target_memory_id,
        "revised",
        "revised body",
    )
    .await
    .unwrap();
    let version_no: i32 = sqlx::query_scalar("SELECT version_no FROM memory_entries WHERE id=$1")
        .bind(next_version)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        version_no, 3,
        "a revision of an old version must allocate above the full stream"
    );
    let mut tx = pool.begin().await.unwrap();
    let project_id: Uuid = sqlx::query_scalar("SELECT id FROM projects WHERE slug=$1")
        .bind(&project)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(matches!(
        mem_store::post_curation::enqueue(&mut tx, project_id, proposal, &[next_version], &actor)
            .await,
        Err(mem_store::Error::Conflict)
    ));
    tx.rollback().await.unwrap();
    mem_curate::refresh_memory_relations(&pool, &project, next_version)
        .await
        .unwrap();

    mem_test_support::cleanup_project(&pool, &project)
        .await
        .expect("cleanup test project");
}

#[tokio::test]
async fn semantic_dedup_links_paraphrases_and_flags_conflicts() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("semantic-db");
    mem_test_support::cleanup_project(&pool, &project)
        .await
        .expect("cleanup old test project");

    // Paraphrases with almost no shared >=3-char tokens and disjoint
    // tags/files, so the lexical relation classifier cannot link them.
    let duplicate_id = seed_memory(
        &pool,
        &project,
        "Deploy artifacts get shipped after checks finish running.",
        "Shipping happens post-checks",
        "deploy",
        "docs/deploy.md",
        "[1, 0, 0]",
    )
    .await;
    let paraphrase_id = seed_memory(
        &pool,
        &project,
        "Release bundles are published once verification completes.",
        "Publication follows verification",
        "release",
        "docs/release.md",
        "[0.999, 0.02, 0]",
    )
    .await;

    let duplicates = refresh_semantic_relations(&pool, &project, &[duplicate_id], 0.9)
        .await
        .expect("semantic pass over paraphrased pair");
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].other_memory_id, paraphrase_id);
    assert!(duplicates[0].similarity >= 0.9);
    assert!(!duplicates[0].conflict);
    assert_eq!(
        relation_count(&pool, duplicate_id, paraphrase_id, "duplicates").await,
        1
    );
    assert_eq!(
        relation_count(&pool, paraphrase_id, duplicate_id, "duplicates").await,
        1
    );

    // Same embedding neighborhood, but supersede/negation language: must be
    // flagged as a conflict and linked related_to, never duplicates.
    let conflict_id = seed_memory(
        &pool,
        &project,
        "The pipeline no longer publishes bundles; that flow was retired.",
        "Bundle publishing retired",
        "pipeline",
        "docs/pipeline.md",
        "[0.998, 0.03, 0]",
    )
    .await;

    let conflicts = refresh_semantic_relations(&pool, &project, &[conflict_id], 0.9)
        .await
        .expect("semantic pass over conflicting memory");
    assert_eq!(conflicts.len(), 2);
    assert!(conflicts.iter().all(|pair| pair.conflict));
    assert_eq!(
        relation_count(&pool, conflict_id, duplicate_id, "related_to").await,
        1
    );
    assert_eq!(
        relation_count(&pool, conflict_id, duplicate_id, "duplicates").await,
        0
    );

    // Below-threshold pairs are ignored entirely.
    let unrelated = refresh_semantic_relations(&pool, &project, &[duplicate_id], 0.99999)
        .await
        .expect("semantic pass with strict threshold");
    assert!(unrelated.is_empty());

    mem_test_support::cleanup_project(&pool, &project)
        .await
        .expect("cleanup test project");
}

async fn seed_memory(
    pool: &sqlx::PgPool,
    project: &str,
    canonical_text: &str,
    summary: &str,
    tag: &str,
    file_path: &str,
    embedding_literal: &str,
) -> Uuid {
    let mut request = capture_request(project, summary, canonical_text, summary);
    request.files_changed = vec![file_path.to_string()];
    request.structured_candidates[0].tags = vec![tag.to_string()];
    request.structured_candidates[0].sources[0].file_path = Some(file_path.to_string());
    let capture = store_capture(pool, &request).await.expect("store capture");
    curate(
        pool,
        &CurateRequest {
            project: project.to_string(),
            batch_size: Some(10),
            raw_capture_id: Some(capture.raw_capture_id),
            replacement_policy: Some(ReplacementPolicy::Balanced),
            dry_run: false,
        },
    )
    .await
    .expect("curate capture");

    let memory_id: Uuid = sqlx::query(
        r#"
        SELECT me.id
        FROM memory_entries me
        JOIN projects p ON p.id = me.project_id
        WHERE p.slug = $1 AND me.summary = $2
        "#,
    )
    .bind(project)
    .bind(summary)
    .fetch_one(pool)
    .await
    .expect("fetch seeded memory id")
    .try_get("id")
    .expect("decode memory id");

    // Chunk embeddings are normally built by service embedding maintenance;
    // insert deterministic vectors directly so the test needs no embedder.
    let chunk_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO memory_chunks (id, memory_entry_id, chunk_text, search_text, tsv)
        VALUES ($1, $2, $3, $3, to_tsvector('english', $3))
        "#,
    )
    .bind(chunk_id)
    .bind(memory_id)
    .bind(canonical_text)
    .execute(pool)
    .await
    .expect("insert chunk");
    sqlx::query(
        r#"
        INSERT INTO memory_chunk_embeddings (
            chunk_id, embedding_space, embedding, embedding_dimension, embedding_updated_at
        )
        VALUES ($1, 'test-space', $2::vector, 3, now())
        "#,
    )
    .bind(chunk_id)
    .bind(embedding_literal)
    .execute(pool)
    .await
    .expect("insert chunk embedding");
    memory_id
}

async fn relation_count(pool: &sqlx::PgPool, src: Uuid, dst: Uuid, relation_type: &str) -> i64 {
    sqlx::query(
        r#"
        SELECT COUNT(*)::bigint AS count
        FROM memory_relations
        WHERE src_canonical_id = (SELECT canonical_id FROM memory_entries WHERE id = $1)
          AND dst_canonical_id = (SELECT canonical_id FROM memory_entries WHERE id = $2)
          AND relation_type = $3
        "#,
    )
    .bind(src)
    .bind(dst)
    .bind(relation_type)
    .fetch_one(pool)
    .await
    .expect("count relations")
    .try_get("count")
    .expect("decode relation count")
}

#[tokio::test]
async fn curation_rollback_and_competing_curators_preserve_atomic_events() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("r1-curation");
    let capture = store_capture(
        &pool,
        &capture_request(&project, "curate me", "persist once", "one fact"),
    )
    .await
    .unwrap();
    let request = CurateRequest {
        project: project.clone(),
        batch_size: Some(1),
        raw_capture_id: Some(capture.raw_capture_id),
        replacement_policy: None,
        dry_run: false,
    };
    let context = mem_store::events::MutationContext::internal("curation-test");
    let mut tx = pool.begin().await.unwrap();
    let tentative = mem_curate::curate_tx(&mut tx, &request, &context)
        .await
        .unwrap();
    assert_eq!(tentative.response.input_count, 1);
    assert_eq!(tentative.events.len(), 1);
    tx.rollback().await.unwrap();
    let uncurated: bool =
        sqlx::query_scalar("SELECT curated_at IS NULL FROM raw_captures WHERE id=$1")
            .bind(capture.raw_capture_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(uncurated);
    assert_eq!(
        mem_store::events::page(&pool, capture.project_id, 0, 10)
            .await
            .unwrap()
            .events
            .len(),
        1,
        "only the committed capture event survives"
    );
    let (first, second) = tokio::join!(
        mem_curate::curate_as(&pool, &request, &context),
        mem_curate::curate_as(&pool, &request, &context)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.response.input_count + second.response.input_count, 1);
    assert_eq!(
        first.response.output_count + second.response.output_count,
        1
    );
    for result in [first, second] {
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].0.id, result.response.run_id);
    }
    assert_eq!(
        mem_store::events::page(&pool, capture.project_id, 0, 10)
            .await
            .unwrap()
            .events
            .len(),
        3
    );
    mem_test_support::cleanup_project(&pool, &project)
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_transaction_couples_event_and_replay_identity() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("r1-capture");
    let request = capture_request(&project, "atomic capture", "synthetic fact", "fact");
    let mut preview_request = request.clone();
    preview_request.dry_run = true;
    assert!(
        store_capture(&pool, &preview_request)
            .await
            .unwrap()
            .dry_run
    );
    let preview_absent: bool =
        sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM projects WHERE slug=$1)")
            .bind(&project)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        preview_absent,
        "direct library preview must not insert sessions, captures or events"
    );
    let actor = mem_store::events::MutationContext {
        actor_id: Uuid::new_v4().to_string(),
        actor_name: "reviewer".into(),
        source: "test".into(),
        occurred_at: None,
    };
    let mut tx = pool.begin().await.unwrap();
    let tentative = mem_curate::store_capture_tx(&mut tx, &request, &actor)
        .await
        .unwrap();
    assert!(tentative.event.is_some());
    tx.rollback().await.unwrap();
    let absent: bool =
        sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM projects WHERE slug=$1)")
            .bind(&project)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        absent,
        "project, session, task, capture and event all roll back"
    );
    let (first, second) = tokio::join!(
        mem_curate::store_capture_as(&pool, &request, &actor),
        mem_curate::store_capture_as(&pool, &request, &actor)
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(
        first.response.raw_capture_id,
        second.response.raw_capture_id
    );
    assert_eq!(first.response.session_id, second.response.session_id);
    assert_eq!(first.response.task_id, second.response.task_id);
    assert_eq!(
        usize::from(first.event.is_some()) + usize::from(second.event.is_some()),
        1
    );
    let events = mem_store::events::page(&pool, first.response.project_id, 0, 10)
        .await
        .unwrap();
    assert_eq!(events.events.len(), 1);
    assert_eq!(events.events[0]["actor_id"], actor.actor_id);
    let tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks t JOIN sessions s ON s.id=t.session_id WHERE s.project_id=$1",
    )
    .bind(first.response.project_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tasks, 1);
    mem_test_support::cleanup_project(&pool, &project)
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_event_failure_cannot_leave_a_capture_committed() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = mem_test_support::unique_project_slug("r1-capture-fault");
    // Deliberately exceed the event bound after domain writes have been staged.
    let request = capture_request(&project, &"x".repeat(65537), "synthetic fact", "fact");
    assert!(store_capture(&pool, &request).await.is_err());
    let absent: bool =
        sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM projects WHERE slug=$1)")
            .bind(&project)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(absent);
}

fn capture_request(
    project: &str,
    title: &str,
    canonical_text: &str,
    summary: &str,
) -> CaptureTaskRequest {
    CaptureTaskRequest {
        project: project.to_string(),
        task_title: title.to_string(),
        user_prompt: "Record durable query provenance behavior.".to_string(),
        writer_id: "db-test".to_string(),
        writer_name: Some("DB Test".to_string()),
        agent_summary: "Captured a structured candidate for DB integration tests.".to_string(),
        files_changed: vec!["crates/mem-search/src/lib.rs".to_string()],
        git_diff_summary: None,
        git_commit: None,
        tests: Vec::new(),
        notes: Vec::new(),
        structured_candidates: vec![CaptureCandidateInput {
            canonical_text: canonical_text.to_string(),
            summary: summary.to_string(),
            memory_type: MemoryType::Implementation,
            confidence: 0.9,
            importance: 3,
            tags: vec!["query".to_string()],
            sources: vec![CaptureCandidateSourceInput {
                file_path: Some("crates/mem-search/src/lib.rs".to_string()),
                git_commit: None,
                line_start: None,
                line_end: None,
                content_hash: None,
                target_memory_id: None,
                symbol_name: None,
                symbol_kind: None,
                source_kind: SourceKind::File,
                excerpt: Some("query provenance".to_string()),
            }],
        }],
        command_output: None,
        idempotency_key: None,
        dry_run: false,
    }
}
