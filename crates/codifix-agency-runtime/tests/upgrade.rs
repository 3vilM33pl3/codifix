#![allow(clippy::unwrap_used)]
use codifix_test_support::IsolatedDatabase;
use std::process::Command;

async fn fixture_url(db: &IsolatedDatabase) -> String {
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let mut url = url::Url::parse(&std::env::var("CODIFIX_TEST_DATABASE_URL").unwrap()).unwrap();
    url.set_path(&name);
    url.into()
}

#[tokio::test]
#[ignore = "requires disposable PDS, previous agency binary and native OpenBao"]
async fn pds_social_upgrade_restarts_and_reports() {
    let publisher = IsolatedDatabase::new().await.unwrap();
    let ca = IsolatedDatabase::new().await.unwrap();
    let run = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/federation-pds/upgrade.py"
        ))
        .env("CANDIDATE_AGENCY", env!("CARGO_BIN_EXE_codifix-agency"))
        .env("CANDIDATE_VERSION", env!("CARGO_PKG_VERSION"))
        .env("PUBLISHER_DB", fixture_url(&publisher).await)
        .env("CA_DB", fixture_url(&ca).await)
        .status()
        .unwrap();
    publisher.cleanup().await.unwrap();
    ca.cleanup().await.unwrap();
    assert!(run.success());
}
