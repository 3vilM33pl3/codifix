//! Additional records a Linear specialist manages alongside a reviewed project plan.
use crate::{EntityKind, Error, Identity, Plan, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoadmapRecord {
    pub identity: Identity,
    pub project_key: String,
    pub change: RoadmapChange,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoadmapChange {
    Initiative {
        name: String,
        description: String,
        owner_id: Option<Uuid>,
        target_date: Option<chrono::NaiveDate>,
        status: Option<InitiativeStatus>,
    },
    InitiativeProject {
        initiative_key: String,
    },
    ProjectDependency {
        blocked_project_key: String,
    },
    ProjectUpdate {
        body: String,
        health: Health,
    },
    InitiativeUpdate {
        initiative_key: String,
        body: String,
        health: Health,
    },
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InitiativeStatus {
    Proposed,
    Planned,
    Active,
    Completed,
    Canceled,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    OnTrack,
    AtRisk,
    OffTrack,
}

impl RoadmapRecord {
    pub fn kind(&self) -> EntityKind {
        match self.change {
            RoadmapChange::Initiative { .. } => EntityKind::Initiative,
            RoadmapChange::InitiativeProject { .. } => EntityKind::InitiativeProject,
            RoadmapChange::ProjectDependency { .. } => EntityKind::ProjectDependency,
            RoadmapChange::ProjectUpdate { .. } => EntityKind::ProjectUpdate,
            RoadmapChange::InitiativeUpdate { .. } => EntityKind::InitiativeUpdate,
        }
    }
    pub(crate) fn validate(&self, plan: &Plan) -> Result<()> {
        let project_exists = |key: &str| plan.projects.iter().any(|p| p.identity.key == key);
        let initiative_exists = |key: &str| {
            plan.roadmap.iter().any(|r| {
                r.identity.key == key && matches!(r.change, RoadmapChange::Initiative { .. })
            })
        };
        let ok = project_exists(&self.project_key)
            && match &self.change {
                RoadmapChange::Initiative { name, .. } => !name.trim().is_empty(),
                RoadmapChange::InitiativeProject { initiative_key } => {
                    initiative_exists(initiative_key)
                }
                RoadmapChange::ProjectDependency {
                    blocked_project_key,
                } => {
                    blocked_project_key != &self.project_key && project_exists(blocked_project_key)
                }
                RoadmapChange::ProjectUpdate { body, .. } => !body.trim().is_empty(),
                RoadmapChange::InitiativeUpdate {
                    initiative_key,
                    body,
                    ..
                } => initiative_exists(initiative_key) && !body.trim().is_empty(),
            };
        if !ok {
            return Err(Error::Validation(
                "invalid roadmap text, project or initiative reference".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn desired(&self, ids: &BTreeMap<String, Uuid>) -> Value {
        let project = ids[&self.project_key];
        match &self.change {
            RoadmapChange::Initiative {
                name,
                description,
                owner_id,
                target_date,
                status,
            } => {
                let mut fields = json!({"name":name,"description":description});
                if let Some(id) = owner_id {
                    fields["ownerId"] = json!(id);
                }
                if let Some(date) = target_date {
                    fields["targetDate"] = json!(date);
                }
                if let Some(status) = status {
                    fields["status"] = json!(status);
                }
                fields
            }
            RoadmapChange::InitiativeProject { initiative_key } => {
                json!({"initiativeId":ids[initiative_key],"projectId":project})
            }
            RoadmapChange::ProjectDependency {
                blocked_project_key,
            } => {
                json!({"projectId":project,"relatedProjectId":ids[blocked_project_key],"type":"dependency","anchorType":"end","relatedAnchorType":"start"})
            }
            RoadmapChange::ProjectUpdate { body, health } => {
                json!({"projectId":project,"body":body,"health":health})
            }
            RoadmapChange::InitiativeUpdate {
                initiative_key,
                body,
                health,
            } => json!({"initiativeId":ids[initiative_key],"body":body,"health":health}),
        }
    }
}
