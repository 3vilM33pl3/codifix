// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit private descriptor, never a shell command or provider client.
#[cfg(not(windows))]
fn main() {
    eprintln!("native Windows evaluation only");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    use std::path::Path;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 6
        && args[0] == "--bootstrap-prepare"
        && args[2] == "--sha256"
        && args[4] == "--output"
    {
        let outcome = mem_platform::windows_evaluation_helper::bootstrap::run(
            Path::new(&args[1]),
            args[3].to_str().unwrap_or_default(),
            Path::new(&args[5]),
        );
        if let Ok(receipt) = outcome
            && let Ok(json) = serde_json::to_string(&receipt)
        {
            println!("{json}");
            return;
        }
        eprintln!(
            "native bootstrap preparation failed; retain its private output and do not automatically repeat"
        );
        std::process::exit(1);
    }
    if args.len() == 6 && args[0] == "--host-journal" && args[4] == "--caller-pid" {
        use mem_platform::windows_evaluation_helper::{host_journal, verify_outer_job};
        use std::io::Read;
        let outcome = (|| -> anyhow::Result<String> {
            let pid = args[5]
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
                .ok_or_else(|| anyhow::anyhow!("explicit caller required"))?;
            verify_outer_job(pid)?;
            let operation = args[1]
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid operation"))?;
            let name = args[3]
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("invalid name"))?;
            let mut bytes = Vec::new();
            if operation == "write" {
                std::io::stdin()
                    .take(host_journal::LIMIT + 1)
                    .read_to_end(&mut bytes)?;
            }
            let receipt = host_journal::run(operation, Path::new(&args[2]), name, &bytes)?;
            Ok(serde_json::to_string(&receipt)?)
        })();
        match outcome {
            Ok(json) => println!("{json}"),
            Err(_) => {
                eprintln!("protected host journal operation failed; retain private evidence");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.len() == 2 && args[0] == "--verify-outer-job" {
        if let Some(pid) = args[1].to_str().and_then(|value| value.parse::<u32>().ok())
            && let Ok(receipt) = mem_platform::windows_evaluation_helper::verify_outer_job(pid)
            && let Ok(json) = serde_json::to_string(&receipt)
        {
            println!("{json}");
            return;
        }
        eprintln!("native trusted outer Job verification failed; no payload should be written");
        std::process::exit(1);
    }
    if args.len() == 4 && args[0] == "--prepare-io" && args[2] == "--request-id" {
        let id = args[3]
            .to_str()
            .and_then(|value| uuid::Uuid::parse_str(value).ok());
        if let Some(id) = id
            && let Ok(receipt) =
                mem_platform::windows_evaluation_helper::prepare_io(Path::new(&args[1]), id)
            && let Ok(json) = serde_json::to_string(&receipt)
        {
            println!("{json}");
            return;
        }
        eprintln!("native private I/O preparation failed; no payload should be written");
        std::process::exit(1);
    }
    if args.len() != 6
        || args[0] != "--descriptor"
        || args[2] != "--sha256"
        || args[4] != "--output"
    {
        eprintln!(
            "expected --descriptor PRIVATE_JSON --sha256 DIGEST --output NEW_PRIVATE_DIRECTORY"
        );
        std::process::exit(1);
    }
    let hash = args[3].to_str().unwrap_or_default();
    match mem_platform::windows_evaluation_helper::run(
        Path::new(&args[1]),
        hash,
        Path::new(&args[5]),
    ) {
        Ok(receipt) => {
            match serde_json::to_string(&receipt) {
                Ok(json) => println!("{json}"),
                Err(_) => {
                    eprintln!("could not serialize native evaluation receipt");
                    std::process::exit(1);
                }
            }
            if receipt.state != "succeeded" {
                std::process::exit(1);
            }
        }
        Err(_) => {
            eprintln!("native evaluation helper failed; inspect the private output directory");
            std::process::exit(1);
        }
    }
}
