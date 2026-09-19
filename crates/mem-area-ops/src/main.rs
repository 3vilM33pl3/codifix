// SPDX-License-Identifier: AGPL-3.0-or-later
use clap::{Args, Parser, Subcommand};
use mem_area_ops::{Execution, Failure, Inputs};
use std::{io::Write, path::PathBuf, process::ExitCode};
use uuid::Uuid;
#[derive(Parser)]
#[command(
    name = "memory-area-import",
    about = "Explicit private migration preparation/import; never discovers databases or activates services"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Args)]
struct Source {
    #[arg(long)]
    inventory: PathBuf,
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    payload: PathBuf,
    /// Normalized digest printed by memory-area inventory.
    #[arg(long)]
    inventory_sha256: String,
    /// SHA-256 of compact serde manifest JSON, not file whitespace.
    #[arg(long)]
    manifest_sha256: String,
}

#[derive(Args)]
struct Retention {
    /// Total retained target MiB (256..=1024); repeat the prepared receipt's value for import.
    #[arg(long, default_value_t = mem_area::DEFAULT_RETAINED_TARGET_MIB,
        value_parser = clap::value_parser!(u32).range(256..=1024))]
    max_retained_target_mib: u32,
}

impl Retention {
    fn budget(self) -> Result<mem_area::RetainedTargetBudget, Failure> {
        Ok(mem_area::RetainedTargetBudget::from_mib(
            self.max_retained_target_mib,
        )?)
    }
}
impl From<Source> for Inputs {
    fn from(s: Source) -> Self {
        Self {
            inventory: s.inventory,
            manifest: s.manifest,
            payload: s.payload,
            inventory_sha256: s.inventory_sha256,
            manifest_sha256: s.manifest_sha256,
        }
    }
}
#[derive(Subcommand)]
enum Command {
    /// Generate an exact reviewed preview manifest in protected files only; no DB or consent grant.
    PreviewManifest {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        selection_sha256: String,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Inspect the exact compiled schema preparation digest; no DB connection.
    SchemaPlan,
    /// Initialize schema only in a fresh empty local DB; never start a service.
    InitializeSchema {
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        target_sha256: String,
        #[arg(long)]
        schema_sha256: String,
        #[arg(long)]
        execute: bool,
        #[arg(long)]
        writers_stopped: bool,
        #[arg(long)]
        fresh_database: bool,
    },
    /// Independently verify every exported row; no ownership review, writes or DB connection.
    VerifyPayload {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        payload: PathBuf,
        #[arg(long)]
        inventory_sha256: String,
    },
    /// Print normalized review digests and blocker count, without writing or connecting.
    Inspect {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Validate reviewed inputs, durably publish a full archive and metadata receipt.
    Prepare {
        #[command(flatten)]
        source: Source,
        #[command(flatten)]
        retention: Retention,
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Import only into an explicitly selected local, migrated, inactive target.
    Import {
        #[command(flatten)]
        source: Source,
        #[command(flatten)]
        retention: Retention,
        #[arg(long)]
        prepared: PathBuf,
        #[arg(long)]
        prepared_sha256: String,
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        target_sha256: String,
        #[arg(long)]
        operation_id: Uuid,
        #[arg(long)]
        execute: bool,
        #[arg(long)]
        writers_stopped: bool,
        #[arg(long)]
        archive_backed_up: bool,
    },
}
fn print(value: &impl serde::Serialize) -> Result<(), Failure> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(|_| Failure::Output)?;
    writeln!(output).map_err(|_| Failure::Output)
}
async fn run(cli: Cli) -> Result<(), Failure> {
    match cli.command {
        Command::PreviewManifest {
            inventory,
            selection,
            selection_sha256,
            output_dir,
        } => mem_area_ops::preview::write_preview(
            &inventory,
            &selection,
            &selection_sha256,
            &output_dir,
        ),
        Command::SchemaPlan => print(&mem_area_ops::schema::plan()?),
        Command::InitializeSchema {
            target,
            target_sha256,
            schema_sha256,
            execute,
            writers_stopped,
            fresh_database,
        } => print(
            &mem_area_ops::schema::initialize(&mem_area_ops::schema::Initialization {
                target,
                target_sha256,
                schema_sha256,
                execute,
                writers_stopped,
                fresh_database,
            })
            .await?,
        ),
        Command::VerifyPayload {
            inventory,
            payload,
            inventory_sha256,
        } => print(&mem_area_ops::verify_payload(
            &inventory,
            &payload,
            &inventory_sha256,
        )?),
        Command::Inspect {
            inventory,
            manifest,
        } => print(&mem_area_ops::inspect(&inventory, &manifest)?),
        Command::Prepare {
            source,
            retention,
            output_dir,
        } => print(&mem_area_ops::prepare_with_budget(
            &source.into(),
            &output_dir,
            retention.budget()?,
        )?),
        Command::Import {
            source,
            retention,
            prepared,
            prepared_sha256,
            target,
            target_sha256,
            operation_id,
            execute,
            writers_stopped,
            archive_backed_up,
        } => {
            let execution = Execution {
                prepared,
                prepared_sha256,
                target,
                target_sha256,
                operation_id,
                execute,
                writers_stopped,
                archive_backed_up,
            };
            print(
                &mem_area_ops::import_with_budget(&source.into(), &execution, retention.budget()?)
                    .await?,
            )
        }
    }
}
#[tokio::main]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            eprintln!("invalid migration arguments; use --help (supplied values suppressed)");
            return ExitCode::from(2);
        }
    };
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
