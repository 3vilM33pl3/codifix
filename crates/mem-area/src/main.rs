// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::File,
    io::{BufReader, Write},
    path::PathBuf,
    process::ExitCode,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mem_area::{
    Inventory, Manifest, draft_manifest, inventory_digest, verify_inventory, verify_manifest,
};
use serde::de::DeserializeOwned;

#[derive(Parser)]
#[command(
    name = "memory-area",
    about = "Offline area migration planner (no database access or import)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate and summarize a complete externally collected JSON inventory.
    Inventory {
        #[arg(long)]
        input: PathBuf,
    },
    /// Emit an unresolved manifest for source/project/content review.
    Plan {
        #[arg(long)]
        inventory: PathBuf,
    },
    /// Verify reviewed decisions against the pinned source inventory.
    Verify {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
    },
}

fn read<T: DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parse {}", path.display()))
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

fn run(cli: Cli) -> Result<bool> {
    match cli.command {
        Command::Inventory { input } => {
            let inventory: Inventory = read(&input)?;
            let blockers = verify_inventory(&inventory);
            print_json(&serde_json::json!({
                "inventory_valid": blockers.is_empty(), "import_supported": false,
                "inventory_sha256": inventory_digest(&inventory)?,
                "table_count": inventory.tables.len(), "project_count": inventory.projects.len(),
                "record_count": inventory.tables.iter().map(|table| table.rows.len()).sum::<usize>(),
                "blockers": blockers,
            }))?;
            Ok(blockers.is_empty())
        }
        Command::Plan { inventory } => {
            let inventory: Inventory = read(&inventory)?;
            let blockers = verify_inventory(&inventory);
            if !blockers.is_empty() {
                print_json(
                    &serde_json::json!({ "inventory_valid": false, "import_supported": false, "blockers": blockers }),
                )?;
                return Ok(false);
            }
            print_json(&draft_manifest(&inventory)?)?;
            Ok(true)
        }
        Command::Verify {
            inventory,
            manifest,
        } => {
            let inventory: Inventory = read(&inventory)?;
            let manifest: Manifest = read(&manifest)?;
            let report = verify_manifest(&inventory, &manifest);
            print_json(&report)?;
            Ok(report.plan_valid)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::from(1)
        }
    }
}
