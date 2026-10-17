// SPDX-License-Identifier: AGPL-3.0-or-later
use super::api::ApiClient;
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use codifix_record::JobStatus;
use uuid::Uuid;

pub(crate) const HELP: &str = "Agent notes:\nUse an explicit project. Listings and details contain sanitized metadata only. Retry requires --yes and a stable --request-id; retain that ID after an uncertain reply. Original inputs and completed stages are retained; current authorization and policy still apply. Provider calls may incur repeat billing. See docs/user/cli/jobs.md.";

#[derive(Debug, Args)]
pub(crate) struct JobsArgs {
    #[arg(long)]
    pub project: String,
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Option<JobsCommand>,
}
#[derive(Debug, Subcommand)]
pub(crate) enum JobsCommand {
    #[command(about = "List bounded, filtered project jobs.", after_help = HELP)]
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        after: Option<Uuid>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    #[command(about = "Inspect stages, dependencies and attempt history.", after_help = HELP)]
    Inspect { id: Uuid },
    #[command(about = "Explicitly retry a failed or blocked stage.", after_help = HELP)]
    Retry {
        id: Uuid,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        yes: bool,
    },
}
pub(crate) async fn handle(args: JobsArgs, api: &ApiClient) -> Result<()> {
    match args.command.unwrap_or(JobsCommand::List {
        status: None,
        after: None,
        limit: 50,
    }) {
        JobsCommand::List {
            status,
            after,
            limit,
        } => {
            let status = status
                .map(|s| serde_json::from_value::<JobStatus>(serde_json::Value::String(s)))
                .transpose()?;
            let response = api.jobs(&args.project, status, after, limit).await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
                return Ok(());
            }
            println!(
                "Background work for {} (queued={}, running={}, retry_wait={}, blocked={}, failed={})",
                args.project,
                response.stats.queued,
                response.stats.running,
                response.stats.retry_wait,
                response.stats.blocked,
                response.stats.failed
            );
            for job in response.page.jobs {
                println!(
                    "{}  {:?}  {:?}  generation={} attempts={} failure={:?} reason={:?}",
                    job.id,
                    job.kind,
                    job.status,
                    job.generation,
                    job.attempts,
                    job.last_failure,
                    job.reason
                );
            }
            if let Some(after) = response.page.next_after {
                println!(
                    "Next page: codifix jobs --project {} list --after {after}",
                    args.project
                );
            }
        }
        JobsCommand::Inspect { id } => {
            let detail = api.job(&args.project, id).await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&detail)?);
                return Ok(());
            }
            println!(
                "{} {:?}: {:?}, generation {}, attempts {}",
                id, detail.job.kind, detail.job.status, detail.job.generation, detail.job.attempts
            );
            println!(
                "Failure: {:?}; reason: {:?}; next eligible: {}",
                detail.job.last_failure, detail.job.reason, detail.job.available_at
            );
            println!("Dependencies: {:?}", detail.dependencies);
            for attempt in detail.history {
                println!(
                    "generation={} attempt={} outcome={:?} failure={:?} uncertain={}",
                    attempt.generation,
                    attempt.attempt,
                    attempt.outcome,
                    attempt.failure,
                    attempt.uncertain
                );
            }
            if detail.history_truncated {
                println!("Older attempt history omitted from this bounded view.");
            }
            println!("Stages:");
            for stage in detail.stages {
                println!("  {} {:?} {:?}", stage.id, stage.kind, stage.status);
            }
        }
        JobsCommand::Retry {
            id,
            request_id,
            yes,
        } => {
            if !yes {
                bail!(
                    "Retry {id} in project {} with request ID {request_id} requires --yes. Inputs and completed stages are retained; provider calls may incur repeat billing.",
                    args.project
                );
            }
            let response = api.retry_job(&args.project, id, request_id).await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Retry accepted: job {id}, generation {}, request {request_id}",
                    response.generation
                );
            }
        }
    }
    Ok(())
}
