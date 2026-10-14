// SPDX-License-Identifier: AGPL-3.0-or-later
use super::CompanyArgs;
use anyhow::Result;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;
use uuid::Uuid;
#[cfg(feature = "project-sync")]
mod native;
#[cfg(feature = "project-sync")]
mod settings;

#[derive(Debug, Args)]
pub(in crate::commands) struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}
#[derive(Debug, Subcommand)]
enum AgentCommand {
    /// Save a separate Keychain credential and print its public approval request.
    #[command(after_help = super::HELP)]
    Prepare(Prepare),
    /// Check this independent credential and cache its exact approved receipt.
    #[command(after_help = super::HELP)]
    Status(Status),
    /// Serve only this project's cache under the approved caller disclosure.
    #[command(after_help = super::HELP)]
    Mcp(Mcp),
    /// Preview or explicitly apply only this project's secret-free MCP entry.
    #[command(after_help = super::HELP)]
    Configure(Configure),
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
    /// Exact corpus approved for this agent; changing it needs a new identity.
    #[arg(long, default_value = "shared_publications_v1", value_parser = ["shared_publications_v1", "curated_project_v1"])]
    replication_mode: String,
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
    /// Open only the public approval JSON on the pinned company browser origin.
    #[arg(long)]
    open_browser: bool,
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
#[derive(Debug, Args)]
struct Configure {
    #[command(flatten)]
    company: CompanyArgs,
    #[arg(long)]
    agent_id: Uuid,
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    workspace_journal: PathBuf,
    /// Permit future cached disclosure only under the explicit bounded receipt.
    #[arg(long)]
    allow_offline: bool,
    /// Exact digest printed by the preview; required together with --confirm.
    #[arg(long, requires = "confirm")]
    plan_sha256: Option<String>,
    #[arg(long, requires = "plan_sha256")]
    confirm: bool,
}
pub(super) async fn handle(args: &AgentArgs) -> Result<()> {
    #[cfg(feature = "project-sync")]
    {
        match &args.command {
            AgentCommand::Prepare(args) => native::prepare(args).await,
            AgentCommand::Status(args) => native::status(args).await,
            AgentCommand::Mcp(args) => native::mcp(args).await,
            AgentCommand::Configure(args) => native::configure(args).await,
        }
    }
    #[cfg(not(feature = "project-sync"))]
    {
        let _ = args;
        anyhow::bail!("independent laptop agents require the native project-sync package");
    }
}
