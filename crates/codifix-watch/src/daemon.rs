// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

use crate::{
    WatcherAgentOwner, build_watcher_heartbeat_request, build_watcher_unregister_request,
    detect_hostname, fetch_service_instance_id, heartbeat_watcher, load_state,
    owner_session_is_alive, path_is_ignored, run_once, unregister_watcher,
};
use anyhow::{Context, Result};
use clap::Args;
use codifix_config::{AppConfig, read_repo_project_slug};
#[cfg(test)]
use codifix_platform as platform;
use reqwest::Client;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeartbeatState {
    Unknown,
    Healthy,
    Failing,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BackendInstanceState {
    current: Option<String>,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub repo_root: Option<PathBuf>,
    #[arg(long)]
    pub agent_cli: Option<String>,
    #[arg(long)]
    pub agent_session_id: Option<String>,
    #[arg(long)]
    pub agent_pid: Option<u32>,
    #[arg(long)]
    pub agent_started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[arg(long, hide = true)]
    pub service_managed: bool,
}

pub async fn run_loop(
    config: AppConfig,
    args: RunArgs,
    writer_id: String,
    writer_name: Option<String>,
) -> Result<()> {
    anyhow::ensure!(
        !config.evaluation.active(),
        "evaluation prohibits watcher execution"
    );
    let repo_root = resolve_repo_root(&config, args.repo_root)?;
    let project = resolve_project(args.project, &repo_root)?;
    let _owner_lock = acquire_watcher_lock(&config.automation, &repo_root)?;
    let client = Client::new();
    let state = load_state(&project, &repo_root, &config.automation).await?;
    let watcher_id = config
        .area
        .as_ref()
        .map(|area| format!("{}-{}", area.key(), Uuid::new_v4()))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let hostname = detect_hostname();
    let host_service_id = config.cluster.service_id.clone();
    let managed_by_service = args.service_managed || watcher_is_service_managed();
    let pid = std::process::id();
    let started_at = chrono::Utc::now();
    let owner = WatcherAgentOwner {
        agent_cli: args.agent_cli.clone(),
        agent_session_id: args.agent_session_id.clone(),
        agent_pid: args.agent_pid,
        agent_started_at: args.agent_started_at,
    };

    let heartbeat_request = build_watcher_heartbeat_request(
        &state,
        &watcher_id,
        &hostname,
        &host_service_id,
        managed_by_service,
        pid,
        started_at,
        &owner,
    );
    let mut heartbeat_state = HeartbeatState::Unknown;
    let mut backend_instance = BackendInstanceState {
        current: fetch_service_instance_id(&client, &config)
            .await
            .ok()
            .flatten(),
    };
    heartbeat_state = log_heartbeat_transition(
        heartbeat_state,
        heartbeat_watcher(&client, &config, &heartbeat_request).await,
    );

    let mut repo_event_source = if config.automation.file_events {
        match start_repo_event_source(&repo_root, &config.automation.ignored_paths) {
            Ok(rx) => Some(rx),
            Err(error) => {
                eprintln!("watcher file events unavailable; falling back to polling: {error}");
                None
            }
        }
    } else {
        None
    };
    let mut debounce: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;
    let mut fallback_poll = tokio::time::interval(config.automation.poll_interval);
    fallback_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    fallback_poll.tick().await;
    let mut liveness = tokio::time::interval(std::time::Duration::from_secs(30));
    liveness.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    liveness.tick().await;
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                let request = build_watcher_unregister_request(&project, &watcher_id);
                if let Err(error) = unregister_watcher(&client, &config, &request).await {
                    eprintln!("watcher unregister failed: {error}");
                }
                break;
            }
            _ = liveness.tick() => {
                if !owner_session_is_alive(&owner) {
                    let request = build_watcher_unregister_request(&project, &watcher_id);
                    if let Err(error) = unregister_watcher(&client, &config, &request).await {
                        eprintln!("watcher unregister failed after owner exit: {error}");
                    }
                    println!("watcher owner session is gone; exiting");
                    break;
                }
            }
            _ = heartbeat.tick() => {
                let state = load_state(&project, &repo_root, &config.automation).await?;
                let request = build_watcher_heartbeat_request(
                    &state,
                    &watcher_id,
                    &hostname,
                    &host_service_id,
                    managed_by_service,
                    pid,
                    started_at,
                    &owner,
                );
                heartbeat_state = log_heartbeat_transition(
                    heartbeat_state,
                    heartbeat_watcher(&client, &config, &request).await,
                );
                update_backend_instance_state(
                    &mut backend_instance,
                    fetch_service_instance_id(&client, &config).await?,
                )?;
            }
            _ = fallback_poll.tick(), if repo_event_source.is_none() => {
                run_once(
                    &config,
                    &client,
                    &project,
                    &repo_root,
                    false,
                    false,
                    &writer_id,
                    writer_name.as_deref(),
                ).await?;
            }
            _ = async {
                if let Some(delay) = debounce.as_mut() {
                    delay.as_mut().await;
                }
            }, if debounce.is_some() => {
                debounce = None;
                refresh_repo_event_source(&mut repo_event_source);
                run_once(
                    &config,
                    &client,
                    &project,
                    &repo_root,
                    false,
                    false,
                    &writer_id,
                    writer_name.as_deref(),
                ).await?;
            }
            Some(_) = async {
                match repo_event_source.as_mut() {
                    Some(source) => source.rx.recv().await,
                    None => std::future::pending().await,
                }
            }, if repo_event_source.is_some() => {
                debounce = Some(Box::pin(tokio::time::sleep(std::time::Duration::from_secs(2))));
            }
        }
    }
    Ok(())
}

/// Keep the file descriptor for the lifetime of the daemon. Kernel locks are
/// released on crash and restart, and retaining the inode avoids unlink races.
fn acquire_watcher_lock(
    config: &codifix_config::AutomationConfig,
    repo: &Path,
) -> Result<std::fs::File> {
    let path = crate::state_path(config, repo).with_extension("watcher.lock");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    file.try_lock().map_err(|error| {
        anyhow::anyhow!(
            "watcher already owns this area/repository, or its lock is unavailable: {error}"
        )
    })?;
    Ok(file)
}

struct RepoEventSource {
    watcher: notify::RecommendedWatcher,
    rx: tokio::sync::mpsc::Receiver<()>,
    repo_root: PathBuf,
    ignored_paths: Vec<String>,
    watched_directories: std::collections::BTreeSet<PathBuf>,
}

impl RepoEventSource {
    fn refresh_directory_watches(&mut self) -> Result<()> {
        let mut pending = vec![self.repo_root.clone()];
        while let Some(directory) = pending.pop() {
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if directory == self.repo_root => {
                    return Err(error).with_context(|| format!("read {}", directory.display()));
                }
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                let Some(relative) = path
                    .strip_prefix(&self.repo_root)
                    .ok()
                    .and_then(|value| value.to_str())
                else {
                    continue;
                };
                if path_is_ignored(relative, &self.ignored_paths) {
                    continue;
                }
                if self.watched_directories.insert(path.clone()) {
                    notify::Watcher::watch(
                        &mut self.watcher,
                        &path,
                        notify::RecursiveMode::NonRecursive,
                    )
                    .with_context(|| format!("watch {}", path.display()))?;
                }
                // Nested repositories and submodules own their build and
                // metadata trees. Their root event still exposes checkout
                // changes without subscribing to every child directory.
                if !path.join(".git").exists() {
                    pending.push(path);
                }
            }
        }
        Ok(())
    }
}

fn refresh_repo_event_source(source: &mut Option<RepoEventSource>) {
    if let Some(source) = source
        && let Err(error) = source.refresh_directory_watches()
    {
        eprintln!("watcher could not refresh repository event subscriptions: {error}");
    }
}

fn start_repo_event_source(
    repo_root: &std::path::Path,
    ignored_paths: &[String],
) -> Result<RepoEventSource> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let root = repo_root.to_path_buf();
    let ignored = ignored_paths.to_vec();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        let Ok(event) = result else {
            return;
        };
        if repository_event_is_useful(&event, &root, &ignored) {
            let _ = tx.try_send(());
        }
    })
    .context("create repository filesystem watcher")?;
    notify::Watcher::watch(&mut watcher, repo_root, notify::RecursiveMode::NonRecursive)
        .with_context(|| format!("watch {}", repo_root.display()))?;
    let mut source = RepoEventSource {
        watcher,
        rx,
        repo_root: repo_root.to_path_buf(),
        ignored_paths: ignored_paths.to_vec(),
        watched_directories: std::collections::BTreeSet::from([repo_root.to_path_buf()]),
    };
    source.refresh_directory_watches()?;
    Ok(source)
}

fn repository_event_is_useful(
    event: &notify::Event,
    repo_root: &std::path::Path,
    ignored_paths: &[String],
) -> bool {
    if matches!(event.kind, notify::EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        path.strip_prefix(repo_root)
            .ok()
            .and_then(|relative| relative.to_str())
            .is_some_and(|relative| !path_is_ignored(relative, ignored_paths))
    })
}

fn update_backend_instance_state(
    state: &mut BackendInstanceState,
    current: Option<String>,
) -> Result<()> {
    match (&state.current, current) {
        (Some(previous), Some(current)) if previous != &current => {
            anyhow::bail!(
                "backend service restarted (instance changed from {previous} to {current}); exiting watcher for clean restart"
            );
        }
        (None, Some(current)) => {
            state.current = Some(current);
        }
        _ => {}
    }
    Ok(())
}

fn log_heartbeat_transition(
    previous: HeartbeatState,
    result: Result<codifix_record::WatcherPresenceSummary>,
) -> HeartbeatState {
    match result {
        Ok(summary) => {
            if previous == HeartbeatState::Failing {
                println!(
                    "watcher heartbeat recovered: {} active watcher(s), last heartbeat {}",
                    summary.active_count,
                    summary
                        .last_heartbeat_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "n/a".to_string())
                );
            }
            HeartbeatState::Healthy
        }
        Err(error) => {
            if previous != HeartbeatState::Failing {
                eprintln!("watcher heartbeat failed: {error}");
            }
            HeartbeatState::Failing
        }
    }
}

fn watcher_is_service_managed() -> bool {
    std::env::var("CODIFIX_WATCH_SERVICE_MANAGED")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn resolve_repo_root(config: &AppConfig, repo_root: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(repo_root) = repo_root {
        return Ok(repo_root);
    }
    if let Some(repo_root) = &config.automation.repo_root {
        return Ok(PathBuf::from(repo_root));
    }
    std::env::current_dir().context("read current directory")
}
fn resolve_project(project: Option<String>, repo_root: &std::path::Path) -> Result<String> {
    if let Some(project) = project {
        return Ok(project);
    }
    if let Some(project) = read_repo_project_slug(repo_root) {
        return Ok(project);
    }
    let Some(name) = repo_root.file_name().and_then(|value| value.to_str()) else {
        anyhow::bail!("could not determine project slug from repo root");
    };
    Ok(name.to_string())
}
#[cfg(test)]
fn resolve_writer_id(config: &AppConfig, cli_writer_id: Option<String>) -> Result<String> {
    if let Some(writer_id) = cli_writer_id {
        let trimmed = writer_id.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(writer_id) = std::env::var("CODIFIX_WRITER_ID") {
        let trimmed = writer_id.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(writer_id) = std::env::var("CODIFIX_AGENT_ID") {
        let trimmed = writer_id.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let trimmed = config.writer.id.trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_string());
    }
    Ok(platform::derive_default_writer_id("codifix-watch"))
}

#[cfg(test)]
mod tests {
    use super::{
        BackendInstanceState, HeartbeatState, log_heartbeat_transition, repository_event_is_useful,
        resolve_project, resolve_writer_id, start_repo_event_source, update_backend_instance_state,
    };
    use codifix_config::AppConfig;
    use codifix_record::WatcherPresenceSummary;
    use std::{fs, path::PathBuf, sync::Mutex};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn area_watcher_lock_rejects_duplicates_and_recovers_on_drop() {
        let root = std::env::temp_dir().join(format!("memory-area-lock-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).expect("create lock fixture");
        let mut first = codifix_config::AutomationConfig {
            state_file_path: Some(root.join("state.json").display().to_string()),
            ..Default::default()
        };
        first.area = Some(codifix_config::AreaIdentityConfig {
            name: "workshop".into(),
            instance_id: uuid::Uuid::new_v4(),
        });
        let mut second = first.clone();
        second.area = Some(codifix_config::AreaIdentityConfig {
            name: "homelab".into(),
            instance_id: uuid::Uuid::new_v4(),
        });
        let owner = super::acquire_watcher_lock(&first, &root).expect("first watcher owns lock");
        assert!(super::acquire_watcher_lock(&first, &root).is_err());
        let unrelated =
            super::acquire_watcher_lock(&second, &root).expect("separate area owns lock");
        drop(owner);
        let recovered =
            super::acquire_watcher_lock(&first, &root).expect("released lock is reusable");
        drop(recovered);
        drop(unrelated);
        fs::remove_dir_all(root).expect("clean lock fixture");
    }

    fn restore_env_var(key: &str, value: Option<String>) {
        unsafe {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn heartbeat_failure_enters_failing_state() {
        let next = log_heartbeat_transition(
            HeartbeatState::Unknown,
            Err(anyhow::anyhow!("connect failed")),
        );
        assert_eq!(next, HeartbeatState::Failing);
    }

    #[test]
    fn heartbeat_success_after_failure_recovers() {
        let summary = WatcherPresenceSummary {
            active_count: 1,
            unhealthy_count: 0,
            stale_after_seconds: 90,
            last_heartbeat_at: None,
            watchers: Vec::new(),
        };
        let next = log_heartbeat_transition(HeartbeatState::Failing, Ok(summary));
        assert_eq!(next, HeartbeatState::Healthy);
    }

    #[test]
    fn backend_instance_change_requests_watcher_restart() {
        let mut state = BackendInstanceState {
            current: Some("old-instance".to_string()),
        };
        let error = update_backend_instance_state(&mut state, Some("new-instance".to_string()))
            .expect_err("instance change should force restart");
        assert!(error.to_string().contains("backend service restarted"));
    }

    #[test]
    fn backend_instance_is_seeded_when_first_seen() {
        let mut state = BackendInstanceState::default();
        update_backend_instance_state(&mut state, Some("instance-a".to_string()))
            .expect("seed backend instance");
        assert_eq!(state.current.as_deref(), Some("instance-a"));
    }

    #[test]
    fn resolve_project_uses_repo_metadata_when_present() {
        let repo_root = unique_temp_dir("codifix-watch-project-slug");
        fs::create_dir_all(repo_root.join(".codifix")).expect("create .codifix directory");
        fs::write(
            repo_root.join(".codifix").join("project.toml"),
            "slug = \"sctp\"\nrepo_root = \"/tmp/sctp-conformance\"\n",
        )
        .expect("write project metadata");

        assert_eq!(
            resolve_project(None, &repo_root).expect("resolve project from metadata"),
            "sctp"
        );

        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn repository_events_prune_ignored_and_nested_repository_directories() {
        let repo_root = unique_temp_dir("codifix-watch-event-source");
        fs::create_dir_all(repo_root.join("src").join("nested")).expect("create source directory");
        fs::create_dir_all(repo_root.join("src").join("target").join("debug"))
            .expect("create nested target directory");
        fs::create_dir_all(repo_root.join(".git")).expect("create git directory");
        fs::create_dir_all(repo_root.join("vendor").join("checkout").join(".git"))
            .expect("create nested repository metadata");
        fs::create_dir_all(repo_root.join("vendor").join("checkout").join("src"))
            .expect("create nested repository source");

        let ignored = vec![".git/".to_string(), "target/".to_string()];
        let mut source =
            start_repo_event_source(&repo_root, &ignored).expect("start repository event source");

        assert!(source.watched_directories.contains(&repo_root));
        assert!(source.watched_directories.contains(&repo_root.join("src")));
        assert!(
            source
                .watched_directories
                .contains(&repo_root.join("src").join("nested"))
        );
        assert!(
            !source
                .watched_directories
                .contains(&repo_root.join("src").join("target"))
        );
        assert!(!source.watched_directories.contains(&repo_root.join(".git")));
        assert!(
            source
                .watched_directories
                .contains(&repo_root.join("vendor").join("checkout"))
        );
        assert!(
            !source
                .watched_directories
                .contains(&repo_root.join("vendor").join("checkout").join("src"))
        );

        fs::create_dir_all(repo_root.join("docs").join("reference"))
            .expect("create docs directory");
        source
            .refresh_directory_watches()
            .expect("refresh repository event source");
        assert!(source.watched_directories.contains(&repo_root.join("docs")));
        assert!(
            source
                .watched_directories
                .contains(&repo_root.join("docs").join("reference"))
        );

        drop(source);
        let _ = fs::remove_dir_all(repo_root);
    }

    #[test]
    fn repository_events_ignore_directory_access_but_keep_source_changes() {
        use notify::event::{AccessKind, AccessMode, ModifyKind};

        let repo_root = PathBuf::from("/tmp/memory-repo");
        let ignored = vec!["target/".to_string()];
        let access =
            notify::Event::new(notify::EventKind::Access(AccessKind::Open(AccessMode::Any)))
                .add_path(repo_root.join("src"));
        let source_change = notify::Event::new(notify::EventKind::Modify(ModifyKind::Any))
            .add_path(repo_root.join("src/lib.rs"));
        let ignored_change = notify::Event::new(notify::EventKind::Modify(ModifyKind::Any))
            .add_path(repo_root.join("target/debug/memory"));

        assert!(!repository_event_is_useful(&access, &repo_root, &ignored));
        assert!(repository_event_is_useful(
            &source_change,
            &repo_root,
            &ignored
        ));
        assert!(!repository_event_is_useful(
            &ignored_change,
            &repo_root,
            &ignored
        ));
    }

    #[test]
    fn writer_id_falls_back_to_derived_value() {
        let _guard = ENV_LOCK.lock().expect("lock test environment");
        let old_user = std::env::var("CODIFIX_WRITER_IDENTITY_USER").ok();
        let old_host = std::env::var("CODIFIX_WRITER_IDENTITY_HOST").ok();
        let old_writer = std::env::var("CODIFIX_WRITER_ID").ok();
        let old_agent = std::env::var("CODIFIX_AGENT_ID").ok();

        unsafe {
            std::env::set_var("CODIFIX_WRITER_IDENTITY_USER", "watch-user");
            std::env::set_var("CODIFIX_WRITER_IDENTITY_HOST", "watch-host");
            std::env::remove_var("CODIFIX_WRITER_ID");
            std::env::remove_var("CODIFIX_AGENT_ID");
        }

        let config: AppConfig =
            toml::from_str("[service]\n\n[database]\nurl = \"postgres://example\"\n")
                .expect("parse minimal config");
        let writer_id = resolve_writer_id(&config, None).expect("resolve writer id");

        restore_env_var("CODIFIX_WRITER_IDENTITY_USER", old_user);
        restore_env_var("CODIFIX_WRITER_IDENTITY_HOST", old_host);
        restore_env_var("CODIFIX_WRITER_ID", old_writer);
        restore_env_var("CODIFIX_AGENT_ID", old_agent);

        assert_eq!(writer_id, "codifix-watch-watch-user-watch-host");
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after Unix epoch")
                .as_nanos()
        ));
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
        path
    }
}
