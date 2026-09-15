// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private trusted-host JSONL bridge to the existing campaign budget/result
//! store. No network, provider credentials, payment approval or agent API.
use anyhow::{Result, ensure};
use mem_eval::campaign::{
    Limits,
    budget::{Allowance, RequestBudget},
    provider_results::ProviderResults,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    version: u8,
    manifest_sha256: String,
    limits: Limits,
    task_limits: Limits,
    resume: bool,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Begin {
        task_id: String,
        request_id: String,
        payload_sha256: String,
        allowance: Allowance,
    },
    Complete {
        task_id: String,
        request_id: String,
        payload_sha256: String,
        output: Value,
        usage: Allowance,
    },
    Recover {
        request_id: String,
    },
    Status,
    Audit,
    Shutdown,
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "private configuration and directory required"
    );
    let bytes = std::fs::read(&args[0])?;
    ensure!(bytes.len() <= 32768, "oversized configuration");
    let config: Configuration = serde_json::from_slice(&bytes)?;
    ensure!(config.version == 1, "invalid configuration");
    let root = PathBuf::from(&args[1]);
    let metadata = std::fs::symlink_metadata(&root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "private root required"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "private root required"
        );
    }
    let (mut budget, results) = if config.resume {
        (
            RequestBudget::resume_scoped(
                &root.join("budget.jsonl"),
                &config.manifest_sha256,
                config.limits,
                config.task_limits,
            )?,
            ProviderResults::open(&root.join("results"))?,
        )
    } else {
        (
            RequestBudget::create_scoped(
                &root.join("budget.jsonl"),
                &config.manifest_sha256,
                config.limits,
                config.task_limits,
            )?,
            ProviderResults::create(&root.join("results"))?,
        )
    };
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    loop {
        let mut bytes = Vec::new();
        let size =
            std::io::Read::take(&mut input, 2 * 1024 * 1024 + 1).read_until(b'\n', &mut bytes)?;
        if size == 0 {
            return Ok(());
        }
        ensure!(
            size <= 2 * 1024 * 1024 && bytes.last() == Some(&b'\n'),
            "invalid frame"
        );
        let request: Request = serde_json::from_slice(&bytes)?;
        if matches!(request, Request::Shutdown) {
            return Ok(());
        }
        let response = (|| -> Result<Value> {
            Ok(match request {
                Request::Begin {
                    task_id,
                    request_id,
                    payload_sha256,
                    allowance,
                } => serde_json::to_value(results.begin(
                    &mut budget,
                    &task_id,
                    &request_id,
                    &payload_sha256,
                    allowance,
                )?)?,
                Request::Complete {
                    task_id,
                    request_id,
                    payload_sha256,
                    output,
                    usage,
                } => json!({"output":
                results.complete_reserved(&mut budget, &task_id, &request_id, &payload_sha256, output, usage)?, "usage": usage}),
                Request::Recover { request_id } => {
                    json!({"output":results.recover(&mut budget, &request_id)?})
                }
                Request::Status => {
                    json!({"ledger":budget.ledger(),"uncertain_ids":budget.uncertain_ids()})
                }
                Request::Audit => {
                    let audit = results.audit(&mut budget)?;
                    json!({"sha256":mem_eval::campaign::digest_value(&audit)?,"audit":audit})
                }
                Request::Shutdown => unreachable!(),
            })
        })();
        // A rejected reservation is not an invocation. Keep the session usable
        // for status/recovery; never echo parser, source or provider error bodies.
        let response = match response {
            Ok(value) => json!({"version":1,"ok":true,"result":value}),
            Err(_) => json!({"version":1,"ok":false,"error":"budget or receipt rejected"}),
        };
        let bytes = serde_json::to_vec(&response)?;
        ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "oversized private response"
        );
        output.write_all(&bytes)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}

fn main() {
    if run().is_err() {
        eprintln!("R3 private budget session failed; retain state for reconciliation");
        std::process::exit(1);
    }
}
