use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepPhase {
    Prepared,
    Attempted,
    Verified,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepReceipt {
    pub step_digest: String,
    pub phase: StepPhase,
    pub observed: Option<RemoteEntity>,
}
/// Host implementation must durably commit receipts before returning, enforce
/// monotonic phases, serialize executions, and fence stale task/plan owners.
/// The authorization hook is mandatory and must check current authority/revocation.
pub trait StepJournal {
    fn load(&mut self, step_id: Uuid) -> impl Future<Output = Result<Option<StepReceipt>>> + Send;
    fn save(
        &mut self,
        step_id: Uuid,
        receipt: &StepReceipt,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Mandatory live read admission, including prewrite checks, readback and
    /// resume reconciliation. Host must meter this read and enforce current
    /// resource scope, task fencing, deadline and mandate/ancestor revocation.
    fn authorize_read(
        &mut self,
        kind: EntityKind,
        id: Uuid,
    ) -> impl Future<Output = Result<()>> + Send;
    fn authorize_step(
        &mut self,
        changes: &PreparedChangeSet,
        step: &Step,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Read-only observation, separate from the managed fields used to prepare writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionSnapshot {
    pub entity: RemoteEntity,
    pub url: Option<String>,
    pub workflow_state: Option<ObservedWorkflowState>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedWorkflowState {
    pub id: Uuid,
    pub name: String,
    pub category: String,
}

/// HTTP adapter has no credential discovery, implicit retry, or live default.
/// Supply `https://api.linear.app/graphql` and a raw API key or `Bearer ...`
/// authorization header explicitly. Debug deliberately excludes credentials.
pub struct LinearClient {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    authorization: reqwest::header::HeaderValue,
}
impl LinearClient {
    pub fn new(endpoint: &str, authorization: &str, timeout: Duration) -> Result<Self> {
        let endpoint = reqwest::Url::parse(endpoint)
            .map_err(|_| Error::Provider("invalid endpoint".into()))?;
        if endpoint.scheme() != "https"
            && !(endpoint.scheme() == "http"
                && endpoint
                    .host_str()
                    .is_some_and(|h| ["localhost", "127.0.0.1", "[::1]"].contains(&h)))
        {
            return Err(Error::Provider(
                "HTTPS required except on loopback mock servers".into(),
            ));
        }
        if !endpoint.username().is_empty() || endpoint.password().is_some() {
            return Err(Error::Provider(
                "endpoint must not contain credentials".into(),
            ));
        }
        let mut authorization = reqwest::header::HeaderValue::from_str(authorization)
            .map_err(|_| Error::Provider("invalid authorization header".into()))?;
        authorization.set_sensitive(true);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Provider("HTTP client setup failed".into()))?;
        Ok(Self {
            client,
            endpoint,
            authorization,
        })
    }
    async fn graphql(
        &self,
        query: &str,
        variables: Value,
        allow_absent: bool,
    ) -> Result<Option<Value>> {
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .json(&json!({"query":query,"variables":variables}))
            .send()
            .await
            .map_err(|_| Error::Provider("transport error; reconcile before retry".into()))?;
        if !response.status().is_success() {
            return Err(Error::Provider(format!(
                "HTTP {}; reconcile before retry",
                response.status().as_u16()
            )));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Provider("incomplete response; reconcile before retry".into()))?
        {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(Error::Provider("response exceeds 8 MiB".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Provider("invalid JSON response".into()))?;
        if let Some(errors) = body
            .get("errors")
            .and_then(Value::as_array)
            .filter(|e| !e.is_empty())
        {
            if allow_absent
                && errors.iter().all(|e| {
                    e.pointer("/extensions/code").and_then(Value::as_str)
                        == Some("ENTITY_NOT_FOUND")
                        || e.pointer("/extensions/type").and_then(Value::as_str)
                            == Some("not found")
                })
            {
                return Ok(None);
            }
            // Do not log error messages that can echo input or credentials.
            return Err(Error::Provider(
                "GraphQL errors; no effects assumed, reconcile before retry".into(),
            ));
        }
        body.get("data")
            .cloned()
            .map(Some)
            .ok_or_else(|| Error::Provider("missing GraphQL data".into()))
    }
    pub async fn retrieve(&self, kind: EntityKind, id: Uuid) -> Result<Option<RemoteEntity>> {
        let query = format!(
            "query AgencyRead($id: String!) {{ object: {}(id: $id) {{ {} }} }}",
            query_name(kind),
            selection(kind)
        );
        let Some(data) = self.graphql(&query, json!({"id":id}), true).await? else {
            return Ok(None);
        };
        let object = data
            .get("object")
            .ok_or_else(|| Error::Provider("missing read object".into()))?;
        if object.is_null() {
            return Ok(None);
        }
        let observed = normalize(kind, object)?;
        if observed.id != id {
            return Err(Error::Provider("read returned another object ID".into()));
        }
        Ok(Some(observed))
    }
    /// Observe one already-authorized resource. No workspace search, related-object
    /// expansion, mutations, guessed links, or forecasting occurs here.
    pub async fn inspect(&self, kind: EntityKind, id: Uuid) -> Result<Option<InspectionSnapshot>> {
        let extra = match kind {
            EntityKind::Project => "url status { id name type }",
            EntityKind::Issue => "url state { id name type }",
            _ => "",
        };
        let query = format!(
            "query AgencyInspect($id: String!) {{ object: {}(id: $id) {{ {} {} }} }}",
            query_name(kind),
            selection(kind),
            extra
        );
        let Some(data) = self.graphql(&query, json!({"id":id}), true).await? else {
            return Ok(None);
        };
        let object = data
            .get("object")
            .ok_or_else(|| Error::Provider("missing inspection object".into()))?;
        if object.is_null() {
            return Ok(None);
        }
        let entity = normalize(kind, object)?;
        if entity.id != id {
            return Err(Error::Provider(
                "inspection returned another object ID".into(),
            ));
        }
        let state_key = match kind {
            EntityKind::Project => Some("status"),
            EntityKind::Issue => Some("state"),
            _ => None,
        };
        let (url, workflow_state) = if let Some(state_key) = state_key {
            let link = object
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Provider("inspection URL missing".into()))?;
            let parsed = reqwest::Url::parse(link)
                .map_err(|_| Error::Provider("invalid inspection URL".into()))?;
            if parsed.scheme() != "https"
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.host_str().is_none()
            {
                return Err(Error::Provider("invalid inspection URL".into()));
            }
            let state = object
                .get(state_key)
                .ok_or_else(|| Error::Provider("inspection workflow state missing".into()))?;
            let get = |key| {
                state
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| Error::Provider("invalid inspection workflow state".into()))
            };
            let state_id = Uuid::parse_str(get("id")?)
                .map_err(|_| Error::Provider("invalid workflow state UUID".into()))?;
            if state_id.is_nil() {
                return Err(Error::Provider("invalid workflow state UUID".into()));
            }
            (
                Some(link.to_owned()),
                Some(ObservedWorkflowState {
                    id: state_id,
                    name: get("name")?.into(),
                    category: get("type")?.into(),
                }),
            )
        } else {
            (None, None)
        };
        Ok(Some(InspectionSnapshot {
            entity,
            url,
            workflow_state,
        }))
    }

    /// Retrieve only explicitly selected plan IDs, including deterministic new IDs.
    pub async fn retrieve_plan(&self, plan: &Plan) -> Result<Baseline> {
        plan.validate()?;
        let entities = plan.resources();
        let mut baseline = Baseline::new();
        for (identity, kind) in entities {
            let id = identity
                .remote_id
                .unwrap_or_else(|| stable_id(plan.id, &identity.key));
            if let Some(entity) = self.retrieve(kind, id).await? {
                baseline.insert(id, entity);
            }
        }
        Ok(baseline)
    }
    /// Sequential effects stop at first failure. Resume with the *same* changes
    /// and durable journal; completed steps are reconciled by readback.
    pub async fn execute<J: StepJournal>(
        &self,
        plan: &Plan,
        authority: &Authority,
        changes: &PreparedChangeSet,
        journal: &mut J,
    ) -> Result<Vec<RemoteEntity>> {
        if prepare(plan, &changes.baseline, authority)? != *changes {
            return Err(Error::StalePreparation);
        }
        let mut results = Vec::new();
        for step in &changes.steps {
            let digest = fingerprint(step);
            let mut receipt = journal.load(step.id).await?;
            if receipt.as_ref().is_some_and(|r| r.step_digest != digest) {
                return Err(Error::Journal("step binding mismatch".into()));
            }
            if receipt.is_none() {
                let prepared = StepReceipt {
                    step_digest: digest.clone(),
                    phase: StepPhase::Prepared,
                    observed: None,
                };
                journal.save(step.id, &prepared).await?;
                receipt = Some(prepared);
            }
            journal.authorize_read(step.kind, step.entity_id).await?;
            let current = self.retrieve(step.kind, step.entity_id).await?;
            if let Some(entity) = current
                .as_ref()
                .filter(|entity| matches_desired(entity, &step.desired))
            {
                let verified = StepReceipt {
                    step_digest: digest,
                    phase: StepPhase::Verified,
                    observed: Some(entity.clone()),
                };
                if !receipt
                    .as_ref()
                    .is_some_and(|r| r.phase == StepPhase::Verified)
                {
                    journal.save(step.id, &verified).await?;
                }
                results.push(entity.clone());
                continue;
            }
            if receipt
                .as_ref()
                .is_some_and(|r| r.phase == StepPhase::Verified)
            {
                return Err(Error::Conflict(format!(
                    "verified object {} was subsequently changed or removed",
                    step.entity_id
                )));
            }
            match (&step.action, &current, &step.baseline) {
                (ChangeAction::Create, None, None) => {}
                (ChangeAction::Update, Some(current), Some(baseline)) if current == baseline => {
                    if receipt
                        .as_ref()
                        .is_some_and(|r| r.phase == StepPhase::Attempted)
                    {
                        return Err(Error::Conflict(format!(
                            "update {} has uncertain outcome; explicit operator reconciliation required",
                            step.entity_id
                        )));
                    }
                }
                _ => {
                    return Err(Error::Conflict(format!(
                        "object {} differs from reviewed baseline",
                        step.entity_id
                    )));
                }
            }
            // Host checks current policy/revision and task fencing immediately
            // before attempting the mutation. A failed save must prevent sending.
            journal.authorize_step(changes, step).await?;
            let attempted = StepReceipt {
                step_digest: digest.clone(),
                phase: StepPhase::Attempted,
                observed: None,
            };
            journal.save(step.id, &attempted).await?;
            self.mutate(step).await?;
            journal.authorize_read(step.kind, step.entity_id).await?;
            let entity = self
                .retrieve(step.kind, step.entity_id)
                .await?
                .ok_or_else(|| {
                    Error::Provider("write not visible on readback; reconcile on resume".into())
                })?;
            if !matches_desired(&entity, &step.desired) {
                return Err(Error::Conflict(format!(
                    "readback differs for {}",
                    step.entity_id
                )));
            }
            journal
                .save(
                    step.id,
                    &StepReceipt {
                        step_digest: digest,
                        phase: StepPhase::Verified,
                        observed: Some(entity.clone()),
                    },
                )
                .await?;
            results.push(entity);
        }
        Ok(results)
    }
    /// A scoped lookup never searches the whole workspace. Callers must clarify
    /// duplicate titles instead of choosing the first issue from the connection.
    pub async fn find_issue_ids(
        &self,
        project: Uuid,
        team: Uuid,
        title: &str,
    ) -> Result<Vec<Uuid>> {
        if title.trim().is_empty() {
            return Err(Error::Validation("issue title required".into()));
        }
        let query = "query AgencyFindIssue($filter: IssueFilter!) { issues(first: 2, filter: $filter) { nodes { id project { id } team { id } } } }";
        let data = self.graphql(query, json!({"filter":{"project":{"id":{"eq":project}},"team":{"id":{"eq":team}},"title":{"eqIgnoreCase":title}}}), false).await?
            .ok_or_else(|| Error::Provider("missing issue search".into()))?;
        let rows = data
            .pointer("/issues/nodes")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() <= 2)
            .ok_or_else(|| Error::Provider("invalid issue search".into()))?;
        rows.iter()
            .map(|row| {
                if row["project"]["id"] != json!(project) || row["team"]["id"] != json!(team) {
                    return Err(Error::Provider(
                        "issue search returned a different scope".into(),
                    ));
                }
                serde_json::from_value(row["id"].clone())
                    .map_err(|_| Error::Provider("invalid issue search identity".into()))
            })
            .collect()
    }

    pub async fn scoped_issue(
        &self,
        project: Uuid,
        team: Uuid,
        id: Uuid,
    ) -> Result<Option<RemoteEntity>> {
        let query = format!(
            "query AgencyScopedIssue($filter: IssueFilter!) {{ issues(first: 1, filter: $filter) {{ nodes {{ {} }} }} }}",
            selection(EntityKind::Issue)
        );
        let data = self.graphql(&query, json!({"filter":{"project":{"id":{"eq":project}},"team":{"id":{"eq":team}},"id":{"eq":id}}}),false).await?
            .ok_or_else(|| Error::Provider("missing issue connection".into()))?;
        let nodes = data
            .pointer("/issues/nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Provider("invalid scoped issue response".into()))?;
        match nodes.as_slice() {
            [] => Ok(None),
            [node]
                if node["id"] == json!(id)
                    && node["team"]["id"] == json!(team)
                    && node["project"]["id"] == json!(project) =>
            {
                Ok(Some(normalize(EntityKind::Issue, node)?))
            }
            _ => Err(Error::Provider("inconsistent scoped issue response".into())),
        }
    }

    pub async fn write_tracking_issue(
        &self,
        id: Uuid,
        existing: bool,
        project: Uuid,
        team: Uuid,
        fields: &BTreeMap<String, Value>,
    ) -> Result<()> {
        let mut fields = json!(fields);
        if !existing {
            fields["projectId"] = json!(project);
            fields["teamId"] = json!(team);
        }
        self.write_fields(EntityKind::Issue, id, !existing, fields)
            .await
    }

    async fn mutate(&self, step: &Step) -> Result<()> {
        self.write_fields(
            step.kind,
            step.entity_id,
            step.action == ChangeAction::Create,
            serde_json::to_value(&step.patch).expect("JSON map"),
        )
        .await
    }

    async fn write_fields(
        &self,
        kind: EntityKind,
        id: Uuid,
        create: bool,
        mut input: Value,
    ) -> Result<()> {
        let prefix = query_name(kind);
        let payload = prefix;
        let input_type = match kind {
            EntityKind::Project => "Project",
            EntityKind::Milestone => "ProjectMilestone",
            EntityKind::Issue => "Issue",
            EntityKind::Dependency => "IssueRelation",
            EntityKind::Initiative => "Initiative",
            EntityKind::InitiativeProject => "InitiativeToProject",
            EntityKind::ProjectDependency => "ProjectRelation",
            EntityKind::ProjectUpdate => "ProjectUpdate",
            EntityKind::InitiativeUpdate => "InitiativeUpdate",
        };
        let action = if create { "Create" } else { "Update" };
        if create {
            input["id"] = json!(id);
        }
        // Templates could modify fields and create auxiliary effects; disable them.
        if create && matches!(kind, EntityKind::Project | EntityKind::Issue) {
            input["useDefaultTemplate"] = json!(false);
        }
        let (declaration, arguments) = if create {
            ("", "")
        } else {
            (", $id: String!", ", id: $id")
        };
        let query = format!(
            "mutation AgencyWrite($input: {input_type}{action}Input!{declaration}) {{ effect: {prefix}{action}(input: $input{arguments}) {{ success {payload} {{ id }} }} }}"
        );
        let data = self
            .graphql(&query, json!({"input":input,"id":id}), false)
            .await?
            .ok_or_else(|| Error::Provider("mutation response missing".into()))?;
        if data.pointer("/effect/success").and_then(Value::as_bool) != Some(true)
            || data
                .pointer(&format!("/effect/{payload}/id"))
                .and_then(Value::as_str)
                != Some(&id.to_string())
        {
            return Err(Error::Provider(
                "mutation not confirmed; reconcile before retry".into(),
            ));
        }
        Ok(())
    }
}
fn matches_desired(entity: &RemoteEntity, desired: &BTreeMap<String, Value>) -> bool {
    desired.iter().all(|(k, v)| entity.fields.get(k) == Some(v))
}
fn query_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Project => "project",
        EntityKind::Milestone => "projectMilestone",
        EntityKind::Issue => "issue",
        EntityKind::Dependency => "issueRelation",
        EntityKind::Initiative => "initiative",
        EntityKind::InitiativeProject => "initiativeToProject",
        EntityKind::ProjectDependency => "projectRelation",
        EntityKind::ProjectUpdate => "projectUpdate",
        EntityKind::InitiativeUpdate => "initiativeUpdate",
    }
}
fn selection(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Project => {
            "id updatedAt name description content startDate targetDate status { id } lead { id } teams(first: 100) { nodes { id } pageInfo { hasNextPage } }"
        }
        EntityKind::Milestone => "id updatedAt name description targetDate project { id }",
        EntityKind::Issue => {
            "id updatedAt title description url priority labelIds dueDate state { id } cycle { id } parent { id } project { id } projectMilestone { id } team { id } assignee { id }"
        }
        EntityKind::Dependency => "id updatedAt type issue { id } relatedIssue { id }",
        EntityKind::Initiative => "id updatedAt name description owner { id } status targetDate",
        EntityKind::InitiativeProject => "id updatedAt initiative { id } project { id }",
        EntityKind::ProjectDependency => {
            "id updatedAt type anchorType relatedAnchorType project { id } relatedProject { id }"
        }
        EntityKind::ProjectUpdate => "id updatedAt body health project { id }",
        EntityKind::InitiativeUpdate => "id updatedAt body health initiative { id }",
    }
}
fn normalize(kind: EntityKind, object: &Value) -> Result<RemoteEntity> {
    let string = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Provider(format!("readback missing {key}")))
    };
    let id = Uuid::parse_str(string("id")?)
        .map_err(|_| Error::Provider("invalid remote UUID".into()))?;
    let mut fields = BTreeMap::new();
    let mut copy = |key: &str| -> Result<()> {
        let value = object
            .get(key)
            .ok_or_else(|| Error::Provider(format!("readback missing {key}")))?;
        if !value.is_null() && !value.is_string() {
            return Err(Error::Provider(format!("unexpected {key} type")));
        }
        fields.insert(key.into(), value.clone());
        Ok(())
    };
    match kind {
        EntityKind::Project => {
            copy("name")?;
            copy("description")?;
            copy("content")?;
        }
        EntityKind::Milestone => {
            copy("name")?;
            copy("description")?;
        }
        EntityKind::Issue => {
            copy("title")?;
            copy("description")?;
        }
        EntityKind::Dependency => copy("type")?,
        EntityKind::Initiative => {
            copy("name")?;
            copy("description")?;
        }
        EntityKind::InitiativeProject => {}
        EntityKind::ProjectDependency => {
            copy("type")?;
            copy("anchorType")?;
            copy("relatedAnchorType")?;
        }
        EntityKind::ProjectUpdate | EntityKind::InitiativeUpdate => {
            copy("body")?;
            copy("health")?;
        }
    }
    let relations: &[(&str, &str)] = match kind {
        EntityKind::Project => &[("lead", "leadId")],
        EntityKind::Milestone => &[("project", "projectId")],
        EntityKind::Issue => &[
            ("project", "projectId"),
            ("projectMilestone", "projectMilestoneId"),
            ("team", "teamId"),
            ("assignee", "assigneeId"),
        ],
        EntityKind::Dependency => &[("issue", "issueId"), ("relatedIssue", "relatedIssueId")],
        EntityKind::Initiative => &[],
        EntityKind::InitiativeProject => {
            &[("initiative", "initiativeId"), ("project", "projectId")]
        }
        EntityKind::ProjectDependency => &[
            ("project", "projectId"),
            ("relatedProject", "relatedProjectId"),
        ],
        EntityKind::ProjectUpdate => &[("project", "projectId")],
        EntityKind::InitiativeUpdate => &[("initiative", "initiativeId")],
    };
    for (key, field) in relations {
        let relation = object
            .get(key)
            .ok_or_else(|| Error::Provider(format!("readback missing {key}")))?;
        let value = if relation.is_null() {
            Value::Null
        } else {
            let id = relation
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Provider("missing relation ID".into()))?;
            Uuid::parse_str(id).map_err(|_| Error::Provider("invalid relation UUID".into()))?;
            json!(id)
        };
        fields.insert((*field).into(), value);
    }
    let metadata: &[&str] = match kind {
        EntityKind::Project => &["startDate", "targetDate"],
        EntityKind::Milestone => &["targetDate"],
        EntityKind::Issue => &["dueDate"],
        EntityKind::Initiative => &["status", "targetDate"],
        _ => &[],
    };
    for &name in metadata {
        if let Some(value) = object.get(name) {
            fields.insert(name.into(), value.clone());
        }
    }
    let optional_relations: &[(&str, &str)] = match kind {
        EntityKind::Project => &[("status", "statusId")],
        EntityKind::Issue => &[
            ("parent", "parentId"),
            ("cycle", "cycleId"),
            ("state", "stateId"),
        ],
        EntityKind::Initiative => &[("owner", "ownerId")],
        _ => &[],
    };
    for &(key, field) in optional_relations {
        if let Some(value) = object.get(key) {
            let id: Option<Uuid> = if value.is_null() {
                None
            } else {
                Some(
                    serde_json::from_value(value["id"].clone())
                        .map_err(|_| Error::Provider("invalid metadata relation ID".into()))?,
                )
            };
            fields.insert(field.into(), json!(id));
        }
    }
    if kind == EntityKind::Issue {
        if let Some(priority) = object.get("priority") {
            let n = priority
                .as_f64()
                .filter(|n| n.fract() == 0.0 && (0.0..=4.0).contains(n))
                .ok_or_else(|| Error::Provider("invalid issue priority".into()))?;
            fields.insert("priority".into(), json!(n as u8));
        }
        if let Some(labels) = object.get("labelIds") {
            let ids: std::collections::BTreeSet<Uuid> = serde_json::from_value(labels.clone())
                .map_err(|_| Error::Provider("invalid label IDs".into()))?;
            fields.insert("labelIds".into(), json!(ids));
        }
        if let Some(link) = object.get("url").and_then(Value::as_str) {
            let parsed = reqwest::Url::parse(link)
                .map_err(|_| Error::Provider("invalid issue link".into()))?;
            if parsed.scheme() != "https"
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(Error::Provider("invalid issue link".into()));
            }
            fields.insert("url".into(), json!(link));
        }
    }
    if kind == EntityKind::Project {
        if object
            .pointer("/teams/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            != Some(false)
        {
            return Err(Error::Provider(
                "team list incomplete; refusing truncated baseline".into(),
            ));
        }
        let nodes = object
            .pointer("/teams/nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Provider("missing teams".into()))?;
        let mut ids = std::collections::BTreeSet::new();
        for node in nodes {
            let id = node
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Provider("missing team ID".into()))?;
            ids.insert(
                Uuid::parse_str(id).map_err(|_| Error::Provider("invalid team UUID".into()))?,
            );
        }
        fields.insert("teamIds".into(), json!(ids));
    }
    Ok(RemoteEntity {
        id,
        kind,
        fields,
        updated_at: string("updatedAt")?.into(),
    })
}
