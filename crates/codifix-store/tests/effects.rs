// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use codifix_store::effects::*;
use uuid::Uuid;

#[tokio::test]
async fn external_intents_replay_without_rewriting_inputs_and_ack_failure_stays_uncertain() {
    let Some(pool) = codifix_test_support::migrated_pool().await else {
        return;
    };
    let id = Uuid::new_v4();
    let intent = EffectIntent {
        id,
        project_id: None,
        kind: EffectKind::FileReplace,
        host_id: id.to_string(),
        resource: "/synthetic/config.toml".into(),
        input_fingerprint: "a".repeat(64),
        actor_id: "test-operator".into(),
        source: "effect-test".into(),
    };
    begin(&pool, &intent).await.unwrap();
    begin(&pool, &intent).await.unwrap();
    let mut conflict = intent.clone();
    conflict.input_fingerprint = "b".repeat(64);
    assert!(begin(&pool, &conflict).await.is_err());
    let receipt = Uuid::new_v4();
    // Simulate loss of the result transaction after the external effect happened.
    let mut tx = pool.begin().await.unwrap();
    conclude_tx(
        &mut tx,
        id,
        receipt,
        EffectStatus::Confirmed,
        EffectReason::Observed,
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM external_effects WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "pending"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_outcomes WHERE effect_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert!(
        pending(&pool, &intent.host_id, 100)
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query(
        "UPDATE external_effects SET reconcile_after=now()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(pending(&pool, &intent.host_id, 1).await.unwrap()[0].id, id);
    assert!(
        pending(&pool, "another-host", 100)
            .await
            .unwrap()
            .is_empty()
    );
    conclude(
        &pool,
        id,
        Uuid::new_v4(),
        EffectStatus::Uncertain,
        EffectReason::Interrupted,
    )
    .await
    .unwrap();
    conclude(
        &pool,
        id,
        receipt,
        EffectStatus::Confirmed,
        EffectReason::Observed,
    )
    .await
    .unwrap();
    conclude(
        &pool,
        id,
        receipt,
        EffectStatus::Confirmed,
        EffectReason::Observed,
    )
    .await
    .unwrap();
    assert!(
        conclude(
            &pool,
            id,
            Uuid::new_v4(),
            EffectStatus::Failed,
            EffectReason::LocalIo
        )
        .await
        .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM external_effect_outcomes WHERE effect_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    sqlx::query("DELETE FROM external_effect_outcomes WHERE effect_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM external_effects WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}
