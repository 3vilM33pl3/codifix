// SPDX-License-Identifier: AGPL-3.0-or-later
use super::CompanyArgs;
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;
use uuid::Uuid;
#[cfg(feature = "project-sync")]
mod native;

#[derive(Debug, Args)]
pub(in crate::commands) struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}
#[derive(Debug, Subcommand)]
enum AgentCommand {
    /// Save a separate Keychain credential and print its public approval request.
    Prepare(Prepare),
    /// Check this independent credential and cache its exact approved receipt.
    Status(Status),
    /// Serve only this project's cache under the approved caller disclosure.
    Mcp(Mcp),
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Caller {
    Codex,
    Claude,
}
impl Caller {
    #[cfg(feature = "project-sync")]
    fn contract(self) -> mem_record::CompanyAgentCaller {
        match self {
            Self::Codex => mem_record::CompanyAgentCaller::Codex,
            Self::Claude => mem_record::CompanyAgentCaller::Claude,
        }
    }
}
#[derive(Debug, Args)]
struct Prepare {
    #[command(flatten)]
    company: CompanyArgs,
    #[arg(long)]
    caller: Caller,
    /// Reviewed project workspace journal; credentials are never written here.
    #[arg(long)]
    workspace_journal: PathBuf,
    /// Explicitly create a fresh request after the prior grant was denied or
    /// expired. Never clears its revocation or inherits its approval.
    #[arg(long)]
    new_request: bool,
}
#[derive(Debug, Args)]
struct Status {
    #[command(flatten)]
    company: CompanyArgs,
    #[arg(long)]
    agent_id: Uuid,
}
#[derive(Debug, Args)]
struct Mcp {
    #[command(flatten)]
    company: CompanyArgs,
    #[arg(long)]
    agent_id: Uuid,
    /// Exact private native replica profile, never a server database URL.
    #[arg(long)]
    profile: PathBuf,
    /// Permit already-cached disclosure until the explicit grant expires when
    /// the company is unreachable. Observed denial always stops disclosure.
    #[arg(long)]
    allow_offline: bool,
}
pub(super) async fn handle(args: &AgentArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        match &args.command {
            AgentCommand::Prepare(args) => native::prepare(args).await,
            AgentCommand::Status(args) => native::status(args).await,
            AgentCommand::Mcp(args) => native::mcp(args).await,
        }
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!("independent laptop agents require the native project-sync package");
    }
}
