// SPDX-License-Identifier: AGPL-3.0-or-later
//! Set CODIFIX_TEST_DATABASE_URL to a disposable pgvector database and
//! CODIFIX_TEST_REQUIRE_DB=1. Every source and target is a newly generated
//! IsolatedDatabase, never an existing backup or application database. Fixtures
//! cover all import tables, including reviewed canonical relationship edges.
mod support;

use codifix_area_store::{ImportError, import_partition};
use support::*;
use uuid::Uuid;

#[tokio::test]
async fn migration_0056_import_receipt_survives_upgrade_but_cannot_authorize_replay_at_0057() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic old target");
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    execute(
        &target.pool,
        "DROP TABLE federation_agent_expertise_daily,federation_agent_expertise_intents,federation_expertise_addressed; DELETE FROM _sqlx_migrations WHERE version>=62; DROP TABLE area_reconciliation_receipts; DELETE FROM _sqlx_migrations WHERE version=61;",
    )
    .await;
    // This newly created disposable database alone simulates a pre-0057
    // installation. Never downgrade an existing installation or receipt.
    execute(&target.pool, "DROP TABLE supervisor_question_denials; DROP TABLE supervisor_questions; DELETE FROM _sqlx_migrations WHERE version=60; DROP TABLE supervisor_proposal_reviews; DELETE FROM _sqlx_migrations WHERE version=59; ALTER TABLE supervisor_settlements DROP COLUMN token_usage; DELETE FROM _sqlx_migrations WHERE version=58; DROP TABLE supervisor_health_observations; DROP TABLE supervisor_health_schedule; DELETE FROM _sqlx_migrations WHERE version=57; ALTER TABLE area_import_receipts DROP CONSTRAINT area_import_receipts_target_migration_check; ALTER TABLE area_import_receipts ADD CONSTRAINT area_import_receipts_target_migration_check CHECK(target_migration=56);").await;
    import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("supported old head");
    assert!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("same-head replay")
            .already_applied
    );
    let before = rows(&target.pool, "public.area_import_receipts").await;
    sqlx::migrate!("../../migrations")
        .run(&target.pool)
        .await
        .expect("normal additive upgrade");
    assert_eq!(
        rows(&target.pool, "public.area_import_receipts").await,
        before
    );
    assert!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .is_err()
    );
    assert_eq!(
        rows(&target.pool, "public.area_import_receipts").await,
        before
    );
    for table in OWNED {
        assert_eq!(
            rows(&target.pool, table).await,
            rows(&source.pool, table).await
        );
    }
    target.cleanup().await.expect("cleanup");
    source.cleanup().await.expect("cleanup");
}

#[tokio::test]
async fn reviewed_relations_preserve_endpoint_projects_provenance_and_replay() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic target");
    seed(&source.pool).await;
    execute(&source.pool, "INSERT INTO projects(id,slug,name,root_path) VALUES('00000000-0000-0000-0000-000000000051','second-project','Synthetic second project','/synthetic/second'); INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES('00000000-0000-0000-0000-000000000052','00000000-0000-0000-0000-000000000051','00000000-0000-0000-0000-000000000052',1,false,'Synthetic second endpoint','Synthetic second endpoint','documentation','project',3,0.5,'2026-09-01T10:00:00Z','2026-09-01T10:00:00Z',to_tsvector('simple','Synthetic second endpoint'));").await;
    sqlx::query("INSERT INTO memory_relations(id,src_canonical_id,dst_canonical_id,relation_type,origin,confidence,actor,created_at) VALUES($1,$2,'00000000-0000-0000-0000-000000000052','supports','asserted',0.875,'synthetic-human','2026-09-05T10:00:00.123456Z')")
        .bind(id(53)).bind(id(13)).execute(&source.pool).await.expect("synthetic relation");
    let mut fixture = capture(&source.pool).await;
    fixture.manifest.format_version = codifix_area::PROVENANCE_MANIFEST_VERSION;
    let relation = fixture
        .inventory
        .tables
        .iter()
        .find(|t| t.table == "public.memory_relations")
        .expect("synthetic relation table");
    assert_eq!(relation.rows[0].source_project_id, None);
    let (partition, _) = fixture.partition();
    import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("relation import");
    for table in OWNED {
        assert_eq!(
            rows(&target.pool, table).await,
            rows(&source.pool, table).await,
            "{table}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory_relations r JOIN memory_canonical_state s ON s.canonical_id=r.src_canonical_id JOIN memory_canonical_state d ON d.canonical_id=r.dst_canonical_id WHERE s.project_id<>d.project_id AND r.actor='synthetic-human'")
        .fetch_one(&target.pool).await.expect("preserved project boundary");
    assert_eq!(count, 1);
    assert!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("replay")
            .already_applied
    );
    execute(
        &target.pool,
        "UPDATE memory_relations SET actor='changed-attribution'",
    )
    .await;
    assert_eq!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect_err("changed edge"),
        ImportError::Changed
    );
    target.cleanup().await.expect("cleanup target");
    source.cleanup().await.expect("cleanup source");
}

#[tokio::test]
async fn replay_refuses_disabled_kill_switch_and_new_authority_auth_or_job_state() {
    let Some(source) = database().await else {
        return;
    };
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    for mutation in [
        "UPDATE loop_global_state SET kill_switch_enabled=false",
        "INSERT INTO supervisor_instance_binding(instance_id,area) VALUES('00000000-0000-0000-0000-000000005555','general')",
        "INSERT INTO auth_principals(id,kind,display_name) VALUES('00000000-0000-0000-0000-000000005556','internal','Synthetic activated identity')",
        "INSERT INTO background_jobs(id,project_id,operation_id,kind,work_key,actor,input) VALUES('00000000-0000-0000-0000-000000005557','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-000000005558','embedding','synthetic-active-job','{}','{}')",
    ] {
        let target = database().await.expect("synthetic fixture operation");
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("synthetic fixture operation");
        let receipt = rows(&target.pool, "public.area_import_receipts").await;
        execute(&target.pool, mutation).await;
        assert_eq!(
            import_partition(&target.pool, TARGET, OPERATION, &partition)
                .await
                .expect_err("import must reject the invalid target"),
            ImportError::NotFresh
        );
        assert_eq!(
            rows(&target.pool, "public.area_import_receipts").await,
            receipt
        );
        for table in OWNED {
            assert_eq!(
                rows(&target.pool, table).await,
                rows(&source.pool, table).await
            );
        }
        target.cleanup().await.expect("synthetic fixture operation");
    }
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn deferred_commit_failure_rolls_back_rows_receipts_sequence_and_kill_switch() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    execute(&target.pool, "CREATE FUNCTION synthetic_reject_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SYNTHETIC_PRIVATE_ORIGINAL_NOT_FOR_TARGET'; END $$; CREATE CONSTRAINT TRIGGER synthetic_reject_commit AFTER INSERT ON area_import_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_reject_commit();").await;
    let error = import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect_err("import must reject the invalid target");
    assert_eq!(error, ImportError::Database);
    assert!(!format!("{error:?} {error}").contains(PRIVATE));
    assert_empty(&target.pool).await;
    execute(
        &target.pool,
        "DROP TRIGGER synthetic_reject_commit ON area_import_receipts",
    )
    .await;
    assert!(
        !import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("synthetic fixture operation")
            .already_applied
    );
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn exact_history_tombstone_canonical_state_and_provenance_survive_import_and_replay() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let fixture = capture(&source.pool).await;
    let (partition, archive) = fixture.partition();
    assert!(
        String::from_utf8(archive)
            .expect("synthetic fixture operation")
            .contains(PRIVATE)
    );
    let receipt = import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("fresh import");
    assert!(!receipt.already_applied);
    assert_eq!(receipt.row_count as usize, partition.rows().len());
    assert_eq!(receipt.projection_count, 0);
    for table in OWNED {
        assert_eq!(
            rows(&source.pool, table).await,
            rows(&target.pool, table).await,
            "exact source bytes: {table}"
        );
    }
    let versions: Vec<(i32, bool)> =
        sqlx::query_as("SELECT version_no,is_tombstone FROM memory_entries ORDER BY version_no")
            .fetch_all(&target.pool)
            .await
            .expect("synthetic fixture operation");
    assert_eq!(versions, [(1, false), (2, false), (3, true)]);
    let state: (String, i32, f32) =
        sqlx::query_as("SELECT status,importance,confidence FROM memory_canonical_state")
            .fetch_one(&target.pool)
            .await
            .expect("synthetic fixture operation");
    assert_eq!(state, ("archived".into(), 9, 0.875));
    let sequence: (i64, bool) =
        sqlx::query_as("SELECT last_value,is_called FROM memory_source_checks_id_seq")
            .fetch_one(&target.pool)
            .await
            .expect("synthetic fixture operation");
    assert_eq!(sequence, (43, false));
    let evidence = rows(&target.pool, "public.area_import_rows").await;
    let immutable = rows(&target.pool, "public.area_import_receipts").await;
    let replay = import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("verified replay");
    assert!(replay.already_applied);
    assert_eq!(
        rows(&target.pool, "public.area_import_receipts").await,
        immutable
    );
    assert_eq!(
        rows(&target.pool, "public.area_import_rows").await,
        evidence
    );
    for table in OWNED {
        assert_eq!(
            rows(&source.pool, table).await,
            rows(&target.pool, table).await
        );
    }
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn wrong_identity_and_changed_replay_never_repeat_inserts() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    for (target_id, operation) in [
        (Uuid::nil(), OPERATION),
        (TARGET, Uuid::nil()),
        (partition.source_instance(), OPERATION),
    ] {
        assert_eq!(
            import_partition(&target.pool, target_id, operation, &partition)
                .await
                .expect_err("import must reject the invalid target"),
            ImportError::Invalid
        );
        assert_empty(&target.pool).await;
    }
    import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("synthetic fixture operation");
    let receipts = rows(&target.pool, "public.area_import_receipts").await;
    for (target_id, operation) in [(id(20001), OPERATION), (TARGET, id(30001))] {
        assert_eq!(
            import_partition(&target.pool, target_id, operation, &partition)
                .await
                .expect_err("import must reject the invalid target"),
            ImportError::Conflict
        );
    }
    execute(
        &target.pool,
        "UPDATE memory_canonical_state SET importance=1",
    )
    .await;
    assert_eq!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect_err("import must reject the invalid target"),
        ImportError::Changed
    );
    assert_eq!(
        rows(&target.pool, "public.area_import_receipts").await,
        receipts
    );
    let importance: i32 = sqlx::query_scalar("SELECT importance FROM memory_canonical_state")
        .fetch_one(&target.pool)
        .await
        .expect("synthetic fixture operation");
    assert_eq!(
        importance, 1,
        "replay must not restore or overwrite changed state"
    );
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn populated_target_is_denied_without_changing_existing_data() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    seed(&target.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    let before = capture(&target.pool).await.payload;
    assert_eq!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect_err("import must reject the invalid target"),
        ImportError::NotFresh
    );
    assert_eq!(capture(&target.pool).await.payload, before);
    assert!(
        rows(&target.pool, "public.area_import_receipts")
            .await
            .is_empty()
    );
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn target_schema_head_unknown_table_and_column_mismatch_fail_closed() {
    let Some(source) = database().await else {
        return;
    };
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    for mutation in [
        "DELETE FROM _sqlx_migrations WHERE version=56",
        "CREATE TABLE synthetic_unknown(id uuid PRIMARY KEY)",
        "ALTER TABLE memory_entries ADD COLUMN synthetic_extra text",
    ] {
        let target = database().await.expect("synthetic fixture operation");
        execute(&target.pool, mutation).await;
        assert_eq!(
            import_partition(&target.pool, TARGET, OPERATION, &partition)
                .await
                .expect_err("import must reject the invalid target"),
            ImportError::Schema
        );
        assert_empty(&target.pool).await;
        target.cleanup().await.expect("synthetic fixture operation");
    }
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn late_insertion_failure_rolls_back_rows_state_receipts_and_sequence() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    execute(&target.pool, "ALTER TABLE memory_sources ADD CONSTRAINT synthetic_rejection CHECK (source_kind <> 'file')").await;
    let error = import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect_err("import must reject the invalid target");
    assert_eq!(error, ImportError::Database);
    assert!(!format!("{error:?} {error}").contains(PRIVATE));
    assert_empty(&target.pool).await;
    // A corrected fresh target may retry; no partial receipt made it a replay.
    execute(
        &target.pool,
        "ALTER TABLE memory_sources DROP CONSTRAINT synthetic_rejection",
    )
    .await;
    assert!(
        !import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("synthetic fixture operation")
            .already_applied
    );
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn verification_failure_after_insertion_rolls_back_the_entire_transaction() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let (partition, _) = capture(&source.pool).await.partition();
    execute(&target.pool, "CREATE FUNCTION synthetic_alter_import() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.excerpt='synthetic changed by trigger'; RETURN NEW; END $$; CREATE TRIGGER synthetic_alter_import BEFORE INSERT ON memory_sources FOR EACH ROW EXECUTE FUNCTION synthetic_alter_import();").await;
    assert_eq!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect_err("import must reject the invalid target"),
        ImportError::Changed
    );
    assert_empty(&target.pool).await;
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}

#[tokio::test]
async fn reviewed_projection_archives_private_context_and_binds_immutable_transformation_evidence()
{
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic fixture operation");
    seed(&source.pool).await;
    let mut fixture = capture(&source.pool).await;
    fixture.project_provenance();
    let (partition, archive) = fixture.partition();
    assert!(
        String::from_utf8(archive)
            .expect("synthetic fixture operation")
            .contains(PRIVATE)
    );
    assert_eq!(partition.projected_sources().len(), 1);
    let result = import_partition(&target.pool, TARGET, OPERATION, &partition)
        .await
        .expect("synthetic fixture operation");
    assert_eq!(result.projection_count, 1);
    for table in [
        "public.sessions",
        "public.tasks",
        "public.raw_captures",
        "public.memory_source_checks",
    ] {
        assert!(
            rows(&target.pool, table).await.is_empty(),
            "private context must stay archived: {table}"
        );
    }
    let sources = rows(&target.pool, "public.memory_sources").await;
    assert_eq!(sources.len(), 1);
    assert!(!sources[0].contains(PRIVATE));
    let row: serde_json::Value =
        serde_json::from_str(&sources[0]).expect("synthetic fixture operation");
    for column in [
        "task_id",
        "file_path",
        "git_commit",
        "excerpt",
        "symbol_name",
        "symbol_kind",
        "line_start",
        "line_end",
        "content_hash",
        "target_memory_id",
    ] {
        assert!(row[column].is_null(), "projection leaked original {column}");
    }
    let provenance = rows(&target.pool, "public.area_import_provenance").await;
    assert_eq!(provenance.len(), 1);
    let evidence: serde_json::Value =
        serde_json::from_str(&provenance[0]).expect("synthetic fixture operation");
    let projection = &partition.projected_sources()[0];
    assert_eq!(
        evidence["source_sha256"],
        projection.binding().source.row_sha256
    );
    assert_eq!(
        evidence["original_version_sha256"],
        projection.binding().memory_version.row_sha256
    );
    assert_eq!(
        evidence["archived_task_sha256"],
        projection.binding().archived_task.row_sha256
    );
    assert_eq!(evidence["target_sha256"], hash(&sources[0]));
    assert_eq!(evidence["decision_sha256"], projection.decision_sha256());
    assert!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect("synthetic fixture operation")
            .already_applied
    );
    assert!(
        sqlx::query("UPDATE area_import_provenance SET decision_sha256=repeat('0',64)")
            .execute(&target.pool)
            .await
            .is_err(),
        "receipt is immutable"
    );
    // Privileged synthetic corruption, only in this generated test database.
    execute(&target.pool, "ALTER TABLE area_import_provenance DISABLE TRIGGER area_import_provenance_guard; UPDATE area_import_provenance SET decision_sha256=repeat('0',64); ALTER TABLE area_import_provenance ENABLE TRIGGER area_import_provenance_guard;").await;
    assert_eq!(
        import_partition(&target.pool, TARGET, OPERATION, &partition)
            .await
            .expect_err("import must reject the invalid target"),
        ImportError::Changed
    );
    target.cleanup().await.expect("synthetic fixture operation");
    source.cleanup().await.expect("synthetic fixture operation");
}
