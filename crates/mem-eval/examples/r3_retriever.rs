// SPDX-License-Identifier: AGPL-3.0-or-later
// Example external-retriever executable; configuration must be a locked asset.
use anyhow::{Result, ensure};
use mem_eval::campaign::retriever::{Configuration, Request, retrieve};
use std::io::{Read, Write};

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(args.len() == 1, "one prepared configuration required");
    let file = std::fs::File::open(&args[0])?;
    let mut configuration = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut configuration)?;
    ensure!(
        configuration.len() <= 16 * 1024 * 1024,
        "oversized configuration"
    );
    let config: Configuration = serde_json::from_slice(&configuration)?;
    let mut input = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut input)?;
    ensure!(input.len() <= 1024 * 1024, "oversized request");
    let request: Request = serde_json::from_slice(&input)?;
    let response = retrieve(&config, &request)?;
    std::io::stdout().write_all(&serde_json::to_vec(&response)?)?;
    Ok(())
}

fn main() {
    if run().is_err() {
        // Never echo query, configuration contents, paths or parser input.
        eprintln!("R3 retriever rejected request or configuration");
        std::process::exit(1);
    }
}
