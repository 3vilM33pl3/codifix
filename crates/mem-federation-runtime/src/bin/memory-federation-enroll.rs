// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit separate-authority membership operator; never a background service.
use mem_federation_runtime::enrollment::EnrollmentConfig;
use std::{path::PathBuf, time::Duration};

enum Command {
    Preview(PathBuf),
    Inspect(PathBuf),
    Execute(PathBuf, String),
    Help,
}
fn parse(args: Vec<String>) -> Result<Command, &'static str> {
    match args.as_slice() {
        [help] if matches!(help.as_str(), "--help" | "-h") => Ok(Command::Help),
        [config] if !config.starts_with('-') => Ok(Command::Preview(config.into())),
        [operation, config] if operation == "inspect" => Ok(Command::Inspect(config.into())),
        [operation, config, digest]
            if operation == "execute"
                && digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(Command::Execute(config.into(), digest.clone()))
        }
        _ => Err("Invalid enrollment arguments; use --help. No work started."),
    }
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(complete) => {
            if complete {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(2)
            }
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<bool, String> {
    let command = parse(std::env::args().skip(1).collect()).map_err(str::to_owned)?;
    if matches!(command, Command::Help) {
        println!(
            "memory-federation-enroll /PRIVATE/config.json\nmemory-federation-enroll inspect /PRIVATE/config.json\nmemory-federation-enroll execute /PRIVATE/config.json EXACT_REVIEW_SHA256\n\nDefault: offline exact membership preview, no token read or network call.\nInspect: HTTPS signed-history reads only. Execute: explicitly reviewed\nPull/Publish/AskExpertise/AnswerExpertise grants, never ManageMembership.\nUses a separate enrolled authority and pinned receipt keys; no DB or new keys.\nExit 2 means accepted but publication is not yet proven. After an ambiguous\nresponse inspect/retry this exact plan; never replace its stable record IDs."
        );
        return Ok(true);
    }
    let path = match &command {
        Command::Preview(p) | Command::Inspect(p) | Command::Execute(p, _) => p,
        Command::Help => unreachable!(),
    };
    let config = EnrollmentConfig::load(path).map_err(|e| e.to_string())?;
    let (output, complete) = match command {
        Command::Preview(_) => (
            serde_json::to_string_pretty(&config.preview().map_err(|e| e.to_string())?),
            true,
        ),
        Command::Inspect(_) => {
            let report = tokio::time::timeout(Duration::from_secs(120), config.inspect())
                .await
                .map_err(|_| "Gateway inspection timed out; no mutation attempted.".to_owned())?
                .map_err(|e| e.to_string())?;
            (serde_json::to_string_pretty(&report), true)
        }
        Command::Execute(_, digest) => {
            let report = tokio::time::timeout(Duration::from_secs(120), config.execute(&digest))
                .await
                .map_err(|_| {
                    "Enrollment outcome unconfirmed after timeout; inspect or retry only the exact reviewed plan."
                        .to_owned()
                })?
                .map_err(|e| e.to_string())?;
            let complete = report.status == "published_proven";
            (serde_json::to_string_pretty(&report), complete)
        }
        Command::Help => unreachable!(),
    };
    println!(
        "{}",
        output.map_err(|_| "Enrollment metadata serialization unavailable.".to_owned())?
    );
    Ok(complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_explicit_execution_accepts_a_digest() {
        assert!(matches!(
            parse(vec!["/private/config.json".into()]).expect("valid offline preview arguments"),
            Command::Preview(_)
        ));
        assert!(parse(vec!["execute".into(), "/private/config.json".into()]).is_err());
        assert!(
            parse(vec![
                "execute".into(),
                "/private/config.json".into(),
                "0".repeat(64)
            ])
            .is_ok()
        );
        assert!(
            parse(vec![
                "execute".into(),
                "/private/config.json".into(),
                "A".repeat(64)
            ])
            .is_err()
        );
        assert!(parse(vec!["--insecure".into()]).is_err());
        assert!(parse(vec!["/private/config.json".into(), "--yes".into()]).is_err());
    }
}
