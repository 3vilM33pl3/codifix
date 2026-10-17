// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only operator check over the explicitly selected candidate DB. Metadata output only.
use codifix_federation::publication::SpaceId;
use codifix_federation_runtime::verify_retired_history;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("CODIFIX_CUTOVER_DATABASE_URL")?)
        .await?;
    let spaces:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT space FROM codifix_retired_federation_spaces WHERE jsonb_array_length(replicas)>0 ORDER BY space").fetch_all(&pool).await?;
    let mut records = 0;
    let mut receipts = 0;
    for space in &spaces {
        let (state, audit) = verify_retired_history(&pool, SpaceId::new(*space)?).await?;
        records += state.accepted().len();
        receipts += audit.len();
    }
    println!(
        "verified retired spaces={} records={} receipts={}",
        spaces.len(),
        records,
        receipts
    );
    pool.close().await;
    Ok(())
}
