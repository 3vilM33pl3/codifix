// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

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
        store_loop_setting_tx(&mut tx, &definition.loop_id, &scope, &request, &bad_context)
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
        &definition.loop_id,
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
