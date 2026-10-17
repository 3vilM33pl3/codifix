//! Narrow operator-only migration for the independent local preview. Never
//! discovers databases, starts services, rotates credentials or enables models.
use sqlx::{Connection, postgres::PgConnectOptions};
use std::str::FromStr;
use uuid::Uuid;

#[tokio::main]
async fn main() {
    if upgrade().await.is_err() {
        eprintln!(
            "Preview upgrade unconfirmed; private diagnostics suppressed. Inspect migration history before retrying."
        );
        std::process::exit(1);
    }
}
async fn upgrade() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--writers-stopped-backup-verified"] {
        return Err("explicit checkpoint required".into());
    }
    let options = PgConnectOptions::from_str(&std::env::var("DATABASE_URL")?)?;
    if options.get_host() != "127.0.0.1"
        || options.get_database() != Some("memory_workshop_preview_20260918")
    {
        return Err("wrong database".into());
    }
    let mut conn = sqlx::PgConnection::connect_with(&options).await?;
    let instance: Uuid = sqlx::query_scalar(
        "SELECT instance_id FROM supervisor_instance_binding WHERE area='workshop'",
    )
    .fetch_one(&mut conn)
    .await?;
    if instance != Uuid::parse_str("72d1a39c-29fe-4f8d-94ee-b3a8dba51f85")? {
        return Err("wrong instance".into());
    }
    let clients:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()").fetch_one(&mut conn).await?;
    let head: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
        .fetch_one(&mut conn)
        .await?;
    if clients != 0 || ![58, 59].contains(&head) {
        return Err("unexpected clients or schema".into());
    }
    sqlx::migrate!("../../migrations").run(&mut conn).await?;
    sqlx::raw_sql("GRANT SELECT,INSERT ON supervisor_proposal_reviews TO workshop_app;")
        .execute(&mut conn)
        .await?;
    println!("{{\"migration_head\":59,\"new_provider_calls\":0,\"instance_id\":\"{instance}\"}}");
    Ok(())
}
