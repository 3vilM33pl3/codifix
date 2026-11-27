// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project-local policy and durable requests for the resident Linear companion.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use codifix_config::AppConfig;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    endpoint: String,
    token_file: PathBuf,
    local_specialist: String,
    local_mandate_id: Uuid,
    remote_peer: String,
    remote_specialist: String,
    remote_mandate_id: Uuid,
    linear_project_id: Uuid,
    linear_project_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Routes {
    schema_version: u16,
    projects: BTreeMap<String, Route>,
}

#[derive(Default, Deserialize, Serialize)]
struct Journal {
    projects: BTreeMap<String, Project>,
}

#[derive(Deserialize, Serialize)]
struct Project {
    slug: String,
    enabled: bool,
    destination: Uuid,
    source: Option<Uuid>,
    tasks: BTreeMap<String, Task>,
    last_sync: String,
    issue_url: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Task {
    key: Uuid,
    title: String,
    description: String,
    lifecycle: Lifecycle,
    events: VecDeque<Event>,
    #[serde(default)]
    issue_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Lifecycle {
    Started,
    Completed,
    Blocked,
}

#[derive(Clone, Deserialize, Serialize)]
struct Event {
    lifecycle: Lifecycle,
    detail: Option<String>,
    id: Uuid,
    deadline: DateTime<Utc>,
    local_task: Option<Uuid>,
    retry_after: Option<DateTime<Utc>>,
    review_required: bool,
}

impl Event {
    fn new(lifecycle: Lifecycle, detail: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            lifecycle,
            detail,
            id: Uuid::new_v4(),
            deadline: now + Duration::minutes(30),
            local_task: None,
            retry_after: None,
            review_required: false,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct Status {
    enabled: bool,
    linear_project_name: Option<String>,
    pub(crate) last_sync: String,
    issue_url: Option<String>,
    linked_issues: BTreeMap<String, String>,
}

impl Status {
    #[cfg(feature = "tui")]
    pub(crate) fn text(&self) -> String {
        let switch = if self.enabled { "On" } else { "Off" };
        format!(
            "{switch} · {} · {}",
            self.linear_project_name
                .as_deref()
                .unwrap_or("No mapped project"),
            self.last_sync
        )
    }
}

pub(crate) fn default_routes() -> Result<PathBuf> {
    Ok(codifix_platform::user_home_dir()
        .context("user home unavailable")?
        .join(".local/state/codifix-linear/project-routes.json"))
}

fn journal_path(routes: &Path) -> PathBuf {
    routes.with_file_name("project-tracking.json")
}

fn scope(config: &AppConfig, slug: &str) -> String {
    let area = config.area.as_ref().map_or_else(
        || crate::commands::output::service_url(config, ""),
        |area| area.instance_id.to_string(),
    );
    format!("{area}/{slug}")
}

fn read_owned(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file(),
        "tracking configuration must be a regular file"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let parent = path
            .parent()
            .context("tracking configuration has no parent")?;
        let parent = fs::symlink_metadata(parent)?;
        ensure!(
            parent.is_dir()
                && metadata.uid() == parent.uid()
                && parent.mode() & 0o022 == 0
                && metadata.mode() & 0o077 == 0,
            "tracking configuration must be private and owned by the state directory's owner"
        );
    }
    ensure!(
        metadata.len() <= 1024 * 1024,
        "tracking configuration too large"
    );
    Ok(fs::read(path)?)
}

fn read_routes(path: &Path) -> Result<Routes> {
    let routes: Routes = serde_json::from_slice(&read_owned(path)?)?;
    ensure!(
        routes.schema_version == 1,
        "unsupported Linear route schema"
    );
    Ok(routes)
}

fn journal(path: &Path) -> Result<Journal> {
    match fs::read(journal_path(path)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Journal::default()),
        Err(error) => Err(error.into()),
    }
}

fn update<R>(path: &Path, f: impl FnOnce(&mut Journal) -> R) -> Result<R> {
    Ok(codifix_platform::journal::update_json(
        &journal_path(path),
        |j| Ok(f(j)),
    )?)
}

pub(crate) fn status(config: &AppConfig, slug: &str) -> Result<Status> {
    let path = default_routes()?;
    let destination = match read_routes(&path) {
        Ok(routes) => routes.projects.get(slug).cloned(),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::NotFound) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    let journal = journal(&path)?;
    let current = journal.projects.get(&scope(config, slug));
    let enabled = current.is_some_and(|p| {
        p.enabled
            && destination
                .as_ref()
                .is_some_and(|d| d.linear_project_id == p.destination)
    });
    let last_sync = match current {
        Some(p) if p.enabled && !enabled => "Enrolled mapping unavailable; writes paused",
        Some(p) => &p.last_sync,
        None => "No sync yet",
    };
    Ok(Status {
        enabled,
        linear_project_name: destination.map(|d| d.linear_project_name),
        last_sync: last_sync.to_string(),
        issue_url: current.and_then(|p| p.issue_url.clone()),
        linked_issues: current
            .into_iter()
            .flat_map(|p| &p.tasks)
            .filter_map(|(thread, task)| {
                task.issue_url
                    .as_ref()
                    .map(|url| (thread.clone(), url.clone()))
            })
            .collect(),
    })
}

pub(crate) fn set_enabled(config: &AppConfig, slug: &str, enabled: bool) -> Result<Status> {
    ensure!(!config.service.read_only, "project service is read-only");
    let routes = default_routes()?;
    let key = scope(config, slug);
    if enabled {
        let choices = read_routes(&routes).context("load resident companion project mappings")?;
        let choice = choices
            .projects
            .get(slug)
            .context("no approved Linear mapping for this project")?;
        let _ = credentials(&routes, choice)?;
        update(&routes, |j| {
            if let Some(p) = j.projects.get_mut(&key) {
                // A remap must be enrolled separately so queued writes and issue links
                // cannot follow an edited route file into a different Linear project.
                if p.destination != choice.linear_project_id {
                    return false;
                }
                p.enabled = true;
            } else {
                j.projects.insert(
                    key,
                    Project {
                        slug: slug.to_string(),
                        enabled: true,
                        destination: choice.linear_project_id,
                        source: None,
                        tasks: BTreeMap::new(),
                        last_sync: "No sync yet".into(),
                        issue_url: None,
                    },
                );
            }
            true
        })
        .and_then(|ok| {
            ensure!(
                ok,
                "existing tracking history is bound to a different Linear destination"
            );
            Ok(())
        })?;
    } else if routes.parent().is_some_and(Path::is_dir) {
        update(&routes, |j| {
            if let Some(p) = j.projects.get_mut(&key) {
                p.enabled = false;
            }
        })?;
    }
    status(config, slug)
}

pub(crate) fn enqueue(
    config: &AppConfig,
    slug: &str,
    source: Option<Uuid>,
    thread: &str,
    title: &str,
    lifecycle: Lifecycle,
    detail: Option<String>,
) -> Result<()> {
    if config.service.read_only {
        return Ok(());
    }
    let routes = default_routes()?;
    let project = scope(config, slug);
    if !journal(&routes)?
        .projects
        .get(&project)
        .is_some_and(|p| p.enabled)
    {
        return Ok(());
    }
    update(&routes, |j| {
        enqueue_event(j, &project, source, thread, title, lifecycle, detail)
    })?
}

fn enqueue_event(
    journal: &mut Journal,
    project: &str,
    source: Option<Uuid>,
    thread: &str,
    title: &str,
    lifecycle: Lifecycle,
    detail: Option<String>,
) -> Result<()> {
    let Some(p) = journal.projects.get_mut(project).filter(|p| p.enabled) else {
        return Ok(());
    };
    if let Some(source) = source {
        ensure!(
            !source.is_nil() && p.source.is_none_or(|id| id == source),
            "Codifix project identity changed"
        );
        p.source = Some(source);
    }
    let Some(source) = p.source else {
        return Ok(());
    };
    match p.tasks.get_mut(thread) {
        Some(task)
            if task.lifecycle != Lifecycle::Completed
                && (task.lifecycle != lifecycle
                    || lifecycle == Lifecycle::Blocked
                        && task.events.back().is_none_or(|e| e.detail != detail)) =>
        {
            task.lifecycle = lifecycle;
            task.events.push_back(Event::new(lifecycle, detail));
        }
        None if lifecycle == Lifecycle::Started => {
            let title = title.trim();
            ensure!(
                !title.is_empty() && title.len() <= 512 && !title.chars().any(char::is_control),
                "Linear title must be shareable text up to 512 bytes"
            );
            let summary = detail.as_deref().unwrap_or(title).trim();
            ensure!(
                !summary.is_empty()
                    && summary.len() <= 2048
                    && !summary.chars().any(|ch| ch.is_control() && ch != '\n'),
                "Linear summary must be shareable text up to 2048 bytes"
            );
            let description = format!(
                "{summary}\n\nVerification: Codifix records the checks before marking the work complete."
            );
            p.tasks.insert(
                thread.to_string(),
                Task {
                    key: Uuid::new_v5(&source, format!("{project}/{thread}").as_bytes()),
                    description,
                    title: title.into(),
                    lifecycle,
                    events: VecDeque::from([Event::new(lifecycle, None)]),
                    issue_url: None,
                },
            );
        }
        _ => return Ok(()),
    }
    p.last_sync = "Pending".into();
    Ok(())
}

fn credentials(routes: &Path, route: &Route) -> Result<(Url, String)> {
    let url = Url::parse(&route.endpoint)?;
    ensure!(
        url.host_str().is_some_and(|host| host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback()))
            && matches!(url.scheme(), "http" | "https")
            && url.path() == "/"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "resident companion endpoint must be a loopback HTTP origin"
    );
    ensure!(
        route.token_file.is_absolute() && route.token_file.parent() == routes.parent(),
        "resident operator token must be in the project routes directory"
    );
    let token = String::from_utf8(read_owned(&route.token_file)?)?;
    ensure!(
        !token.trim().is_empty(),
        "resident operator token unavailable"
    );
    Ok((url, token.trim().into()))
}

#[derive(Clone)]
struct Pending {
    thread: String,
    key: Uuid,
    title: String,
    description: String,
    event: Event,
}

fn work(path: &Path, scope: &str, route: &Route) -> Result<Option<Pending>> {
    update(path, |j| {
        let p = j
            .projects
            .get_mut(scope)
            .filter(|p| p.enabled && p.destination == route.linear_project_id)?;
        let now = Utc::now();
        let (thread, task) = p.tasks.iter_mut().find(|(_, t)| {
            t.events.front().is_some_and(|e| {
                !e.review_required && e.retry_after.is_none_or(|retry| retry <= now)
            })
        })?;
        let event = task.events.front_mut()?;
        // If the local job has an ID, resolve its outcome even when its deadline passed.
        // A lost submission response can otherwise be retried with the exact same ID.
        if event.deadline <= now && event.local_task.is_none() {
            *event = Event::new(event.lifecycle, event.detail.clone());
        }
        Some(Pending {
            thread: thread.to_string(),
            key: task.key,
            title: task.title.clone(),
            description: task.description.clone(),
            event: event.clone(),
        })
    })
}

fn request(route: &Route, task: &Pending) -> Value {
    let event = &task.event;
    let mut payload = json!({
        "kind":"linear_track","tracking_key":task.key,"lifecycle":event.lifecycle,
        "title":task.title,"description":task.description,
    });
    if let Some(detail) = &event.detail {
        match event.lifecycle {
            Lifecycle::Blocked => payload["blocker"] = json!(detail),
            Lifecycle::Completed => payload["verification"] = json!(detail),
            Lifecycle::Started => {}
        }
    }
    json!({
        "request_id":event.id,"mandate_id":route.local_mandate_id,"specialist_id":route.local_specialist,
        "objective":"Synchronize the Codifix task with Linear","capability":"delegate",
        "resource":{"kind":"peer","id":route.remote_peer},"acceptance_criteria":["delegated_result_reported"],
        "deadline":event.deadline,"budget":{"micro_usd":0,"calls":30,"turns":10,"delegation_depth":1},
        "payload":{"kind":"delegate","peer":route.remote_peer,"request":{
            "request_id":event.id,"mandate_id":route.remote_mandate_id,"specialist_id":route.remote_specialist,
            "objective":"Track the Codifix task without rewriting it","capability":"linear_track",
            "resource":{"kind":"linear_project","id":route.linear_project_id},
            "acceptance_criteria":["linear_issue_readback_verified"],"deadline":event.deadline,
            "budget":{"micro_usd":0,"calls":20,"turns":7,"delegation_depth":0},"payload":payload
        }}
    })
}

async fn call(
    client: &Client,
    origin: &Url,
    token: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = origin.join(path)?;
    let response = match body {
        Some(body) => client.post(url).json(&body),
        None => client.get(url),
    }
    .bearer_auth(token)
    .header("A2A-Version", "1.0")
    .send()
    .await?
    .error_for_status()?;
    Ok(response.json().await?)
}

enum Sync {
    Submitted(Uuid),
    Working,
    Verified(String),
    Review,
    Retry,
}

async fn sync_event(client: &Client, path: &Path, route: &Route, task: &Pending) -> Result<Sync> {
    let (origin, token) = credentials(path, route)?;
    let Some(id) = task.event.local_task else {
        let local = call(
            client,
            &origin,
            &token,
            "/v1/tasks",
            Some(request(route, task)),
        )
        .await?;
        return Ok(Sync::Submitted(serde_json::from_value(
            local["contract"]["id"].clone(),
        )?));
    };
    let reply = call(client, &origin, &token, &format!("/tasks/{id}"), None).await?;
    match reply["status"]["state"].as_str() {
        Some("TASK_STATE_COMPLETED") => {
            Ok(verified_url(&reply).map_or(Sync::Review, Sync::Verified))
        }
        Some("TASK_STATE_INPUT_REQUIRED") => {
            let message = &reply["status"]["message"]["parts"];
            let retryable = message.as_array().is_some_and(|parts| parts.iter().any(|part| {
                part["text"].as_str().is_some_and(|text| text.starts_with("Execution stopped at an authorization, validation, or uncertain-effect boundary."))
            }));
            Ok(if retryable { Sync::Retry } else { Sync::Review })
        }
        Some("TASK_STATE_FAILED" | "TASK_STATE_CANCELED" | "TASK_STATE_REJECTED") => {
            Ok(Sync::Retry)
        }
        Some("TASK_STATE_SUBMITTED" | "TASK_STATE_WORKING")
            if task.event.deadline <= Utc::now() =>
        {
            Ok(Sync::Retry)
        }
        _ => Ok(Sync::Working),
    }
}

pub(crate) async fn drain(path: &Path, only: Option<&str>) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "Linear project routes path must be absolute"
    );
    let routes = read_routes(path)?;
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(4))
        .build()?;
    let snapshot = journal(path)?;
    for (scope, p) in snapshot.projects {
        if !p.enabled || only.is_some_and(|key| key != scope) {
            continue;
        }
        let Some(route) = routes
            .projects
            .get(&p.slug)
            .filter(|r| r.linear_project_id == p.destination)
        else {
            continue;
        };
        let Some(task) = work(path, &scope, route)? else {
            continue;
        };
        let result = sync_event(&client, path, route, &task).await;
        update(path, |j| apply_result(j, &scope, &task, result))?;
    }
    Ok(())
}

fn apply_result(j: &mut Journal, scope: &str, task: &Pending, result: Result<Sync>) {
    let Some(project) = j.projects.get_mut(scope) else {
        return;
    };
    let Some(tracked) = project.tasks.get_mut(&task.thread) else {
        return;
    };
    let Some(event) = tracked.events.front_mut().filter(|e| e.id == task.event.id) else {
        return;
    };
    match result {
        Ok(Sync::Submitted(id)) => {
            event.local_task = Some(id);
            project.last_sync = "Submitted".into();
        }
        Ok(Sync::Working) => {
            project.last_sync = "In progress".into();
            event.retry_after = Some(Utc::now() + Duration::seconds(2));
        }
        Ok(Sync::Verified(url)) => {
            tracked.events.pop_front();
            tracked.issue_url = Some(url.clone());
            project.issue_url = Some(url);
            project.last_sync = "Verified".into();
        }
        Ok(Sync::Review) => {
            event.review_required = true;
            project.last_sync = "Needs review in the companion".into();
        }
        Ok(Sync::Retry) => {
            *event = Event::new(event.lifecycle, event.detail.clone());
            event.retry_after = Some(Utc::now() + Duration::seconds(30));
            project.last_sync = "Sync failed; retry scheduled".into();
        }
        Err(_) => {
            event.retry_after = Some(Utc::now() + Duration::seconds(15));
            project.last_sync = "Companion unavailable; retry scheduled".into();
        }
    }
}

fn artifacts(task: &Value) -> impl Iterator<Item = &Value> {
    task["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|a| a["parts"].as_array().into_iter().flatten())
        .filter_map(|p| p.get("data"))
}

pub(crate) fn verified_url(task: &Value) -> Option<String> {
    if task["status"]["state"] != "TASK_STATE_COMPLETED" {
        return None;
    }
    for delegate in artifacts(task) {
        let result = &delegate["reported_result"];
        if result["status"]["state"] != "TASK_STATE_COMPLETED" {
            continue;
        }
        for evidence in artifacts(result) {
            if evidence["readback_verified"] == true
                && evidence["issue_id"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_some()
            {
                let url = Url::parse(evidence["url"].as_str()?).ok()?;
                if url.scheme() == "https" && url.username().is_empty() && url.password().is_none()
                {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}

pub(crate) const HELP: &str = "\
Agent notes:
  Select the same area and project used for the Codifix task. The companion uses the operator-enrolled Linear route.
  Use status to inspect state and block to record a blocker on an existing tracked task.

See also:
  docs/user/cli/linear.md";

#[derive(Debug, clap::Args)]
pub(crate) struct LinearArgs {
    #[command(subcommand)]
    pub(crate) command: LinearCommand,
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum LinearCommand {
    /// Show the configured destination, switch and last synchronization result.
    #[command(after_help = HELP)]
    Status {
        #[arg(long)]
        project: Option<String>,
    },
    /// Enable the resident agent's enrolled route for this Codifix project.
    #[command(after_help = HELP)]
    Enable {
        #[arg(long)]
        project: Option<String>,
    },
    /// Stop automatic writes and retain previously verified issue links.
    #[command(after_help = HELP)]
    Disable {
        #[arg(long)]
        project: Option<String>,
    },
    /// Advance one pending synchronization for this project.
    #[command(after_help = HELP)]
    Sync {
        #[arg(long)]
        project: Option<String>,
    },
    /// Report a blocker on an already tracked task.
    #[command(after_help = HELP)]
    Block {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        thread_key: String,
        #[arg(long)]
        reason: String,
    },
    /// Advance locally queued requests. Used by the resident companion.
    #[command(hide = true, after_help = HELP)]
    Drain {
        #[arg(long)]
        routes: Option<PathBuf>,
    },
}

pub(crate) async fn handle(args: LinearArgs, config: &AppConfig) -> Result<()> {
    let project = match &args.command {
        LinearCommand::Status { project }
        | LinearCommand::Enable { project }
        | LinearCommand::Disable { project }
        | LinearCommand::Sync { project }
        | LinearCommand::Block { project, .. } => project.clone(),
        LinearCommand::Drain { .. } => {
            unreachable!("resident drain bypasses backend configuration")
        }
    };
    let cwd = std::env::current_dir()?;
    let slug = crate::commands::memory_ops::resolve_project_slug(project, &cwd)?;
    let result = match args.command {
        LinearCommand::Status { .. } => status(config, &slug)?,
        LinearCommand::Enable { .. } => set_enabled(config, &slug, true)?,
        LinearCommand::Disable { .. } => set_enabled(config, &slug, false)?,
        LinearCommand::Sync { .. } => {
            ensure!(!config.service.read_only, "project service is read-only");
            drain(&default_routes()?, Some(&scope(config, &slug))).await?;
            status(config, &slug)?
        }
        LinearCommand::Block {
            thread_key, reason, ..
        } => {
            ensure!(!config.service.read_only, "project service is read-only");
            ensure!(
                !reason.trim().is_empty() && reason.len() <= 512,
                "provide a blocker up to 512 bytes"
            );
            let routes = default_routes()?;
            let key = scope(config, &slug);
            ensure!(
                journal(&routes)?
                    .projects
                    .get(&key)
                    .is_some_and(|p| p.enabled && p.tasks.contains_key(&thread_key)),
                "no enabled tracking for that task thread"
            );
            enqueue(
                config,
                &slug,
                None,
                &thread_key,
                "",
                Lifecycle::Blocked,
                Some(reason),
            )?;
            status(config, &slug)?
        }
        LinearCommand::Drain { .. } => {
            unreachable!("resident drain bypasses backend configuration")
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(feature = "tui")]
pub(crate) fn toggle(config: &AppConfig, slug: &str) -> Result<Status> {
    let path = default_routes()?;
    let enabled = journal(&path)?
        .projects
        .get(&scope(config, slug))
        .is_some_and(|project| project.enabled);
    set_enabled(config, slug, !enabled)
}

pub(crate) fn report_enqueue(result: Result<()>) {
    if let Err(error) = result {
        eprintln!("warning: could not queue Linear tracking: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(slug: &str, id: Uuid, enabled: bool) -> Project {
        Project {
            slug: slug.into(),
            enabled,
            destination: id,
            source: None,
            tasks: BTreeMap::new(),
            last_sync: "No sync yet".into(),
            issue_url: None,
        }
    }

    #[test]
    fn task_identity_is_scoped_and_completed_work_is_not_reopened() {
        let codifix = Uuid::new_v4();
        let homelab = Uuid::new_v4();
        let mut journal = Journal::default();
        journal.projects.insert(
            "scope/codifix".into(),
            project("codifix", Uuid::new_v4(), true),
        );
        journal.projects.insert(
            "scope/homelab".into(),
            project("homelab", Uuid::new_v4(), true),
        );
        for (scope, source) in [("scope/codifix", codifix), ("scope/homelab", homelab)] {
            for lifecycle in [
                Lifecycle::Started,
                Lifecycle::Started,
                Lifecycle::Completed,
                Lifecycle::Started,
            ] {
                enqueue_event(
                    &mut journal,
                    scope,
                    Some(source),
                    "same-title",
                    "Same title",
                    lifecycle,
                    None,
                )
                .unwrap();
            }
            let task = &journal.projects[scope].tasks["same-title"];
            assert_eq!(task.lifecycle, Lifecycle::Completed);
            assert_eq!(task.events.len(), 2);
        }
        assert_ne!(
            journal.projects["scope/codifix"].tasks["same-title"].key,
            journal.projects["scope/homelab"].tasks["same-title"].key,
        );
        assert!(
            enqueue_event(
                &mut journal,
                "scope/codifix",
                Some(homelab),
                "different",
                "Different",
                Lifecycle::Started,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn interruption_keeps_the_request_identity_and_disabling_preserves_issue_links() {
        let mut journal = Journal::default();
        journal.projects.insert(
            "scope/codifix".into(),
            project("codifix", Uuid::new_v4(), true),
        );
        enqueue_event(
            &mut journal,
            "scope/codifix",
            Some(Uuid::new_v4()),
            "start",
            "Start",
            Lifecycle::Started,
            None,
        )
        .unwrap();
        let task = &journal.projects["scope/codifix"].tasks["start"];
        let pending = Pending {
            thread: "start".into(),
            key: task.key,
            title: task.title.clone(),
            description: task.description.clone(),
            event: task.events[0].clone(),
        };
        apply_result(
            &mut journal,
            "scope/codifix",
            &pending,
            Err(anyhow::anyhow!("connection lost")),
        );
        let p = &journal.projects["scope/codifix"];
        let event = &p.tasks["start"].events[0];
        assert_eq!(event.id, pending.event.id);
        assert!(event.retry_after.is_some());

        let issue = "https://linear.app/team/issue/COD-42";
        apply_result(
            &mut journal,
            "scope/codifix",
            &pending,
            Ok(Sync::Verified(issue.into())),
        );
        journal.projects.get_mut("scope/codifix").unwrap().enabled = false;
        enqueue_event(
            &mut journal,
            "scope/codifix",
            None,
            "start",
            "Start",
            Lifecycle::Completed,
            None,
        )
        .unwrap();
        let p = &journal.projects["scope/codifix"];
        assert_eq!(p.issue_url.as_deref(), Some(issue));
        assert_eq!(p.tasks["start"].issue_url.as_deref(), Some(issue));
        assert!(p.tasks["start"].events.is_empty());
    }

    #[test]
    fn start_uses_only_the_explicit_shareable_summary() {
        let mut journal = Journal::default();
        journal.projects.insert(
            "scope/codifix".into(),
            project("codifix", Uuid::new_v4(), true),
        );
        enqueue_event(
            &mut journal,
            "scope/codifix",
            Some(Uuid::new_v4()),
            "start",
            "Start",
            Lifecycle::Started,
            Some("Support the project switch.".into()),
        )
        .unwrap();
        let task = &journal.projects["scope/codifix"].tasks["start"];
        assert!(
            task.description
                .starts_with("Support the project switch.\n")
        );
        assert_eq!(task.events[0].detail, None);
    }

    #[test]
    fn retries_keep_the_destination_and_include_verification_at_completion() {
        let id = Uuid::new_v4();
        let route = Route {
            endpoint: "http://127.0.0.1:4060".into(),
            token_file: "/fixture/token".into(),
            local_specialist: "home".into(),
            local_mandate_id: Uuid::new_v4(),
            remote_peer: "linear".into(),
            remote_specialist: "linear".into(),
            remote_mandate_id: Uuid::new_v4(),
            linear_project_id: id,
            linear_project_name: "Codifix".into(),
        };
        let task = Pending {
            thread: "task".into(),
            key: Uuid::new_v4(),
            title: "Task".into(),
            description: "Verbatim".into(),
            event: Event::new(Lifecycle::Completed, Some("Verified in Codifix".into())),
        };
        let first = request(&route, &task);
        let second = request(&route, &task);
        assert_eq!(first, second);
        assert_eq!(first["payload"]["request"]["resource"]["id"], json!(id));
        assert_eq!(
            first["payload"]["request"]["payload"]["tracking_key"],
            json!(task.key)
        );
        assert_eq!(
            first["payload"]["request"]["payload"]["verification"],
            "Verified in Codifix"
        );
    }
}
