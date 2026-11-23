#![allow(clippy::unwrap_used)]
use codifix_test_support::IsolatedDatabase;
use sqlx::ConnectOptions;

#[tokio::test]
#[ignore = "requires local Docker, Step, OpenBao, OpenSSH, socat and Python cryptography"]
async fn two_companions_renew_only_after_operator_signing_and_fresh_login() {
    let home = IsolatedDatabase::new()
        .await
        .expect("disposable DB required");
    let ca = IsolatedDatabase::new()
        .await
        .expect("disposable DB required");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/ssh-renewal/test_renewal.py");
    let result = tokio::process::Command::new("python3")
        .arg(path)
        .env(
            "RENEW_HOME_DB",
            home.pool.connect_options().to_url_lossy().as_str(),
        )
        .env(
            "RENEW_CA_DB",
            ca.pool.connect_options().to_url_lossy().as_str(),
        )
        .env("RENEW_AGENCY_BINARY", env!("CARGO_BIN_EXE_codifix-agency"))
        .status()
        .await
        .unwrap();
    ca.cleanup().await.unwrap();
    home.cleanup().await.unwrap();
    assert!(result.success());
}
