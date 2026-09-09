// SPDX-License-Identifier: AGPL-3.0-or-later
//! Disposable test provisioning only; never installed with the gateway binary.
use mem_federation_runtime::{GatewaySecurity, private_key, private_secret};
use std::path::PathBuf;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("MEMORY_FEDERATION_ISOLATION_DISPOSABLE").as_deref() != Ok("1") {
        return Err("fixture opt-in required".into());
    }
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("fixture directory required")?,
    );
    let url = private_secret(&root.join("admin-url"))?;
    if !url.ends_with("/memory_isolation") {
        return Err("fixture database required".into());
    }
    let pool = sqlx::PgPool::connect(&url).await?;
    if std::env::args().nth(2).as_deref() == Some("verify") {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM federation_publish_outbox WHERE blocked AND last_error='invalid_receipt')").fetch_one(&pool).await?;
        if !blocked {
            let status: Vec<(i32, bool, Option<String>)> =
                sqlx::query_as("SELECT attempts,blocked,last_error FROM federation_publish_outbox")
                    .fetch_all(&pool)
                    .await?;
            return Err(format!(
                "expected TLS PDS response to synthetic invalid credentials; state {status:?}"
            )
            .into());
        }
        let mut scoped = reqwest::Url::parse(&url)?;
        scoped
            .set_username("gateway_runtime")
            .map_err(|_| "fixture URL")?;
        scoped
            .set_password(Some(&private_secret(&root.join("gateway-password"))?))
            .map_err(|_| "fixture URL")?;
        let restricted = sqlx::PgPool::connect(scoped.as_str()).await?;
        for query in [
            "SELECT * FROM memory_entries LIMIT 1",
            "SELECT * FROM auth_principals LIMIT 1",
            "DELETE FROM federation_audit",
            "TRUNCATE federation_audit",
            "CREATE TABLE forbidden_fixture(id int)",
        ] {
            if sqlx::query(query).execute(&restricted).await.is_ok() {
                return Err("gateway SQL role exceeded table-limited authority".into());
            }
        }
        println!("PASS gateway TLS-to-PDS positive control and restricted SQL role");
        return Ok(());
    }
    mem_test_support::run_migrations(&pool).await?;
    let password = private_secret(&root.join("gateway-password"))?;
    if !password.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("fixture password must be hex".into());
    }
    sqlx::query(&format!("CREATE ROLE gateway_runtime LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS")).execute(&pool).await?;
    sqlx::raw_sql(include_str!(
        "../../../deploy/federation/gateway-grants.sql"
    ))
    .execute(&pool)
    .await?;
    let security = GatewaySecurity::initial(
        &private_key(&root.join("gateway/audit-key"))?,
        Default::default(),
    )?;
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("metadata.json"))?)?;
    let space = serde_json::from_value(meta["space"].clone())?;
    let authority = serde_json::from_value(meta["authority"].clone())?;
    let member = serde_json::from_value(meta["member"].clone())?;
    let store = mem_federation_store::GatewayStore::new(pool.clone());
    let fence = store.initialize(space, authority).await?;
    store
        .accept(
            fence,
            authority,
            &mem_federation_store::PublicationIntent {
                locator: mem_federation::RecordLocator::new("fixture:bootstrap")?,
                envelope: mem_federation::publication::PublicationEnvelope {
                    schema_version: 2,
                    space,
                    publisher: authority,
                    record_id: mem_federation::publication::RecordId::new(uuid::Uuid::new_v4())?,
                    author: mem_federation::AuthorIdentity::new("fixture-administrator")?,
                    authored_at: chrono::Utc::now(),
                    causation: None,
                    record: mem_federation::publication::PublicationRecord::Membership {
                        member,
                        capability: mem_federation::SpaceCapability::Pull,
                    },
                },
            },
        )
        .await?;
    // Only public enrolled keys on stdout. All secrets remain in private files.
    println!(
        "{}",
        serde_json::json!({"receipt_keys":security.trust().keys(),"pds_signing_key":mem_atproto_verify::DetachedSigner::from_secret(&[1;32])?.public_key()})
    );
    Ok(())
}
