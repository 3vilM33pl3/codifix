// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{Context, Result};
use codifix_config::{AppConfig, Profile};
use codifix_watch::{WatcherRunArgs, run_watcher_daemon};
use std::{env, path::PathBuf};

use crate::{
    commands::{
        api::ApiClient,
        memory_ops::{preview_automation_flush, resolve_project_slug},
        runtime::{WatcherArgs, WatcherCommand, WatcherManagerCommand, default_global_config_path},
        skill_support::resolve_repo_root,
        watch_support::{
            ManualWatcherTarget, disable_watch_manager_service, disable_watch_service_routed,
            enable_watch_manager_service_routed, enable_watch_service_routed,
            preview_disable_watch_manager_service, preview_disable_watch_service_routed,
            preview_enable_watch_manager_service, preview_enable_watch_service_routed,
            run_watcher_manager, watch_manager_service_status, watch_service_status_routed,
            watcher_command_requires_config_load,
        },
    },
    writer_identity::{resolve_writer_identity, resolve_writer_identity_for_tool},
};
use codifix_watch::{area_flush_path, run_once};

pub(super) async fn handle_pre_config(
    args: &WatcherArgs,
    cli_config: Option<PathBuf>,
    routing_path: Option<&std::path::Path>,
    route: Option<&codifix_config::ResolvedArea>,
) -> Result<bool> {
    let cwd = env::current_dir().context("read current directory")?;
    let repo_root = resolve_repo_root(&cwd)?;
    // Routing has already been validated by the dispatch boundary. An explicit
    // Windows config without a routing file can still carry a named area.
    // Resolve relative paths before the scheduled task changes its directory.
    #[cfg(windows)]
    let cli_config = if matches!(
        args.command,
        WatcherCommand::Enable(_) | WatcherCommand::Disable(_) | WatcherCommand::Status(_)
    ) {
        cli_config
            .map(|path| {
                path.canonicalize()
                    .context("resolve selected watcher config; no fallback attempted")
            })
            .transpose()?
    } else {
        cli_config
    };
    #[cfg(windows)]
    let explicit_area = if route.is_none()
        && matches!(
            args.command,
            WatcherCommand::Enable(_) | WatcherCommand::Disable(_) | WatcherCommand::Status(_)
        ) {
        cli_config
            .as_ref()
            .map(|path| {
                AppConfig::load_from_path(Some(path.clone()))
                    .context("load selected watcher config; no fallback attempted")
            })
            .transpose()?
            .and_then(|config| config.area)
    } else {
        None
    };
    #[cfg(not(windows))]
    let explicit_area = None;
    let target = ManualWatcherTarget {
        config_path: cli_config.as_deref(),
        routing_path,
        area: route
            .map(|route| &route.identity)
            .or(explicit_area.as_ref()),
    };
    match &args.command {
        WatcherCommand::Run(_) | WatcherCommand::Flush(_) => {}
        WatcherCommand::Manager(args) => match &args.command {
            WatcherManagerCommand::Run => {}
            WatcherManagerCommand::Enable(args) => {
                let output = if args.dry_run {
                    preview_enable_watch_manager_service()?
                } else {
                    enable_watch_manager_service_routed(
                        &cli_config
                            .clone()
                            .unwrap_or_else(default_global_config_path),
                        routing_path,
                    )?
                };
                println!("{output}");
                return Ok(false);
            }
            WatcherManagerCommand::Disable(args) => {
                let output = if args.dry_run {
                    preview_disable_watch_manager_service()?
                } else {
                    disable_watch_manager_service(Profile::detect())?
                };
                println!("{output}");
                return Ok(false);
            }
            WatcherManagerCommand::Status => {
                println!("{}", watch_manager_service_status(Profile::detect())?);
                return Ok(false);
            }
        },
        WatcherCommand::Enable(args) => {
            let project = resolve_project_slug(args.project.clone(), &cwd)?;
            let output = if args.dry_run {
                preview_enable_watch_service_routed(&repo_root, &project, &target)?
            } else {
                enable_watch_service_routed(&repo_root, &project, &target)?
            };
            println!("{output}");
        }
        WatcherCommand::Disable(args) => {
            let project = resolve_project_slug(args.project.clone(), &cwd)?;
            let output = if args.dry_run {
                preview_disable_watch_service_routed(&project, &target)?
            } else {
                disable_watch_service_routed(&project, &target)?
            };
            println!("{output}");
        }
        WatcherCommand::Status(args) => {
            let project = resolve_project_slug(args.project.clone(), &cwd)?;
            let output = watch_service_status_routed(&repo_root, &project, &target)?;
            println!("{output}");
        }
    }
    Ok(watcher_command_requires_config_load(&args.command))
}

pub(super) async fn handle(
    args: WatcherArgs,
    config: AppConfig,
    cli_config_path: Option<PathBuf>,
    cli_writer_id: Option<String>,
) -> Result<()> {
    match args.command {
        WatcherCommand::Run(args) => {
            let writer = resolve_writer_identity_for_tool(
                &config,
                cli_writer_id.as_deref(),
                "memory-watcher",
            )?;
            run_watcher_daemon(
                config,
                WatcherRunArgs {
                    project: args.project,
                    repo_root: args.repo_root,
                    agent_cli: args.agent_cli,
                    agent_session_id: args.agent_session_id,
                    agent_pid: args.agent_pid,
                    agent_started_at: args.agent_started_at,
                    service_managed: args.service_managed,
                },
                writer.id,
                writer.name,
            )
            .await?;
        }
        WatcherCommand::Manager(args) => match args.command {
            WatcherManagerCommand::Run => run_watcher_manager(config, cli_config_path).await?,
            WatcherManagerCommand::Enable(_)
            | WatcherManagerCommand::Disable(_)
            | WatcherManagerCommand::Status => {
                unreachable!("watcher manager lifecycle commands are handled before config loading")
            }
        },
        WatcherCommand::Flush(args) => {
            let cwd = env::current_dir().context("read current directory")?;
            let project = resolve_project_slug(Some(args.project.project), &cwd)?;
            let repo_root = config
                .automation
                .repo_root
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or(cwd);
            let api = ApiClient::new(reqwest::Client::new(), config.clone());
            let writer = resolve_writer_identity(&config, cli_writer_id.as_deref())?;
            if args.dry_run {
                let preview = preview_automation_flush(
                    &api.config,
                    &api.client,
                    &project,
                    &repo_root,
                    args.curate,
                    &writer.id,
                    writer.name.as_deref(),
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&preview)?);
                return Ok(());
            }
            tokio::fs::write(area_flush_path(&config.automation, &repo_root), b"flush\n")
                .await
                .ok();
            run_once(
                &api.config,
                &api.client,
                &project,
                &repo_root,
                true,
                args.curate,
                &writer.id,
                writer.name.as_deref(),
            )
            .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "project": project,
                    "status": "flush_requested",
                    "curate": args.curate
                }))?
            );
        }
        WatcherCommand::Enable(_) | WatcherCommand::Disable(_) | WatcherCommand::Status(_) => {
            unreachable!("watcher lifecycle commands are handled before config loading")
        }
    };
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::commands::runtime::{Cli, Command};
    use clap::Parser;

    #[tokio::test]
    async fn manual_watcher_unavailable_explicit_config_fails_before_task_action() {
        let missing = env::temp_dir().join(format!(
            "missing-manual-watch-config-{}.toml",
            uuid::Uuid::new_v4()
        ));
        for action in ["enable", "disable", "status"] {
            let cli =
                Cli::try_parse_from(["memory", "watcher", action, "--project", "shared-project"])
                    .unwrap();
            let Command::Watcher(args) = cli.command else {
                panic!("watcher command expected")
            };
            let error = handle_pre_config(&args, Some(missing.clone()), None, None)
                .await
                .unwrap_err();
            assert!(error.to_string().contains("no fallback attempted"));
        }
    }
}
