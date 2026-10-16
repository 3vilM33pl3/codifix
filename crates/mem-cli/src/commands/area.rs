// SPDX-License-Identifier: AGPL-3.0-or-later

use super::runtime::{Command, ServiceArgs, ServiceCommand, WatcherArgs, WatcherCommand};
use anyhow::{Context, Result, bail};
use mem_config::{AreaRouting, Profile, ResolvedArea, discover_area_routing_path, normalized_path};
use std::path::{Path, PathBuf};

/// Read clap's already validated arguments (including nested subcommands),
/// rather than interpreting shell arguments or duplicating every command enum.
pub(super) fn matched_string(matches: &clap::ArgMatches, name: &str) -> Option<String> {
    matches
        .try_get_one::<String>(name)
        .ok()
        .flatten()
        .cloned()
        .or_else(|| {
            matches
                .subcommand()
                .and_then(|(_, child)| matched_string(child, name))
        })
}

pub(super) fn matched_path(matches: &clap::ArgMatches, name: &str) -> Option<PathBuf> {
    matches
        .try_get_one::<PathBuf>(name)
        .ok()
        .flatten()
        .cloned()
        .or_else(|| {
            matches
                .subcommand()
                .and_then(|(_, child)| matched_path(child, name))
        })
}

pub(super) fn resolve_cli_area(
    command: &Command,
    config: Option<&Path>,
    explicit: Option<&str>,
    routing: Option<PathBuf>,
    repository: Option<&Path>,
    project: Option<&str>,
    profile: Profile,
) -> Result<(Option<PathBuf>, Option<ResolvedArea>)> {
    let path = discover_area_routing_path(routing, profile);
    // Installed daemon configs remain operator-pinned. The manager resolves
    // each child separately. Repo setup and package maintenance do not select
    // or access knowledge. Explicit --area still validates these requests.
    let pinned = matches!(
        command,
        Command::Service(ServiceArgs {
            command: ServiceCommand::Run
        }) | Command::Watcher(WatcherArgs {
            command: WatcherCommand::Manager(_)
        }) | Command::Init(_)
            | Command::Upgrade(_)
            | Command::Completion(_)
            | Command::Dev(_)
            | Command::Wizard(_)
    );
    if pinned && explicit.is_none() {
        return Ok((path, None));
    }
    let Some(path) = path else {
        if explicit.is_some() {
            bail!("--area requires an operator area-routing.json (or --area-routing)");
        }
        return Ok((None, None));
    };
    if !cfg!(windows)
        && matches!(
            command,
            Command::Watcher(WatcherArgs {
                command: WatcherCommand::Enable(_)
            })
        )
    {
        bail!(
            "area routing requires the watcher manager; legacy watcher enable creates an unscoped singleton"
        );
    }
    if matches!(
        command,
        Command::Service(ServiceArgs {
            command: ServiceCommand::Enable(_) | ServiceCommand::Disable(_)
        })
    ) {
        bail!(
            "use the named-instance upgrade operator for area service lifecycle; this command manages the legacy singleton"
        );
    }
    let route = AreaRouting::read(&path)?.resolve(explicit, repository, project)?;
    if let Some(config) = config
        && normalized_path(config) != normalized_path(&route.config)
    {
        bail!(
            "--config conflicts with selected area {}; select the matching --area/config instead of bypassing routing",
            route.identity.name
        );
    }
    route
        .load(profile)
        .context("validate selected area config")?;
    Ok((Some(path), Some(route)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::runtime::Cli;
    use clap::{CommandFactory, FromArgMatches};

    #[test]
    fn global_area_and_nested_project_and_repo_are_parsed_without_guessing() {
        let matches = Cli::command()
            .try_get_matches_from([
                "memory",
                "watcher",
                "run",
                "--area",
                "homelab",
                "--project",
                "shared-name",
                "--repo-root",
                "/repo/infra",
                "--area-routing",
                "/operator/routes.json",
            ])
            .unwrap();
        assert_eq!(
            matched_string(&matches, "project").as_deref(),
            Some("shared-name")
        );
        assert_eq!(
            matched_path(&matches, "repo_root"),
            Some(PathBuf::from("/repo/infra"))
        );
        let cli = Cli::from_arg_matches(&matches).unwrap();
        assert_eq!(cli.area.as_deref(), Some("homelab"));
        assert_eq!(
            cli.area_routing,
            Some(PathBuf::from("/operator/routes.json"))
        );
    }

    #[test]
    fn explicitly_missing_policy_fails_before_a_knowledge_command() {
        let matches = Cli::command()
            .try_get_matches_from([
                "memory",
                "query",
                "--project",
                "memory",
                "--question",
                "example",
            ])
            .unwrap();
        let command = Cli::from_arg_matches(&matches).unwrap().command;
        let missing =
            std::env::temp_dir().join(format!("missing-routes-{}.json", uuid::Uuid::new_v4()));
        assert!(
            resolve_cli_area(
                &command,
                None,
                None,
                Some(missing),
                None,
                Some("memory"),
                Profile::Prod
            )
            .is_err()
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn routed_legacy_watch_enable_is_rejected_before_a_side_effect() {
        let matches = Cli::command()
            .try_get_matches_from(["memory", "watcher", "enable", "--project", "memory"])
            .unwrap();
        let command = Cli::from_arg_matches(&matches).unwrap().command;
        let error = resolve_cli_area(
            &command,
            None,
            None,
            Some(PathBuf::from("/operator/routes.json")),
            None,
            Some("memory"),
            Profile::Prod,
        )
        .unwrap_err();
        assert!(error.to_string().contains("watcher manager"));
    }

    #[cfg(windows)]
    #[test]
    fn routed_manual_watch_enable_requires_available_operator_policy_on_windows() {
        let matches = Cli::command()
            .try_get_matches_from([
                "memory",
                "watcher",
                "enable",
                "--project",
                "shared-project",
                "--dry-run",
            ])
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        let missing = std::env::temp_dir().join(format!(
            "missing-manual-watch-policy-{}.json",
            uuid::Uuid::new_v4()
        ));
        let error = resolve_cli_area(
            &cli.command,
            None,
            Some("homelab"),
            Some(missing),
            None,
            Some("shared-project"),
            Profile::Prod,
        )
        .unwrap_err();
        assert!(!error.to_string().contains("legacy watcher enable"));
        assert!(error.to_string().contains("area routing"));
    }
}
