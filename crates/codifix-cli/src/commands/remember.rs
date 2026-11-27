// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use codifix_config::AppConfig;
use reqwest::Client;
use std::env;

use crate::{
    commands::{
        api::ApiClient,
        init_support::repo_replacement_policy,
        memory_ops::{build_remember_request, resolve_project_slug},
        runtime::RememberArgs,
        skill_support::resolve_repo_root,
    },
    writer_identity::resolve_writer_identity,
};

pub(super) async fn handle(
    args: RememberArgs,
    client: Client,
    config: AppConfig,
    cli_writer_id: Option<String>,
) -> Result<()> {
    let cwd = env::current_dir().context("read current directory")?;
    let repo_root = resolve_repo_root(&cwd)?;
    let project = resolve_project_slug(args.project.clone(), &cwd)?;
    let writer = resolve_writer_identity(&config, cli_writer_id.as_deref())?;
    let dry_run = args.dry_run;
    let verification = (!args.tests_passed.is_empty() && args.tests_failed.is_empty()).then(|| {
        format!(
            "Codifix recorded {} passing verification checks.",
            args.tests_passed.len()
        )
    });
    let thread_key = args.linear_thread_key.clone();
    let mut request = build_remember_request(args, &project, &writer.id, writer.name.as_deref())?;
    request.dry_run = dry_run;
    let api = ApiClient::new(client, config);
    let capture = api.capture_task(&request).await?;
    let curate = if dry_run {
        api.curate(&project, repo_replacement_policy(&repo_root), true)
            .await?
    } else {
        api.curate_capture(
            &project,
            capture.raw_capture_id,
            repo_replacement_policy(&repo_root),
            false,
        )
        .await?
    };
    if !dry_run
        && !capture.queued_offline
        && let Some(verification) = verification
        && request.structured_candidates.iter().any(|candidate| {
            matches!(
                candidate.memory_type,
                codifix_record::MemoryType::Implementation | codifix_record::MemoryType::Refactor
            )
        })
    {
        let thread_key = crate::plan_execution::derive_plan_thread_key(
            thread_key.as_deref(),
            &request.task_title,
            &project,
        );
        crate::linear_tracking::report_enqueue(crate::linear_tracking::enqueue(
            &api.config,
            &project,
            Some(capture.project_id),
            &thread_key,
            &request.task_title,
            crate::linear_tracking::Lifecycle::Completed,
            Some(verification),
        ));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "capture": capture,
            "curate": curate,
            "dry_run": dry_run,
        }))?
    );

    Ok(())
}
