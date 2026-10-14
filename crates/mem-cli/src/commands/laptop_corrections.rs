// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit local staging: the owner reviews corrections after synchronization.
use super::api::ApiClient;
use anyhow::{Result, ensure};
use clap::Args;
use mem_record::LaptopCorrectionRequest;
use std::{io::Read, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Args)]
pub(in crate::commands) struct CorrectionArgs {
    #[arg(long)]
    subscription: Uuid,
    /// JSON containing a stable operation ID, selected version/digest and curated content.
    #[arg(long)]
    request: PathBuf,
    /// Confirm queueing the displayed correction for transfer and owner review.
    #[arg(long)]
    confirm: bool,
}
pub(super) async fn handle(args: CorrectionArgs, api: &ApiClient) -> Result<()> {
    let file = std::fs::File::open(&args.request)?;
    ensure!(
        file.metadata()?.is_file(),
        "correction request must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "correction request exceeds 64 KiB");
    let request: LaptopCorrectionRequest = serde_json::from_slice(&bytes)?;
    if !args.confirm {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"subscription_id":args.subscription,"request":request,"effect":"Queue a local correction for the owning instance to review. Does not overwrite downloaded knowledge.","confirmed":false})
            )?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string(
            &api.stage_laptop_correction(args.subscription, &request)
                .await?
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[tokio::test]
    async fn correction_cli_defaults_to_preview_and_uses_local_routing() {
        let cli = crate::commands::runtime::Cli::try_parse_from([
            "memory",
            "--area",
            "homelab",
            "projects",
            "propose-correction",
            "--subscription",
            "10000000-0000-0000-0000-000000000001",
            "--request",
            "correction.json",
        ])
        .unwrap();
        assert_eq!(cli.area.as_deref(), Some("homelab"));
        assert!(
            !super::super::auth_enrollment::handle_command(&cli.command)
                .await
                .unwrap()
        );
        let crate::commands::runtime::Command::Projects(projects) = cli.command else {
            panic!("expected project command")
        };
        let super::super::auth_enrollment::ProjectsCommand::ProposeCorrection(args) =
            projects.command
        else {
            panic!("expected correction")
        };
        assert!(!args.confirm);
        assert_eq!(args.request, PathBuf::from("correction.json"));
    }
}
