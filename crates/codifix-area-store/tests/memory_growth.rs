// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic bounded diagnostic: never connect to a source or import target.
#[allow(dead_code)]
#[path = "support/mod.rs"]
mod support;

use sqlx::{PgConnection, Row};

async fn contexts(conn: &mut PgConnection, phase: &str) -> i64 {
    let rows = sqlx::query("SELECT name,sum(total_bytes)::bigint AS bytes FROM pg_backend_memory_contexts GROUP BY name ORDER BY bytes DESC LIMIT 8")
        .persistent(false).fetch_all(&mut *conn).await.expect("synthetic backend memory contexts");
    let total: i64 =
        sqlx::query_scalar("SELECT sum(total_bytes)::bigint FROM pg_backend_memory_contexts")
            .persistent(false)
            .fetch_one(&mut *conn)
            .await
            .expect("synthetic total context bytes");
    eprintln!("{phase}: total_context_bytes={total}");
    for row in rows {
        eprintln!(
            "  {}: {}",
            row.get::<String, _>("name"),
            row.get::<i64, _>("bytes")
        );
    }
    assert!(
        total < 512 * 1024 * 1024,
        "bounded diagnostic stops before excessive backend memory"
    );
    total
}

#[tokio::test]
#[ignore = "bounded synthetic PostgreSQL memory diagnosis; run only in a resource-limited disposable server"]
async fn composite_statement_memory_probe() {
    for (cache_capacity, persistent) in [(100, true), (0, true), (0, false)] {
        let Some(db) = support::database().await else {
            return;
        };
        support::seed(&db.pool).await;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_with(
                db.pool
                    .connect_options()
                    .as_ref()
                    .clone()
                    .statement_cache_capacity(cache_capacity),
            )
            .await
            .expect("synthetic operator-equivalent connection");
        let mut tx = pool.begin().await.expect("synthetic transaction");
        let baseline = contexts(&mut tx, "start").await;
        for n in 0..20_000u128 {
            let value = serde_json::json!({
                "id":support::id(100_000+n), "relation_type":format!("synthetic_{n}"),
                "src_canonical_id":support::id(13), "dst_canonical_id":support::id(13),
                "origin":"derived", "confidence":0.5, "actor":"synthetic", "created_at":"2026-09-20T00:00:00Z"
            }).to_string();
            sqlx::query("INSERT INTO public.memory_relations (id,relation_type,src_canonical_id,dst_canonical_id,origin,confidence,actor,created_at) OVERRIDING SYSTEM VALUE SELECT id,relation_type,src_canonical_id,dst_canonical_id,origin,confidence,actor,created_at FROM json_populate_record(NULL::public.memory_relations,$1::json)")
                .bind(&value).persistent(persistent).execute(&mut *tx).await.expect("synthetic insert");
            if (n + 1) % 5000 == 0 {
                contexts(
                    &mut tx,
                    &format!(
                        "cache={cache_capacity}, persistent={persistent}, rows={}",
                        n + 1
                    ),
                )
                .await;
            }
        }
        let end = contexts(&mut tx, "end").await;
        eprintln!(
            "cache={cache_capacity}, persistent={persistent}, growth_bytes={}",
            end - baseline
        );
        tx.rollback().await.expect("synthetic rollback");
        pool.close().await;
        db.cleanup().await.expect("synthetic cleanup");
    }
}

#[tokio::test]
#[ignore = "100k synthetic relation import; run only in a resource-limited disposable server"]
async fn full_import_large_relation_memory_probe() {
    uncached_import_and_replay(100_000).await;
}

#[tokio::test]
async fn uncached_import_and_replay_do_not_accumulate_named_statements() {
    uncached_import_and_replay(2048).await;
}

async fn uncached_import_and_replay(relation_count: i32) {
    let Some(source) = support::database().await else {
        return;
    };
    let target = support::database().await.expect("synthetic target");
    support::seed(&source.pool).await;
    sqlx::query("INSERT INTO memory_relations(id,relation_type,src_canonical_id,dst_canonical_id,origin,confidence,actor,created_at) SELECT ('00000000-0000-0000-0000-'||lpad(to_hex(n+100000),12,'0'))::uuid,'synthetic_'||n,'00000000-0000-0000-0000-00000000000d','00000000-0000-0000-0000-00000000000d','derived',0.5,'synthetic','2026-09-20T00:00:00Z' FROM generate_series(0,$1-1) n")
        .bind(relation_count).execute(&source.pool).await.expect("synthetic relations");
    support::execute(&target.pool,r#"CREATE FUNCTION synthetic_memory_growth_guard() RETURNS trigger LANGUAGE plpgsql AS $$
      DECLARE context_bytes bigint;
      BEGIN
        IF get_byte(uuid_send(NEW.id),15)=0 THEN
          SELECT sum(total_bytes) INTO context_bytes FROM pg_backend_memory_contexts;
          IF context_bytes > 16777216 THEN RAISE EXCEPTION 'synthetic backend memory bound exceeded'; END IF;
        END IF;
        RETURN NEW;
      END $$;
      CREATE TRIGGER synthetic_memory_growth_guard AFTER INSERT ON memory_relations FOR EACH ROW EXECUTE FUNCTION synthetic_memory_growth_guard();"#).await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            target
                .pool
                .connect_options()
                .as_ref()
                .clone()
                .statement_cache_capacity(0),
        )
        .await
        .expect("zero-cache operator connection");
    eprintln!("synthetic{relation_count}: capture");
    let mut fixture = support::capture(&source.pool).await;
    fixture.manifest.format_version = codifix_area::PROVENANCE_MANIFEST_VERSION;
    fixture.project_provenance();
    eprintln!("synthetic{relation_count}: materialize");
    let (partition, _) = fixture.partition();
    drop(fixture);
    eprintln!("synthetic{relation_count}: import");
    let receipt = codifix_area_store::import_partition(
        &pool,
        support::TARGET,
        support::OPERATION,
        &partition,
    )
    .await
    .expect("synthetic large import");
    assert_eq!(
        receipt.row_count,
        (partition.rows().len() + partition.projected_sources().len()) as i64
    );
    assert_eq!(receipt.projection_count, 1);
    check_import_memory(&pool, "after complete import").await;
    eprintln!("synthetic{relation_count}: replay");
    assert!(
        codifix_area_store::import_partition(
            &pool,
            support::TARGET,
            support::OPERATION,
            &partition
        )
        .await
        .expect("synthetic replay")
        .already_applied
    );
    check_import_memory(&pool, "after complete replay").await;
    pool.close().await;
    target.cleanup().await.expect("synthetic cleanup");
    source.cleanup().await.expect("synthetic cleanup");
}

async fn check_import_memory(pool: &sqlx::PgPool, phase: &str) {
    let mut conn = pool.acquire().await.expect("same operator backend");
    let total = contexts(&mut conn, phase).await;
    assert!(
        total < 16 * 1024 * 1024,
        "bounded complete importer backend"
    );
    let statements: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_prepared_statements")
        .persistent(false)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared statement count");
    assert_eq!(
        statements, 0,
        "uncached import and replay must leave no named statements"
    );
}
