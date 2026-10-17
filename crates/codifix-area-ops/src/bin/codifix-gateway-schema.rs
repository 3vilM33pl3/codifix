// SPDX-License-Identifier: AGPL-3.0-or-later
//! Separate schema-owner operation. Never starts a Memory app or gateway.
use clap::{Parser, Subcommand};
use codifix_area_ops::{Failure, schema::gateway};
use std::{io::Write, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(about = "Fresh dedicated gateway schema only; default is offline review")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Review the exact private target and executable schema; no DB connection.
    Review { target: PathBuf },
    /// Execute the exact reviewed fresh schema; no roles, grants or enrollment.
    Execute {
        target: PathBuf,
        review_sha256: String,
        #[arg(long)]
        writers_stopped: bool,
        #[arg(long)]
        fresh_database: bool,
    },
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let report = match cli.command {
        Command::Review { target } => gateway::preview(&target)?,
        Command::Execute {
            target,
            review_sha256,
            writers_stopped,
            fresh_database,
        } => gateway::initialize(&target, &review_sha256, writers_stopped, fresh_database).await?,
    };
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &report).map_err(|_| Failure::Output)?;
    writeln!(stdout).map_err(|_| Failure::Output)
}

#[tokio::main]
async fn main() -> ExitCode {
    // A single non-option path is the default offline review, never execution.
    let mut arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() == 2
        && !["--help", "-h", "--version", "-V", "review", "execute"]
            .iter()
            .any(|value| arguments[1] == *value)
    {
        arguments.insert(1, "review".into());
    }
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return ExitCode::SUCCESS;
            }
            eprintln!("invalid gateway schema arguments; use --help (supplied values suppressed)");
            return ExitCode::from(2);
        }
    };
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
