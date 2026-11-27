use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub reference: String,
    pub version: String,
    pub return_channel: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub key: String,
    /// Set only to adopt a deliberately selected existing provider object.
    pub remote_id: Option<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_id: Option<Uuid>,

    pub identity: Identity,
    pub name: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub team_ids: BTreeSet<Uuid>,
    pub owner_id: Option<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Milestone {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_date: Option<chrono::NaiveDate>,
    pub identity: Identity,
    pub project_key: String,
    pub name: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<BTreeSet<Uuid>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<chrono::NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_key: Option<String>,

    pub identity: Identity,
    pub project_key: String,
    pub milestone_key: Option<String>,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub team_id: Uuid,
    pub owner_id: Option<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub identity: Identity,
    pub blocking_issue_key: String,
    pub blocked_issue_key: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: u32,
    pub id: Uuid,
    pub revision: u64,
    pub source: Source,
    pub projects: Vec<Project>,
    pub milestones: Vec<Milestone>,
    pub issues: Vec<Issue>,
    pub dependencies: Vec<Dependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roadmap: Vec<crate::RoadmapRecord>,
    pub unresolved_questions: Vec<String>,
}

pub fn fingerprint<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("domain serialization cannot fail");
    format!("{:x}", Sha256::digest(encoded))
}
/// Deterministic client identifier formatted as UUID v4, as required by Linear.
pub fn stable_id(plan_id: Uuid, key: &str) -> Uuid {
    let hash = Sha256::digest([plan_id.as_bytes().as_slice(), key.as_bytes()].concat());
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
impl Plan {
    pub fn resources(&self) -> impl Iterator<Item = (&Identity, EntityKind)> {
        self.projects
            .iter()
            .map(|e| (&e.identity, EntityKind::Project))
            .chain(
                self.milestones
                    .iter()
                    .map(|e| (&e.identity, EntityKind::Milestone)),
            )
            .chain(self.issues.iter().map(|e| (&e.identity, EntityKind::Issue)))
            .chain(
                self.dependencies
                    .iter()
                    .map(|e| (&e.identity, EntityKind::Dependency)),
            )
            .chain(self.roadmap.iter().map(|e| (&e.identity, e.kind())))
    }
    pub fn digest(&self) -> String {
        fingerprint(self)
    }
    pub fn validate(&self) -> Result<()> {
        let invalid = |s: &str| Error::Validation(s.into());
        if self.schema_version != 1 || self.revision == 0 || self.id.is_nil() {
            return Err(invalid("expected schema 1, nonzero revision and plan ID"));
        }
        if [
            &self.source.reference,
            &self.source.version,
            &self.source.return_channel,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
        {
            return Err(invalid(
                "source reference, version and return channel are required",
            ));
        }
        if self.projects.is_empty() {
            return Err(invalid("at least one project required"));
        }
        let mut keys = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (identity, _) in self.resources() {
            if identity.key.trim().is_empty()
                || !keys.insert(&identity.key)
                || !ids.insert(
                    identity
                        .remote_id
                        .unwrap_or_else(|| stable_id(self.id, &identity.key)),
                )
            {
                return Err(invalid(
                    "entity keys and resolved IDs must be nonempty and unique",
                ));
            }
        }
        for p in &self.projects {
            if p.name.trim().is_empty()
                || p.team_ids.is_empty()
                || !criteria_valid(&p.acceptance_criteria)
            {
                return Err(invalid(
                    "projects require name, team and acceptance criteria",
                ));
            }
        }
        for m in &self.milestones {
            if m.name.trim().is_empty()
                || !criteria_valid(&m.acceptance_criteria)
                || !self
                    .projects
                    .iter()
                    .any(|p| p.identity.key == m.project_key)
            {
                return Err(invalid(
                    "invalid milestone project, name or acceptance criteria",
                ));
            }
        }
        let mut parents = BTreeMap::new();
        for i in &self.issues {
            if i.priority.is_some_and(|n| n > 4)
                || i.parent_key
                    .as_ref()
                    .is_some_and(|key| parents.get(key) != Some(&&i.project_key))
            {
                return Err(invalid(
                    "priority must be 0–4 and a parent issue must precede its child in the same project",
                ));
            }
            parents.insert(i.identity.key.clone(), &i.project_key);
            let project = self
                .projects
                .iter()
                .find(|p| p.identity.key == i.project_key)
                .ok_or_else(|| invalid("unknown issue project"))?;
            if i.title.trim().is_empty()
                || !criteria_valid(&i.acceptance_criteria)
                || !project.team_ids.contains(&i.team_id)
            {
                return Err(invalid(
                    "issue requires title, acceptance criteria and a project team",
                ));
            }
            if let Some(k) = &i.milestone_key
                && !self
                    .milestones
                    .iter()
                    .any(|m| &m.identity.key == k && m.project_key == i.project_key)
            {
                return Err(invalid("issue milestone must belong to issue project"));
            }
        }
        let issue_keys: BTreeSet<_> = self
            .issues
            .iter()
            .map(|i| i.identity.key.as_str())
            .collect();
        for record in &self.roadmap {
            record.validate(self)?;
        }
        let mut edges = BTreeSet::new();
        for d in &self.dependencies {
            if !issue_keys.contains(d.blocking_issue_key.as_str())
                || !issue_keys.contains(d.blocked_issue_key.as_str())
                || d.blocking_issue_key == d.blocked_issue_key
                || !edges.insert((&d.blocking_issue_key, &d.blocked_issue_key))
            {
                return Err(invalid("invalid or duplicate dependency"));
            }
        }
        // Kahn's algorithm rejects cycles without recursion or input-order dependence.
        let mut remaining = issue_keys;
        loop {
            let removable: Vec<_> = remaining
                .iter()
                .copied()
                .filter(|k| {
                    !edges
                        .iter()
                        .any(|(a, b)| b.as_str() == *k && remaining.contains(a.as_str()))
                })
                .collect();
            if removable.is_empty() {
                break;
            }
            for key in removable {
                remaining.remove(key);
            }
        }
        if !remaining.is_empty() {
            return Err(invalid("dependency cycle"));
        }
        Ok(())
    }
}
fn criteria_valid(criteria: &[String]) -> bool {
    !criteria.is_empty() && criteria.iter().all(|s| !s.trim().is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Milestone,
    Issue,
    Dependency,
    Initiative,
    InitiativeProject,
    ProjectDependency,
    ProjectUpdate,
    InitiativeUpdate,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEntity {
    pub id: Uuid,
    pub kind: EntityKind,
    /// Canonical managed fields only. Unmanaged fields are never written.
    pub fields: BTreeMap<String, Value>,
    pub updated_at: String,
}
pub type Baseline = BTreeMap<Uuid, RemoteEntity>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExistingResource {
    pub id: Uuid,
    pub kind: EntityKind,
    pub project_key: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Operator-approved stable plan identity; a source reference alone is insufficient.
    pub plan_id: Uuid,
    /// Explicit adoption bindings. Absence permits only this plan's derived IDs.
    pub existing_resources: BTreeMap<String, ExistingResource>,
    pub project_keys: BTreeSet<String>,
    pub team_ids: BTreeSet<Uuid>,
    pub create: bool,
    pub update: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authority {
    Agreed {
        reference: String,
        plan_digest: String,
        source_version: String,
        scope: Scope,
    },
    Standing {
        reference: String,
        source_reference: String,
        scope: Scope,
    },
}
impl Authority {
    fn scope(&self, plan: &Plan) -> Result<&Scope> {
        let (reference, scope) = match self {
            Self::Agreed {
                reference,
                plan_digest,
                source_version,
                scope,
            } if plan_digest == &plan.digest() && source_version == &plan.source.version => {
                (reference, scope)
            }
            Self::Standing {
                reference,
                source_reference,
                scope,
            } if source_reference == &plan.source.reference => (reference, scope),
            _ => {
                return Err(Error::Authority(
                    "agreement is for a different plan/source version".into(),
                ));
            }
        };
        if reference.trim().is_empty() {
            return Err(Error::Authority("authority reference required".into()));
        }
        if scope.plan_id != plan.id || scope.plan_id.is_nil() {
            return Err(Error::Authority(
                "plan identity exceeds approved scope".into(),
            ));
        }
        Ok(scope)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAction {
    Create,
    Update,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub kind: EntityKind,
    pub action: ChangeAction,
    pub project_key: String,
    pub desired: BTreeMap<String, Value>,
    pub patch: BTreeMap<String, Value>,
    pub baseline: Option<RemoteEntity>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedChangeSet {
    pub plan_digest: String,
    pub authority_digest: String,
    pub baseline: Baseline,
    pub steps: Vec<Step>,
}
fn description(body: &str, criteria: &[String], source: &Source) -> String {
    format!(
        "{}\n\nAcceptance criteria:\n{}\n\nSource: {} (version {})",
        body,
        criteria
            .iter()
            .map(|c| format!("- {c}"))
            .collect::<Vec<_>>()
            .join("\n"),
        source.reference,
        source.version
    )
}
/// Validate the entire operator-owned resource boundary before any network read.
/// Authority must come from trusted host configuration, never planner input.
pub fn authorize_plan(plan: &Plan, authority: &Authority) -> Result<()> {
    plan.validate()?;
    let scope = authority.scope(plan)?;
    let mut resources = Vec::new();
    for project in &plan.projects {
        if !project.team_ids.is_subset(&scope.team_ids) {
            return Err(Error::Authority("project teams exceed scope".into()));
        }
        resources.push((
            &project.identity,
            EntityKind::Project,
            project.identity.key.as_str(),
        ));
    }
    for milestone in &plan.milestones {
        resources.push((
            &milestone.identity,
            EntityKind::Milestone,
            milestone.project_key.as_str(),
        ));
    }
    for issue in &plan.issues {
        if !scope.team_ids.contains(&issue.team_id) {
            return Err(Error::Authority("issue team exceeds scope".into()));
        }
        resources.push((
            &issue.identity,
            EntityKind::Issue,
            issue.project_key.as_str(),
        ));
    }
    for dependency in &plan.dependencies {
        let blocking = plan
            .issues
            .iter()
            .find(|i| i.identity.key == dependency.blocking_issue_key)
            .expect("validated issue");
        let blocked = plan
            .issues
            .iter()
            .find(|i| i.identity.key == dependency.blocked_issue_key)
            .expect("validated issue");
        if !scope.project_keys.contains(&blocked.project_key) {
            return Err(Error::Authority(
                "dependency target project exceeds scope".into(),
            ));
        }
        resources.push((
            &dependency.identity,
            EntityKind::Dependency,
            blocking.project_key.as_str(),
        ));
    }
    resources.extend(
        plan.roadmap
            .iter()
            .map(|r| (&r.identity, r.kind(), r.project_key.as_str())),
    );
    for (identity, kind, project_key) in resources {
        if !scope.project_keys.contains(project_key) {
            return Err(Error::Authority("project exceeds scope".into()));
        }
        match (
            identity.remote_id,
            scope.existing_resources.get(&identity.key),
        ) {
            (None, None) => {}
            (Some(id), Some(binding))
                if id == binding.id
                    && kind == binding.kind
                    && project_key == binding.project_key
                    && !id.is_nil() => {}
            _ => {
                return Err(Error::Authority(format!(
                    "resource {} does not match operator-approved identity/kind/project binding",
                    identity.key
                )));
            }
        }
    }
    Ok(())
}

pub fn prepare(
    plan: &Plan,
    baseline: &Baseline,
    authority: &Authority,
) -> Result<PreparedChangeSet> {
    authorize_plan(plan, authority)?;
    if !plan.unresolved_questions.is_empty() {
        return Err(Error::Validation(
            "unresolved questions require clarification or handoff".into(),
        ));
    }
    let scope = authority.scope(plan)?;
    let ids: BTreeMap<_, _> = plan
        .resources()
        .map(|(identity, _)| identity)
        .map(|i| {
            (
                i.key.clone(),
                i.remote_id.unwrap_or_else(|| stable_id(plan.id, &i.key)),
            )
        })
        .collect();
    let mut entities = Vec::new();
    for p in &plan.projects {
        if !p.team_ids.is_subset(&scope.team_ids) {
            return Err(Error::Authority("project teams exceed scope".into()));
        }
        let mut value = json!({"name":p.name,"description":p.description,"content":description(&p.description,&p.acceptance_criteria,&plan.source),"teamIds":p.team_ids,"leadId":p.owner_id});
        if let Some(date) = p.start_date {
            value["startDate"] = json!(date);
        }
        if let Some(date) = p.target_date {
            value["targetDate"] = json!(date);
        }
        if let Some(status) = p.status_id {
            value["statusId"] = json!(status);
        }
        entities.push((
            &p.identity,
            EntityKind::Project,
            p.identity.key.clone(),
            value,
        ));
    }
    for m in &plan.milestones {
        let mut value = json!({"name":m.name,"description":description(&m.description,&m.acceptance_criteria,&plan.source),"projectId":ids[&m.project_key]});
        if let Some(date) = m.target_date {
            value["targetDate"] = json!(date);
        }
        entities.push((
            &m.identity,
            EntityKind::Milestone,
            m.project_key.clone(),
            value,
        ));
    }
    for i in &plan.issues {
        let mut value = json!({"title":i.title,"description":description(&i.description,&i.acceptance_criteria,&plan.source),"projectId":ids[&i.project_key],"projectMilestoneId":i.milestone_key.as_ref().map(|k| ids[k]),"teamId":i.team_id,"assigneeId":i.owner_id});
        if let Some(n) = i.priority {
            value["priority"] = json!(n);
        }
        if let Some(labels) = &i.label_ids {
            value["labelIds"] = json!(labels);
        }
        if let Some(state) = i.state_id {
            value["stateId"] = json!(state);
        }
        if let Some(cycle) = i.cycle_id {
            value["cycleId"] = json!(cycle);
        }
        if let Some(key) = &i.parent_key {
            value["parentId"] = json!(ids[key]);
        }
        if let Some(date) = i.due_date {
            value["dueDate"] = json!(date);
        }
        entities.push((&i.identity, EntityKind::Issue, i.project_key.clone(), value));
    }
    // Create initiatives before their project associations and progress updates.
    let mut roadmap: Vec<_> = plan.roadmap.iter().collect();
    roadmap.sort_by_key(|r| r.kind() != EntityKind::Initiative);
    for r in roadmap {
        entities.push((
            &r.identity,
            r.kind(),
            r.project_key.clone(),
            r.desired(&ids),
        ));
    }
    for d in &plan.dependencies {
        let project_key = plan
            .issues
            .iter()
            .find(|i| i.identity.key == d.blocking_issue_key)
            .expect("validated issue")
            .project_key
            .clone();
        let blocked_project = &plan
            .issues
            .iter()
            .find(|i| i.identity.key == d.blocked_issue_key)
            .expect("validated issue")
            .project_key;
        if !scope.project_keys.contains(blocked_project) {
            return Err(Error::Authority(
                "dependency target project exceeds scope".into(),
            ));
        }
        entities.push((&d.identity,EntityKind::Dependency,project_key,json!({"issueId":ids[&d.blocking_issue_key],"relatedIssueId":ids[&d.blocked_issue_key],"type":"blocks"})));
    }
    let mut steps = Vec::new();
    for (identity, kind, project_key, value) in entities {
        if !scope.project_keys.contains(&project_key) {
            return Err(Error::Authority("project exceeds scope".into()));
        }
        let id = ids[&identity.key];
        let desired: BTreeMap<_, _> = value
            .as_object()
            .expect("object literal")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let prior = baseline.get(&id);
        if identity.remote_id.is_some() && prior.is_none() {
            return Err(Error::Validation(
                "adopted objects require a retrieved baseline".into(),
            ));
        }
        if prior.is_some_and(|b| b.kind != kind || b.id != id) {
            return Err(Error::Validation("baseline kind/ID mismatch".into()));
        }
        let patch: BTreeMap<_, _> = desired
            .iter()
            .filter(|(k, v)| prior.is_none_or(|p| p.fields.get(*k) != Some(*v)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if prior.is_some()
            && match kind {
                EntityKind::InitiativeProject => !patch.is_empty(),
                EntityKind::ProjectUpdate => patch.contains_key("projectId"),
                EntityKind::InitiativeUpdate => patch.contains_key("initiativeId"),
                _ => false,
            }
        {
            return Err(Error::Conflict(
                "existing roadmap record belongs to different resources".into(),
            ));
        }
        if patch.is_empty() {
            continue;
        }
        let action = if prior.is_some() {
            ChangeAction::Update
        } else {
            ChangeAction::Create
        };
        if (action == ChangeAction::Create && !scope.create)
            || (action == ChangeAction::Update && !scope.update)
        {
            return Err(Error::Authority("mutation kind exceeds scope".into()));
        }
        let step_id = stable_id(
            plan.id,
            &format!("{}:{}:{}", plan.digest(), identity.key, fingerprint(&patch)),
        );
        steps.push(Step {
            id: step_id,
            entity_id: id,
            kind,
            action,
            project_key,
            desired,
            patch,
            baseline: prior.cloned(),
        });
    }
    Ok(PreparedChangeSet {
        plan_digest: plan.digest(),
        authority_digest: fingerprint(authority),
        baseline: baseline.clone(),
        steps,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationSession {
    pub rounds: u32,
    pub max_rounds: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Clarification {
    Ready {
        source: Source,
        plan_digest: String,
    },
    Ask {
        source: Source,
        questions: Vec<String>,
        round: u32,
    },
    Handoff {
        source: Source,
        unresolved_questions: Vec<String>,
        reason: String,
    },
}
impl ClarificationSession {
    /// Caller persists the session and amended versioned plan before delivering messages.
    pub fn next(&mut self, plan: &Plan) -> Result<Clarification> {
        plan.validate()?;
        if plan.unresolved_questions.is_empty() {
            return Ok(Clarification::Ready {
                source: plan.source.clone(),
                plan_digest: plan.digest(),
            });
        }
        if self.rounds >= self.max_rounds {
            return Ok(Clarification::Handoff {
                source: plan.source.clone(),
                unresolved_questions: plan.unresolved_questions.clone(),
                reason: "clarification round budget exhausted".into(),
            });
        }
        self.rounds += 1;
        Ok(Clarification::Ask {
            source: plan.source.clone(),
            questions: plan.unresolved_questions.clone(),
            round: self.rounds,
        })
    }
}
