//! Provider changes initiated by an enrolled task lifecycle publisher.
//! The issue ID is tied to the stable task key within its Linear project.
use super::{IssueWriter, Scope, Settings, client, debit_read, matches_fields, scope_matches};
use crate::store::{Store, StoredTask};
use anyhow::{Context, Result, ensure};
use codifix_agency_projects::{RemoteEntity, fingerprint, stable_id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

const MAX_DESCRIPTION: usize = 16_384;
const BLOCKER_HEADING: &str = "\n\n### Blocker\n\n";
const VERIFICATION_HEADING: &str = "\n\n### Verification\n\n";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AutomaticScope {
    requester: String,
    in_progress_state_id: Uuid,
    done_state_id: Uuid,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Lifecycle {
    Started,
    Completed,
    Blocked,
}

#[derive(Deserialize)]
struct Event {
    tracking_key: Uuid,
    lifecycle: Lifecycle,
    title: String,
    description: String,
    blocker: Option<String>,
    verification: Option<String>,
}

pub(super) async fn track(
    store: &Store,
    task: &StoredTask,
    settings: &Settings,
    scope: &Scope,
) -> Result<Option<Value>> {
    let automatic = scope
        .automatic
        .as_ref()
        .context("automatic issue tracking is disabled for this mandate")?;
    ensure!(
        task.requester == automatic.requester,
        "automatic issue publisher is not enrolled on this mandate"
    );
    ensure!(
        !automatic.in_progress_state_id.is_nil()
            && !automatic.done_state_id.is_nil()
            && automatic.in_progress_state_id != automatic.done_state_id,
        "invalid automatic Linear workflow states"
    );

    let payload = &task.request.payload;
    let fields = payload.as_object().context("invalid lifecycle payload")?;
    ensure!(
        fields.keys().all(|field| matches!(
            field.as_str(),
            "kind"
                | "tracking_key"
                | "lifecycle"
                | "title"
                | "description"
                | "blocker"
                | "verification"
                | "delegation_trace"
                | "execution_lease_seconds"
                | "source_destinations"
        )),
        "unsupported automatic tracking field"
    );
    let event: Event = serde_json::from_value(payload.clone())?;
    ensure!(
        !event.tracking_key.is_nil()
            && !event.title.trim().is_empty()
            && event.title.len() <= 512
            && !event.description.trim().is_empty()
            && event.description.len() <= MAX_DESCRIPTION,
        "invalid automatic issue wording or task identity"
    );
    ensure!(
        if event.lifecycle == Lifecycle::Blocked {
            event
                .blocker
                .as_ref()
                .is_some_and(|blocker| !blocker.trim().is_empty() && blocker.len() <= 2048)
        } else {
            event.blocker.is_none()
        },
        "a blocker is valid only on a blocked event"
    );
    ensure!(
        event.verification.as_ref().is_none_or(|report| {
            event.lifecycle == Lifecycle::Completed
                && !report.trim().is_empty()
                && report.len() <= 2048
        }),
        "verification is valid only on a completed event"
    );

    let id = stable_id(
        scope.project_id,
        &format!("automatic:{}:{}", scope.team_id, event.tracking_key),
    );
    let client = client(settings)?;
    let _guard = codifix_store::executions::Execution::acquire(&store.pool, id).await?;
    debit_read(store, task).await?;
    let existing = client
        .scoped_issue(scope.project_id, scope.team_id, id)
        .await?;
    ensure!(
        existing
            .as_ref()
            .is_none_or(|issue| scope_matches(issue, scope)),
        "automatic issue outside configured project/team"
    );
    let fields = desired_fields(&event, automatic, existing.as_ref())?;
    let issue = if let Some(issue) = existing
        .as_ref()
        .filter(|issue| matches_fields(issue, &fields))
    {
        issue.clone()
    } else {
        let digest = fingerprint(&(id, &event.lifecycle, &fields));
        IssueWriter {
            store,
            task,
            client: &client,
            scope,
        }
        .write(id, existing.is_some(), &fields, digest)
        .await?
    };
    let url = issue.fields["url"]
        .as_str()
        .context("missing Linear issue URL")?;
    Ok(Some(json!({
        "issue_id": id,
        "url": url,
        "tracking_key": event.tracking_key,
        "lifecycle": event.lifecycle,
        "state_id": issue.fields.get("stateId"),
        "readback_verified": true,
    })))
}

fn desired_fields(
    event: &Event,
    scope: &AutomaticScope,
    existing: Option<&RemoteEntity>,
) -> Result<BTreeMap<String, Value>> {
    let mut fields = BTreeMap::new();
    if let Some(issue) = existing {
        // Resuming cannot overwrite an operator's edits or move an issue out
        // of its current state. A completed issue also ignores old blockers.
        if event.lifecycle != Lifecycle::Completed
            && issue.fields.get("stateId") == Some(&json!(scope.done_state_id))
        {
            return Ok(fields);
        }
        match event.lifecycle {
            Lifecycle::Started => {}
            Lifecycle::Completed => {
                fields.insert("stateId".into(), json!(scope.done_state_id));
                if let Some(report) = &event.verification {
                    let description = issue.fields["description"]
                        .as_str()
                        .context("missing tracked issue description")?;
                    fields.insert(
                        "description".into(),
                        json!(append_note(description, VERIFICATION_HEADING, report)?),
                    );
                }
            }
            Lifecycle::Blocked => {
                let description = issue.fields["description"]
                    .as_str()
                    .context("missing tracked issue description")?;
                let blocker = event.blocker.as_deref().context("missing blocker")?;
                fields.insert(
                    "description".into(),
                    json!(append_note(description, BLOCKER_HEADING, blocker)?),
                );
            }
        }
    } else {
        let state = match event.lifecycle {
            Lifecycle::Completed => scope.done_state_id,
            Lifecycle::Started | Lifecycle::Blocked => scope.in_progress_state_id,
        };
        let description = match (&event.blocker, &event.verification) {
            (Some(blocker), _) => append_note(&event.description, BLOCKER_HEADING, blocker)?,
            (_, Some(report)) => append_note(&event.description, VERIFICATION_HEADING, report)?,
            _ => event.description.clone(),
        };
        fields.insert("title".into(), json!(&event.title));
        fields.insert("description".into(), json!(description));
        fields.insert("stateId".into(), json!(state));
    }
    Ok(fields)
}

fn append_note(description: &str, heading: &str, text: &str) -> Result<String> {
    let note = format!("{heading}{text}");
    if description.ends_with(&note)
        || [BLOCKER_HEADING, VERIFICATION_HEADING]
            .iter()
            .any(|next| description.contains(&format!("{note}{next}")))
    {
        return Ok(description.to_owned());
    }
    ensure!(
        description.len() + note.len() <= MAX_DESCRIPTION,
        "issue description exceeds limits"
    );
    Ok(format!("{description}{note}"))
}
