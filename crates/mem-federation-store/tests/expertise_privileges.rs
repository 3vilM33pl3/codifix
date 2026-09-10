// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(clippy::unwrap_used)]

#[tokio::test]
async fn deployment_role_cannot_read_origin_collections_or_erase_publication_history() {
    let Some(db) = mem_test_support::IsolatedDatabase::new().await else {
        return;
    };
    let mut tx = db.pool.begin().await.unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let role = format!("f1_test_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE {role} NOLOGIN"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let grants = include_str!("../../../deploy/federation/gateway-grants.sql")
        .replace("memory_isolation", &database)
        .replace("gateway_runtime", &role);
    sqlx::raw_sql(&grants).execute(&mut *tx).await.unwrap();
    for (table, privileges, expected) in [
        (
            "federation_expertise_mailbox",
            "SELECT,INSERT,UPDATE,DELETE",
            true,
        ),
        (
            "federation_expertise_attempts",
            "SELECT,INSERT,DELETE",
            true,
        ),
        ("federation_expertise_attempts", "UPDATE", false),
        (
            "federation_expertise_daily_budget",
            "SELECT,INSERT,UPDATE",
            true,
        ),
        ("federation_expertise_daily_budget", "DELETE", false),
        (
            "federation_expertise_deliveries",
            "SELECT,INSERT,UPDATE,DELETE",
            true,
        ),
        (
            "federation_expertise_reviews",
            "SELECT,INSERT,UPDATE,DELETE",
            false,
        ),
        (
            "federation_expertise_collection_grants",
            "SELECT,INSERT,UPDATE,DELETE",
            false,
        ),
        ("federation_accepted", "DELETE,TRUNCATE", false),
        (
            "federation_expertise_requests",
            "SELECT,INSERT,UPDATE,DELETE",
            false,
        ),
        (
            "federation_expertise_request_audit",
            "SELECT,INSERT,UPDATE,DELETE",
            false,
        ),
        ("federation_audit", "DELETE,TRUNCATE", false),
        ("auth_principals", "SELECT,INSERT,UPDATE,DELETE", false),
    ] {
        // has_table_privilege with a comma list means ANY; assert each exact
        // operation so accidentally missing one grant cannot pass this test.
        for privilege in privileges.split(',') {
            let actual: bool = sqlx::query_scalar("SELECT has_table_privilege($1,$2,$3)")
                .bind(&role)
                .bind(table)
                .bind(privilege)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(actual, expected, "{table} {privilege}");
        }
    }
    // Role and grants exist only inside this disposable transaction.
    tx.rollback().await.unwrap();
    db.cleanup().await.unwrap();
}
