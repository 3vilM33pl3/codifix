// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[tokio::test]
async fn interrupted_loop_preserves_committed_phases_and_rejects_stale_outputs() {
    let Some(database) = mem_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let pool = &database.pool;
    let project = Uuid::new_v4();
    let run = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'loop recovery','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(pool)
    .await
    .unwrap();
    let owner = mem_store::executions::Execution::acquire(pool, run)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    owner
        .register(&mut tx, mem_store::executions::Kind::Loop, Some(project))
        .await
        .unwrap();
    sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status) VALUES($1,'fixture',1,$2,'project',$3,'suggest_only','running')").bind(run).bind(project).bind(project.to_string()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    store_run_phase(
        pool,
        run,
        "first",
        "First phase",
        &json!({"completed":true}),
        None,
        None,
    )
    .await
    .unwrap();
    reconcile_interrupted_runs(pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap(),
        "running"
    );
    let pid: i32 = sqlx::query_scalar("SELECT backend_pid FROM foreground_executions WHERE id=$1")
        .bind(run)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(pool)
        .await
        .unwrap();
    drop(owner);
    assert!(
        store_run_phase(
            pool,
            run,
            "stale",
            "Stale phase",
            &json!({"must_not_commit":true}),
            None,
            None
        )
        .await
        .is_err()
    );
    reconcile_interrupted_runs(pool).await.unwrap();
    reconcile_interrupted_runs(pool).await.unwrap();
    let output: serde_json::Value =
        sqlx::query_scalar("SELECT output_json FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(output["first"]["completed"], true);
    assert!(output.get("stale").is_none());
    assert_eq!(output["interrupted"], true);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(pool)
            .await
            .unwrap(),
        "failed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM run_traces WHERE run_id=$1 AND trace_type='interrupted'"
        )
        .bind(run)
        .fetch_one(pool)
        .await
        .unwrap(),
        1
    );
    let proposal = LoopMemoryProposalCreateRequest {
        project: project.to_string(),
        loop_id: "fixture".into(),
        proposal_type: "create".into(),
        run_id: Some(run),
        target_memory_id: None,
        candidate: json!({"summary":"stale"}),
        evidence: json!([]),
        confidence: 0.5,
        risk_notes: None,
    };
    assert!(
        proposal_creation::create(
            pool,
            &proposal,
            Uuid::new_v4(),
            &mem_store::events::MutationContext::internal("stale-executor")
        )
        .await
        .is_err()
    );
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn terminal_loop_state_jobs_and_event_commit_together() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = Uuid::new_v4();
    let run = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'R1 phases','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status) VALUES($1,'memory.consolidation',1,$2,'project',$3,'manual','running')")
        .bind(run).bind(project).bind(project.to_string()).execute(&pool).await.unwrap();
    let request: LoopRunRequest =
        serde_json::from_value(json!({"project":project.to_string()})).unwrap();
    let mut bad = LoopExecution::internal();
    bad.context.actor_name = "late\0event".into();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        finish_run_tx(
            &mut tx,
            Some(project),
            run,
            mem_loops::LOOP_MEMORY_CONSOLIDATION,
            &request,
            true,
            &bad
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM background_jobs WHERE operation_id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM run_traces WHERE run_id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    finish_run(
        &pool,
        Some(project),
        run,
        mem_loops::LOOP_MEMORY_CONSOLIDATION,
        &request,
        true,
        &LoopExecution::internal(),
    )
    .await
    .unwrap();
    let input: serde_json::Value =
        sqlx::query_scalar("SELECT input FROM background_jobs WHERE operation_id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(input["manual"], true);
    assert_eq!(input["run_id"], run.to_string());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "succeeded"
    );
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancellation_and_approval_are_atomic_with_the_run_journal() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    let project = Uuid::new_v4();
    let run = Uuid::new_v4();
    let approval = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'R1 admin','/synthetic')",
    )
    .bind(project)
    .bind(project.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO loop_runs(id,loop_id,definition_version,project_id,scope_type,scope_id,mode,status) VALUES($1,'test',1,$2,'project',$3,'suggest_only','running')")
        .bind(run).bind(project).bind(project.to_string()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO approval_requests(id,project_id,run_id,loop_id,action_type,proposed_action_json,risk_reason,status) VALUES($1,$2,$3,'test','write_memory_proposal','{}','Review required','pending')")
        .bind(approval).bind(project).bind(run).execute(&pool).await.unwrap();
    let request: LoopApprovalDecisionRequest =
        serde_json::from_value(json!({"reviewer":"client-reported-author"})).unwrap();
    let mut bad_context = mem_store::events::MutationContext::internal("admin-test");
    bad_context.actor_id = "late\0failure".into();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        resolve_loop_approval_tx(
            &mut tx,
            approval,
            LoopApprovalStatus::Rejected,
            &request,
            &bad_context
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM approval_requests WHERE id=$1")
            .bind(approval)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "pending"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM run_traces WHERE run_id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("CREATE TEMP TABLE run_traces (LIKE public.run_traces INCLUDING DEFAULTS, CHECK(FALSE)) ON COMMIT DROP").execute(&mut *tx).await.unwrap();
    let cancel = LoopCancelRequest {
        reason: Some("synthetic secret should not enter trace".into()),
    };
    assert!(
        cancel_loop_run_tx(
            &mut tx,
            run,
            &cancel,
            &mem_store::events::MutationContext::internal("cancel-test")
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM loop_runs WHERE id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "running"
    );
    let context = mem_store::events::MutationContext::internal("admin-test");
    resolve_loop_approval_as(
        &pool,
        approval,
        LoopApprovalStatus::Rejected,
        &request,
        &context,
    )
    .await
    .unwrap();
    assert!(
        resolve_loop_approval_as(
            &pool,
            approval,
            LoopApprovalStatus::Approved,
            &request,
            &context
        )
        .await
        .is_err()
    );
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload_json FROM run_traces WHERE run_id=$1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(payload.get("proposed_action").is_none());
    assert!(payload.get("reviewer").is_none());
    cancel_loop_run_record(&pool, run, &cancel, &context)
        .await
        .unwrap();
    let cancellation: serde_json::Value = sqlx::query_scalar(
        "SELECT payload_json FROM run_traces WHERE run_id=$1 AND trace_type='cancel'",
    )
    .bind(run)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cancellation["actor_id"], context.actor_id);
    assert!(!cancellation.to_string().contains("synthetic secret"));
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn setting_and_global_switch_roll_back_with_their_audits() {
    let Some(pool) = mem_test_support::migrated_pool().await else {
        return;
    };
    register_builtin_loop_definitions(&pool).await.unwrap();
    let id = Uuid::new_v4();
    let slug = id.to_string();
    sqlx::query(
        "INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'R1 settings','/synthetic')",
    )
    .bind(id)
    .bind(&slug)
    .execute(&pool)
    .await
    .unwrap();
    let request: LoopSettingsUpdateRequest =
        serde_json::from_value(json!({"project":slug,"enabled":false})).unwrap();
    let scope = resolve_loop_scope(&pool, &request, true).await.unwrap();
    let definition = &builtin_loop_definitions()[0];
    let mut bad_context = mem_store::events::MutationContext::internal("test-settings");
    bad_context.actor_name = "fail\0event".into();
    let mut tx = pool.begin().await.unwrap();
    assert!(
        store_loop_setting_tx(&mut tx, definition.loop_id, &scope, &request, &bad_context)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM loop_settings WHERE project_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM loop_setting_audit WHERE scope_id=$1")
            .bind(&slug)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );

    let mut tx = pool.begin().await.unwrap();
    let (_, receipt) = store_loop_setting_tx(
        &mut tx,
        definition.loop_id,
        &scope,
        &request,
        &mem_store::events::MutationContext::internal("test-settings"),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(receipt.is_some());

    // Shadow only this connection's audit table. No global trigger or customer data.
    let before = fetch_loop_global_state(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("CREATE TEMP TABLE loop_setting_audit (LIKE public.loop_setting_audit INCLUDING DEFAULTS, CHECK(FALSE)) ON COMMIT DROP")
        .execute(&mut *tx).await.unwrap();
    assert!(
        store_loop_global_state_tx(
            &mut tx,
            &LoopGlobalStateUpdateRequest {
                kill_switch_enabled: !before.kill_switch_enabled,
                updated_by: Some("test-actor".into()),
                reason: None,
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    let after = fetch_loop_global_state(&pool).await.unwrap();
    assert_eq!(before.kill_switch_enabled, after.kill_switch_enabled);
    assert_eq!(before.updated_at, after.updated_at);
    sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}
