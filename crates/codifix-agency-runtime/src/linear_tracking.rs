//! A project-scoped issue conversation. The originator reviews the provider's
//! exact title/body and the selected issue before a write crosses the boundary.
use crate::{
    config::Config,
    store::{Store, StoredTask},
};
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use codifix_agency::Usage;
use codifix_agency_projects::{EntityKind, LinearClient, RemoteEntity, fingerprint, stable_id};
use codifix_store::effects::{self, EffectIntent, EffectKind, EffectReason, EffectStatus};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

mod lifecycle;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Settings {
    pub endpoint: String,
    pub token_env: String,
    #[serde(default)]
    pub authorities: BTreeMap<Uuid, codifix_agency_projects::Authority>,
    #[serde(default)]
    pub require_origin_confirmation: bool,
    #[serde(default)]
    tracking: BTreeMap<Uuid, Scope>,
}
#[derive(Deserialize)]
struct Scope {
    project_id: Uuid,
    team_id: Uuid,
    #[serde(default)]
    automatic: Option<lifecycle::AutomaticScope>,
}

async fn wait(store: &Store, task: &StoredTask, question: String) -> Result<Option<Value>> {
    let mut task = task.clone();
    task.contract.clarify(question, Utc::now())?;
    store.update_contract(&task).await?;
    Ok(None)
}
async fn debit_read(store: &Store, task: &StoredTask) -> Result<()> {
    store
        .authorize(
            task.contract.id,
            task.revision,
            Usage {
                calls: 1,
                ..Usage::default()
            },
        )
        .await?;
    Ok(())
}
fn scope_matches(issue: &RemoteEntity, scope: &Scope) -> bool {
    issue.fields.get("projectId") == Some(&json!(scope.project_id))
        && issue.fields.get("teamId") == Some(&json!(scope.team_id))
}

fn tracking_metadata(value: Option<&Value>) -> Result<BTreeMap<String, Value>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let fields = value
        .as_object()
        .context("issue metadata must be an object")?;
    fields
        .iter()
        .map(|(name, value)| {
            let field = match name.as_str() {
                "priority" => {
                    let priority = value
                        .as_u64()
                        .filter(|n| *n <= 4)
                        .context("invalid issue priority")?;
                    json!(priority)
                }
                "dueDate" => {
                    let date: Option<chrono::NaiveDate> = serde_json::from_value(value.clone())?;
                    json!(date)
                }
                "assigneeId" | "stateId" | "cycleId" | "parentId" | "projectMilestoneId" => {
                    let id: Option<Uuid> = serde_json::from_value(value.clone())?;
                    ensure!(!id.is_some_and(|id| id.is_nil()), "nil issue metadata ID");
                    json!(id)
                }
                "labelIds" => {
                    let ids: BTreeSet<Uuid> = serde_json::from_value(value.clone())?;
                    ensure!(
                        ids.len() <= 100 && ids.iter().all(|id| !id.is_nil()),
                        "invalid issue labels"
                    );
                    json!(ids)
                }
                _ => anyhow::bail!("unsupported issue metadata field"),
            };
            Ok((name.clone(), field))
        })
        .collect()
}

pub(crate) async fn track(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Option<Value>> {
    let settings: Settings =
        serde_json::from_value(cfg.linear.clone().context("Linear unavailable")?)?;
    let scope = settings
        .tracking
        .get(&task.request.mandate_id)
        .context("no issue tracking scope")?;
    ensure!(
        task.request.resource.kind == "linear_project"
            && task.request.resource.id == scope.project_id.to_string(),
        "tracking project mismatch"
    );
    let data = &task.request.payload;
    if data.get("tracking_key").is_some()
        || data.get("lifecycle").is_some()
        || data.get("blocker").is_some()
        || data.get("verification").is_some()
    {
        return lifecycle::track(store, task, &settings, scope).await;
    }
    let metadata = tracking_metadata(data.get("metadata"))?;
    let title = data["title"].as_str().unwrap_or_default().trim();
    let description = data["description"].as_str().unwrap_or_default().trim();
    if title.is_empty() || description.is_empty() {
        return wait(store, task, "Please supply the proposed title and description, including how the work will be verified.".into()).await;
    }
    let lookup = data
        .get("lookup_title")
        .map(|v| v.as_str().context("invalid lookup title"))
        .transpose()?
        .unwrap_or(title)
        .trim();
    ensure!(
        !lookup.is_empty()
            && lookup.len() <= 512
            && title.len() <= 512
            && description.len() <= 16384,
        "issue wording exceeds limits"
    );
    let client = client(&settings)?;
    let scoped_key = stable_id(
        scope.project_id,
        &format!("{}:{}", scope.team_id, lookup.to_lowercase()),
    );
    let _guard = codifix_store::executions::Execution::acquire(&store.pool, scoped_key).await?;
    let explicit: Option<Uuid> = serde_json::from_value(data["issue_id"].clone())?;
    let id = if let Some(id) = explicit {
        id
    } else {
        debit_read(store, task).await?;
        match client.find_issue_ids(scope.project_id,scope.team_id,lookup).await?.as_slice() {
            [] => scoped_key,
            [id] => *id,
            _ => return wait(store,task, "Multiple issues match this title in the configured team and project. Supply the intended issue_id.".into()).await,
        }
    };
    debit_read(store, task).await?;
    let existing = client
        .scoped_issue(scope.project_id, scope.team_id, id)
        .await?;
    ensure!(
        existing.as_ref().is_none_or(|i| scope_matches(i, scope)),
        "issue outside configured project/team"
    );
    ensure!(
        explicit.is_none() || existing.is_some(),
        "selected issue does not exist"
    );
    let mut fields = metadata.clone();
    fields.insert("title".into(), json!(title));
    fields.insert("description".into(), json!(description));
    let issue = if let Some(issue) = existing.as_ref().filter(|i| matches_fields(i, &fields)) {
        issue.clone()
    } else {
        let text_digest = fingerprint(&(
            task.contract.id,
            &task.requester,
            scope.project_id,
            scope.team_id,
            id,
            existing.as_ref(),
            title,
            description,
        ));
        let (digest, changes) = if metadata.is_empty() {
            (text_digest, String::new())
        } else {
            (
                fingerprint(&(&text_digest, &metadata)),
                format!(
                    "\n\nIssue fields:\n{}",
                    serde_json::to_string_pretty(&metadata)?
                ),
            )
        };
        let confirmation = format!("confirm {digest}");
        let question = format!(
            "Proposed Linear issue {id}\n\n{title}\n\n{description}{changes}\n\nReply with `{confirmation}` or supply corrected wording."
        );
        if !confirmed(store, task, &question, &confirmation).await? {
            return wait(store, task, question).await;
        }
        if let Some(parent) = metadata.get("parentId").and_then(Value::as_str) {
            let parent = Uuid::parse_str(parent)?;
            ensure!(parent != id, "issue cannot be its own parent");
            debit_read(store, task).await?;
            ensure!(
                client
                    .scoped_issue(scope.project_id, scope.team_id, parent)
                    .await?
                    .is_some(),
                "parent issue outside configured project/team"
            );
        }
        if let Some(milestone) = metadata.get("projectMilestoneId").and_then(Value::as_str) {
            debit_read(store, task).await?;
            let milestone = client
                .retrieve(EntityKind::Milestone, Uuid::parse_str(milestone)?)
                .await?;
            ensure!(
                milestone.is_some_and(
                    |item| item.fields.get("projectId") == Some(&json!(scope.project_id))
                ),
                "milestone outside configured project"
            );
        }
        IssueWriter {
            store,
            task,
            client: &client,
            scope,
        }
        .write(id, existing.is_some(), &fields, digest)
        .await?
    };
    let url = issue
        .fields
        .get("url")
        .and_then(Value::as_str)
        .context("missing Linear issue URL")?;
    let mut evidence = json!({"issue_id":id,"url":url,"title":title,"readback_verified":true});
    if !metadata.is_empty() {
        evidence["metadata"] = json!(metadata);
    }
    Ok(Some(evidence))
}

fn client(settings: &Settings) -> Result<LinearClient> {
    Ok(LinearClient::new(
        &settings.endpoint,
        &std::env::var(&settings.token_env).context("Linear credential unavailable")?,
        std::time::Duration::from_secs(30),
    )?)
}

fn matches_fields(issue: &RemoteEntity, fields: &BTreeMap<String, Value>) -> bool {
    fields
        .iter()
        .all(|(name, value)| issue.fields.get(name) == Some(value))
}

struct IssueWriter<'a> {
    store: &'a Store,
    task: &'a StoredTask,
    client: &'a LinearClient,
    scope: &'a Scope,
}

impl IssueWriter<'_> {
    async fn write(
        &self,
        id: Uuid,
        existing: bool,
        fields: &BTreeMap<String, Value>,
        digest: String,
    ) -> Result<RemoteEntity> {
        let Self {
            store,
            task,
            client,
            scope,
        } = *self;
        let effect_id = Uuid::new_v5(&task.contract.id, digest.as_bytes());
        store
            .admit_effect(
                task.contract.id,
                task.revision,
                Usage {
                    calls: 1,
                    ..Usage::default()
                },
                &EffectIntent {
                    id: effect_id,
                    project_id: Some(task.project_id),
                    kind: EffectKind::ProviderRequest,
                    host_id: "agency".into(),
                    resource: format!("linear-issue:{id}"),
                    input_fingerprint: digest,
                    actor_id: task.contract.specialist_id.clone(),
                    source: "agency-linear-track".into(),
                },
            )
            .await?;
        let write = client
            .write_tracking_issue(id, existing, scope.project_id, scope.team_id, fields)
            .await;
        // A mutation may commit before its response or before the issue index
        // catches up. Only retry the scoped readback.
        for delay_ms in [0, 250, 750, 1500] {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            debit_read(store, task).await?;
            if let Ok(Some(issue)) = client
                .scoped_issue(scope.project_id, scope.team_id, id)
                .await
                && scope_matches(&issue, scope)
                && matches_fields(&issue, fields)
            {
                effects::conclude(
                    &store.pool,
                    effect_id,
                    Uuid::new_v4(),
                    EffectStatus::Confirmed,
                    EffectReason::Observed,
                )
                .await?;
                return Ok(issue);
            }
        }
        write?;
        anyhow::bail!("could not verify the saved Linear issue fields")
    }
}

pub(crate) fn issue_confirmation(
    changes: &codifix_agency_projects::PreparedChangeSet,
) -> Option<(String, String)> {
    let proposed:Vec<_> = changes.steps.iter()
        .filter(|s| s.kind == codifix_agency_projects::EntityKind::Issue
            && (s.patch.contains_key("title") || s.patch.contains_key("description")))
        .map(|s| json!({"id":s.entity_id,"title":s.desired.get("title"),"description":s.desired.get("description")}))
        .collect();
    if proposed.is_empty() {
        return None;
    }
    let confirmation = format!("confirm {}", fingerprint(changes));
    let question = format!(
        "Proposed Linear wording:\n{}\nReply with `{confirmation}` or supply corrections.",
        serde_json::to_string_pretty(&proposed).expect("JSON values")
    );
    Some((question, confirmation))
}

/// The task contract records the question actually sent. The inbox's durable
/// response records the authenticated peer who answered it.
pub(crate) async fn confirmed(
    store: &Store,
    task: &StoredTask,
    question: &str,
    answer: &str,
) -> Result<bool> {
    if !task
        .contract
        .clarification
        .as_ref()
        .is_some_and(|q| q.question == question && q.answer.as_deref() == Some(answer))
    {
        return Ok(false);
    }
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agency_messages WHERE task_id=$1 AND sender=$2 AND body->>'answer'=$3)").bind(task.contract.id).bind(&task.requester).bind(answer).fetch_one(&store.pool).await?)
}
