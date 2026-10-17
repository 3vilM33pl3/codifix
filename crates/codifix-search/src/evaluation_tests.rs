// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[test]
fn semantic_time_controls_recency_without_disabling_reinforcement() {
    let epoch = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .to_utc();
    let rank = |at| {
        let candidate = CandidateRecord {
            memory_id: Uuid::nil(),
            project: None,
            project_name: None,
            repo_root: None,
            summary: "runbook".into(),
            memory_type: MemoryType::Implementation,
            canonical_text: "runbook".into(),
            importance: 1,
            confidence: 0.8,
            updated_at: epoch,
            entry_fts: 1.0,
            chunk_fts: 0.0,
            semantic_similarity: 0.0,
            best_chunk_text: "runbook".into(),
            tags: vec![],
            source_paths: vec![],
            graph_boost: 0.0,
            graph_match_count: 0,
            graph_edge_count: 0,
            graph_connections: vec![],
        };
        let params = ReinforcementRankParams {
            enabled: true,
            weight: 1.0,
            ..ReinforcementRankParams::default().at_time(Some(at))
        };
        assert!(params.active());
        rank_candidate(
            candidate,
            &QueryIntent::from_query("runbook"),
            &HashMap::new(),
            None,
            &ProvenanceConfig::default(),
            false,
            Some(&ReinforcementRankSignal {
                activation: 1.0,
                needs_review: false,
            }),
            &params,
        )
        .final_score
    };
    assert_eq!(rank(epoch), rank(epoch));
    // Half the recency boost at fourteen days; seeded activation still counts.
    assert!((rank(epoch) - rank(epoch + chrono::Duration::days(14)) - 0.3).abs() < 1e-9);
}

#[tokio::test]
async fn semantic_time_controls_sql_decay_without_writing_scores() {
    let Some(source) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(source.connect_options().as_ref().clone())
        .await
        .unwrap();
    // Connection-local tables isolate this test even under parallel execution.
    sqlx::raw_sql(
        "CREATE TEMP TABLE memory_entries (id uuid, canonical_id uuid);
        CREATE TEMP TABLE memory_scores (canonical_id uuid, activation float8,
            last_decay_at timestamptz, needs_review boolean);
        INSERT INTO memory_entries VALUES ('00000000-0000-0000-0000-000000000000',
            '00000000-0000-0000-0000-000000000000');
        INSERT INTO memory_scores VALUES ('00000000-0000-0000-0000-000000000000',
            8, '2026-01-01Z', false);",
    )
    .execute(&pool)
    .await
    .unwrap();
    let epoch = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .to_utc();
    let at = epoch + chrono::Duration::days(1);
    let first = repository::fetch_reinforcement_rank_map(&pool, &[Uuid::nil()], 86400.0, Some(at))
        .await
        .unwrap();
    let again = repository::fetch_reinforcement_rank_map(&pool, &[Uuid::nil()], 86400.0, Some(at))
        .await
        .unwrap();
    assert_eq!(first[&Uuid::nil()].activation, 4.0);
    assert_eq!(
        first[&Uuid::nil()].activation,
        again[&Uuid::nil()].activation
    );
    let original: f64 = sqlx::query_scalar("SELECT activation FROM memory_scores")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(original, 8.0);
    pool.close().await;
}
