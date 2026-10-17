// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use codifix_config::AppConfig;
use reqwest::Client;

use crate::commands::{
    api::get_json, output::print_memory_history, output::service_url, runtime::HistoryArgs,
};

pub(super) async fn handle(args: HistoryArgs, client: Client, config: AppConfig) -> Result<()> {
    if let Some(subscription) = args.subscription {
        let api = super::api::ApiClient::new(client, config);
        let inventory = api.laptop_cache().await?;
        let selected = inventory
            .projects
            .iter()
            .find(|p| p.subscription_id == subscription)
            .context("subscription was not selected for this laptop area")?;
        let history = api
            .laptop_cache_history(
                selected,
                args.memory_id,
                args.after_version.unwrap_or(0),
                args.limit.unwrap_or(16),
            )
            .await?;
        if args.json {
            println!("{}", serde_json::to_string(&history)?);
        } else {
            println!(
                "Read-only owner history: {} (owner {})",
                history.canonical_id, history.project.owner_instance_id
            );
            for version in &history.versions {
                println!(
                    "\nVersion {}{} · {} · {}\nAuthor: {}\nSHA-256: {}\n{}",
                    version.version,
                    if version.tombstone {
                        " (tombstone)"
                    } else {
                        ""
                    },
                    version.id,
                    version.summary,
                    version.author,
                    version.digest,
                    version.text
                );
                if !version.unavailable_source_ids.is_empty() {
                    println!(
                        "{} source references unavailable; raw bytes remain with the owner.",
                        version.unavailable_source_ids.len()
                    );
                }
            }
            if let Some(next) = history.next_after_version {
                println!("More versions: repeat with --after-version {next}");
            }
        }
        return Ok(());
    }
    let payload: codifix_record::MemoryHistoryResponse = get_json(
        client
            .get(service_url(
                &config,
                &format!("/v1/memory/{}/history", args.memory_id),
            ))
            .send()
            .await
            .context("history request failed")?,
    )
    .await?;
    if args.json {
        println!("{}", serde_json::to_string(&payload)?);
    } else {
        print_memory_history(&payload);
    }

    Ok(())
}
