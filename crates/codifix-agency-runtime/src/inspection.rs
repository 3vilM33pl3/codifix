//! Read-only plan monitoring. Every provider read is authorized and metered;
//! observations and missing resources are facts, not delivery forecasts.
use crate::{
    config::Config,
    store::{Store, StoredTask},
};
use anyhow::{Result, ensure};
use chrono::Utc;
use codifix_agency::{ResourceScope, Usage};
use codifix_agency_projects::{
    Authority, EntityKind, InspectionSnapshot, Plan, authorize_plan, stable_id,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinearSettings {
    endpoint: String,
    token_env: String,
    authorities: BTreeMap<Uuid, Authority>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    Present,
    Missing,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservation {
    pub key: String,
    pub id: Uuid,
    pub kind: EntityKind,
    pub status: ObservationStatus,
    pub snapshot: Option<InspectionSnapshot>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionBlocker {
    MissingResource {
        key: String,
        id: Uuid,
        resource_kind: EntityKind,
    },
    UnresolvedSourceQuestion {
        question: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInspection {
    pub plan_id: Uuid,
    pub plan_revision: u64,
    pub plan_digest: String,
    pub source: codifix_agency_projects::Source,
    pub observed_at: chrono::DateTime<Utc>,
    pub resources: Vec<ResourceObservation>,
    pub blockers: Vec<InspectionBlocker>,
    pub interpretation: String,
}

pub async fn plan_inspect(store: &Store, cfg: &Config, task: &StoredTask) -> Result<Value> {
    let plan: Plan = serde_json::from_value(task.request.payload["plan"].clone())?;
    plan.validate()?;
    ensure!(
        task.request.capability == "plan_inspect" && task.request.payload["kind"] == "plan_inspect",
        "inspection capability required"
    );
    ensure!(
        task.request.resource
            == ResourceScope {
                kind: "plan".into(),
                id: plan.id.to_string()
            },
        "plan resource binding mismatch"
    );
    let settings: LinearSettings = serde_json::from_value(
        cfg.linear
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Linear is not configured"))?,
    )?;
    let authority = settings
        .authorities
        .get(&task.request.mandate_id)
        .ok_or_else(|| anyhow::anyhow!("no Linear scope for mandate"))?;
    // This checks every resource, including adoption/team/project identities,
    // before even creating the provider client or fetching its first resource.
    authorize_plan(&plan, authority)?;
    store
        .authorize(task.contract.id, task.revision, Usage::default())
        .await?;
    let token = std::env::var(&settings.token_env)
        .map_err(|_| anyhow::anyhow!("Linear credential unavailable"))?;
    let client = codifix_agency_projects::LinearClient::new(
        &settings.endpoint,
        &token,
        std::time::Duration::from_secs(30),
    )?;
    let selected = plan
        .projects
        .iter()
        .map(|p| (&p.identity, EntityKind::Project))
        .chain(
            plan.milestones
                .iter()
                .map(|m| (&m.identity, EntityKind::Milestone)),
        )
        .chain(plan.issues.iter().map(|i| (&i.identity, EntityKind::Issue)))
        .chain(
            plan.dependencies
                .iter()
                .map(|d| (&d.identity, EntityKind::Dependency)),
        );
    let mut resources = Vec::new();
    let mut blockers = plan
        .unresolved_questions
        .iter()
        .map(|question| InspectionBlocker::UnresolvedSourceQuestion {
            question: question.clone(),
        })
        .collect::<Vec<_>>();
    for (identity, kind) in selected {
        let id = identity
            .remote_id
            .unwrap_or_else(|| stable_id(plan.id, &identity.key));
        // Recheck live mandate/ancestors, task revision/deadline and remaining
        // budget immediately before EACH read. Never charge one call for a batch.
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
        let snapshot = client.inspect(kind, id).await?;
        let status = if snapshot.is_some() {
            ObservationStatus::Present
        } else {
            blockers.push(InspectionBlocker::MissingResource {
                key: identity.key.clone(),
                id,
                resource_kind: kind,
            });
            ObservationStatus::Missing
        };
        resources.push(ResourceObservation {
            key: identity.key.clone(),
            id,
            kind,
            status,
            snapshot,
        });
    }
    // A revocation/cancel during the final provider read must not publish a new
    // observation as a successfully authorized task result.
    store
        .authorize(task.contract.id, task.revision, Usage::default())
        .await?;
    Ok(serde_json::to_value(PlanInspection{plan_id:plan.id,plan_revision:plan.revision,plan_digest:plan.digest(),source:plan.source,
        observed_at:Utc::now(),resources,blockers,interpretation:"Snapshot of selected provider resources; no delivery forecast or unobserved blocker inference.".into()})?)
}
