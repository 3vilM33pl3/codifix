// SPDX-License-Identifier: AGPL-3.0-or-later
#[allow(dead_code)]
mod support;
use mem_area_store::{
    ImportError, import_partition,
    reconciliation::{preview, reconcile},
};
use support::*;

#[tokio::test]
async fn existing_target_merge_is_bound_atomic_and_replayable() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("target");
    seed(&source.pool).await;
    let (initial, _) = capture(&source.pool).await.partition();
    import_partition(&target.pool, TARGET, OPERATION, &initial)
        .await
        .expect("initial import");
    execute(&target.pool, "INSERT INTO auth_principals(id,kind,display_name) VALUES('00000000-0000-0000-0000-000000005556','internal','Retained live identity'); INSERT INTO projects(id,slug,name,root_path) VALUES('00000000-0000-0000-0000-000000005557','target-only','Retain target-only','/synthetic/target');").await;
    let runtime: Vec<String> =
        sqlx::query_scalar("SELECT to_jsonb(t)::text FROM auth_principals t ORDER BY id")
            .fetch_all(&target.pool)
            .await
            .expect("synthetic reconciliation fixture");
    // Tombstone append leaves the deliberately customized canonical state intact.
    execute(&source.pool, "INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) VALUES('00000000-0000-0000-0000-00000000000e','00000000-0000-0000-0000-000000000001','00000000-0000-0000-0000-00000000000d',4,true,'','','documentation','project',3,0.5,'2026-09-29','2026-09-29',to_tsvector('simple',''))").await;
    let (partition, _) = capture(&source.pool).await.partition();
    let plan = preview(&target.pool, TARGET, &partition)
        .await
        .expect("preview");
    assert_eq!(plan.conflicts(), 0);
    assert_eq!(
        reconcile(&target.pool, id(500), &partition, &plan, false)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Invalid
    );
    // A late receipt failure rolls back every inserted row and trigger effect.
    execute(&target.pool,"CREATE FUNCTION fail_reconciliation_test() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic injected failure'; END $$; CREATE TRIGGER inject_failure BEFORE INSERT ON area_reconciliation_receipts FOR EACH ROW EXECUTE FUNCTION fail_reconciliation_test();").await;
    let before = rows(&target.pool, "public.memory_entries").await;
    assert_eq!(
        reconcile(&target.pool, id(500), &partition, &plan, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Database
    );
    assert_eq!(rows(&target.pool, "public.memory_entries").await, before);
    execute(
        &target.pool,
        "DROP TRIGGER inject_failure ON area_reconciliation_receipts",
    )
    .await;
    let receipt = reconcile(&target.pool, id(500), &partition, &plan, true)
        .await
        .expect("merge");
    assert_eq!(receipt.inserted, 1);
    assert!(!receipt.already_applied);
    assert!(
        reconcile(&target.pool, id(500), &partition, &plan, true)
            .await
            .expect("replay")
            .already_applied
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT to_jsonb(t)::text FROM auth_principals t ORDER BY id"
        )
        .fetch_all(&target.pool)
        .await
        .expect("synthetic reconciliation fixture"),
        runtime
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE slug='target-only'")
        .fetch_one(&target.pool)
        .await
        .expect("synthetic reconciliation fixture");
    assert_eq!(count, 1);
    execute(
        &target.pool,
        "UPDATE projects SET name='Target changed after replay' WHERE slug='target-only'",
    )
    .await;
    assert_eq!(
        reconcile(&target.pool, id(500), &partition, &plan, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Changed
    );
    assert_eq!(
        reconcile(&target.pool, id(501), &partition, &plan, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Changed
    );
    // Source edits under an existing version identity are explicit conflicts.
    execute(
        &source.pool,
        "UPDATE memory_entries SET summary='conflicting history' WHERE version_no=1",
    )
    .await;
    let (changed, _) = capture(&source.pool).await.partition();
    let conflict = preview(&target.pool, TARGET, &changed)
        .await
        .expect("conflict preview");
    assert_eq!(conflict.conflicts(), 1);
    assert_eq!(
        reconcile(&target.pool, id(502), &changed, &conflict, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Invalid
    );
    assert_eq!(
        reconcile(&target.pool, id(500), &changed, &plan, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Conflict
    );
    target
        .cleanup()
        .await
        .expect("synthetic reconciliation fixture");
    source
        .cleanup()
        .await
        .expect("synthetic reconciliation fixture");
}

#[tokio::test]
async fn explicit_destination_resolution_preserves_state_across_version_triggers() {
    let Some(source) = database().await else {
        return;
    };
    let target = database().await.expect("synthetic reconciliation fixture");
    seed(&source.pool).await;
    let (initial, _) = capture(&source.pool).await.partition();
    import_partition(&target.pool, TARGET, OPERATION, &initial)
        .await
        .expect("synthetic reconciliation fixture");
    execute(
        &target.pool,
        "UPDATE projects SET root_path='/new/area/path'",
    )
    .await;
    let canonical_before = rows(&target.pool, "public.memory_canonical_state").await;
    execute(&source.pool,"INSERT INTO memory_entries(id,project_id,canonical_id,version_no,is_tombstone,canonical_text,summary,memory_type,scope,importance,confidence,created_at,updated_at,search_document) SELECT '00000000-0000-0000-0000-00000000000e',project_id,canonical_id,4,false,'new content','new content',memory_type,scope,importance,confidence,'2026-09-29','2026-09-29',to_tsvector('simple','new content') FROM memory_entries WHERE version_no=3").await;
    let (partition, _) = capture(&source.pool).await.partition();
    let mut reviewed = preview(&target.pool, TARGET, &partition)
        .await
        .expect("synthetic reconciliation fixture");
    assert_eq!(reviewed.conflicts(), 2);
    for row in &mut reviewed.rows {
        if row.conflict {
            row.keep_destination = true;
        }
    }
    assert_eq!(reviewed.conflicts(), 0);
    assert_eq!(
        reconcile(&target.pool, id(700), &partition, &reviewed, true)
            .await
            .expect("synthetic reconciliation fixture")
            .inserted,
        1
    );
    assert_eq!(
        rows(&target.pool, "public.memory_canonical_state").await,
        canonical_before
    );
    assert!(
        reconcile(&target.pool, id(700), &partition, &reviewed, true)
            .await
            .expect("synthetic reconciliation fixture")
            .already_applied
    );
    execute(
        &source.pool,
        "UPDATE memory_entries SET summary='changed history' WHERE version_no=1",
    )
    .await;
    let (changed, _) = capture(&source.pool).await.partition();
    let mut invalid = preview(&target.pool, TARGET, &changed)
        .await
        .expect("synthetic reconciliation fixture");
    for row in &mut invalid.rows {
        if row.conflict {
            row.keep_destination = true;
        }
    }
    assert_eq!(
        reconcile(&target.pool, id(701), &changed, &invalid, true)
            .await
            .expect_err("invalid reconciliation must fail"),
        ImportError::Invalid
    );
    target
        .cleanup()
        .await
        .expect("synthetic reconciliation fixture");
    source
        .cleanup()
        .await
        .expect("synthetic reconciliation fixture");
}
