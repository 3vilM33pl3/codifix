// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit database-administrator tool, not a public service authorization path.
use uuid::Uuid;

#[tokio::main]
async fn main() {
    if repair().await.is_err() {
        eprintln!(
            "Repair was not confirmed. Check inputs and protected database diagnostics; retry the same operation UUID after an uncertain commit. No raw connection error is printed."
        );
        std::process::exit(1);
    }
}

async fn repair() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 5 || args[0] != "--apply" {
        eprintln!(
            "Usage: repair-post-curation --apply PROJECT_UUID OPERATION_UUID PRINCIPAL_UUID VERSION_UUID... (1-200 versions); requires MEMORY_LAYER_REPAIR_DATABASE_URL"
        );
        return Err("explicit repair confirmation and identifiers required".into());
    }
    let project = Uuid::parse_str(&args[1])?;
    let operation = Uuid::parse_str(&args[2])?;
    let principal = Uuid::parse_str(&args[3])?;
    let versions: Vec<Uuid> = args[4..]
        .iter()
        .map(|s| Uuid::parse_str(s))
        .collect::<Result<_, _>>()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("MEMORY_LAYER_REPAIR_DATABASE_URL")?)
        .await?;
    let mut tx = pool.begin().await?;
    let work = codifix_store::post_curation::request_repair(
        &mut tx, project, operation, principal, &versions,
    )
    .await?;
    tx.commit().await?;
    println!("{}", serde_json::to_string(&work)?);
    pool.close().await;
    Ok(())
}
