#![allow(clippy::unwrap_used)]
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::post};
use codifix_agency_projects::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

fn plan() -> Plan {
    serde_json::from_str(include_str!("../examples/plan.json")).unwrap()
}
fn authority(plan: &Plan) -> Authority {
    Authority::Agreed {
        reference: "approval://example/1".into(),
        plan_digest: plan.digest(),
        source_version: plan.source.version.clone(),
        scope: Scope {
            plan_id: plan.id,
            existing_resources: BTreeMap::new(),
            project_keys: plan
                .projects
                .iter()
                .map(|p| p.identity.key.clone())
                .collect(),
            team_ids: plan
                .projects
                .iter()
                .flat_map(|p| p.team_ids.iter().copied())
                .collect(),
            create: true,
            update: true,
        },
    }
}
#[derive(Default, Clone)]
struct Journal {
    receipts: BTreeMap<Uuid, StepReceipt>,
    writes: usize,
    reads: Vec<(EntityKind, Uuid)>,
    deny_read_at: Option<usize>,
    deny_at: Option<usize>,
    fail_phase: Option<StepPhase>,
}
impl StepJournal for Journal {
    async fn load(&mut self, id: Uuid) -> Result<Option<StepReceipt>> {
        Ok(self.receipts.get(&id).cloned())
    }
    async fn save(&mut self, id: Uuid, receipt: &StepReceipt) -> Result<()> {
        if self.fail_phase == Some(receipt.phase) {
            return Err(Error::Journal("mock persistence failure".into()));
        }
        if let Some(prior) = self.receipts.get(&id) {
            assert_eq!(prior.step_digest, receipt.step_digest);
            let rank = |p| match p {
                StepPhase::Prepared => 0,
                StepPhase::Attempted => 1,
                StepPhase::Verified => 2,
            };
            assert!(rank(receipt.phase) >= rank(prior.phase));
        }
        self.receipts.insert(id, receipt.clone());
        Ok(())
    }
    async fn authorize_read(&mut self, kind: EntityKind, id: Uuid) -> Result<()> {
        self.reads.push((kind, id));
        if self.deny_read_at == Some(self.reads.len()) {
            return Err(Error::Authority(
                "read authorization revoked or exhausted".into(),
            ));
        }
        Ok(())
    }
    async fn authorize_step(&mut self, _changes: &PreparedChangeSet, _step: &Step) -> Result<()> {
        self.writes += 1;
        if self.deny_at == Some(self.writes) {
            return Err(Error::Authority("revoked".into()));
        }
        Ok(())
    }
}
#[derive(Default)]
struct ServerState {
    reads: Vec<Uuid>,
    objects: BTreeMap<Uuid, Value>,
    mutations: Vec<Value>,
    fail_before_at: Option<usize>,
    fail_after_at: Option<usize>,
    timeout_after_at: Option<usize>,
    malformed_readback: bool,
    wrong_id: bool,
    graphql_read_error: bool,
}
struct Mock {
    state: Arc<Mutex<ServerState>>,
    endpoint: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Mock {
    async fn start() -> Self {
        let state = Arc::new(Mutex::new(ServerState::default()));
        let app = Router::new()
            .route("/graphql", post(handle))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            state,
            endpoint,
            task,
        }
    }
    fn client(&self) -> LinearClient {
        LinearClient::new(&self.endpoint, "fake-test-key", Duration::from_millis(100)).unwrap()
    }
    fn writes(&self) -> usize {
        self.state.lock().unwrap().mutations.len()
    }
}
async fn handle(
    State(state): State<Arc<Mutex<ServerState>>>,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let query = body["query"].as_str().unwrap();
    let mut delay = false;
    let response = {
        let mut state = state.lock().unwrap();
        if query.starts_with("query") {
            let id = Uuid::parse_str(body["variables"]["id"].as_str().unwrap()).unwrap();
            state.reads.push(id);
            if state.graphql_read_error {
                return Json(json!({"errors":[{"message":"permission denied","extensions":{"code":"FORBIDDEN"}}]})).into_response();
            }
            if let Some(object) = state.objects.get(&id) {
                let mut object = object.clone();
                if state.malformed_readback {
                    object.as_object_mut().unwrap().remove("updatedAt");
                }
                if state.wrong_id {
                    object["id"] = json!(Uuid::new_v4());
                }
                return Json(json!({"data":{"object":object}})).into_response();
            }
            return Json(json!({"errors":[{"message":"not found","extensions":{"code":"ENTITY_NOT_FOUND"}}]})).into_response();
        }
        state.mutations.push(body.clone());
        let number = state.mutations.len();
        if state.fail_before_at == Some(number) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        let input = &body["variables"]["input"];
        let create = query.contains("CreateInput");
        let id = Uuid::parse_str(if create {
            input["id"].as_str().unwrap()
        } else {
            body["variables"]["id"].as_str().unwrap()
        })
        .unwrap();
        if create && state.objects.contains_key(&id) {
            return Json(json!({"errors":[{"extensions":{"code":"DUPLICATE"}}]})).into_response();
        }
        let payload = query
            .split("success ")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let mut object = state
            .objects
            .get(&id)
            .cloned()
            .unwrap_or_else(|| json!({"id":id}));
        object["updatedAt"] = json!(format!("2026-10-31T12:00:{number:02}Z"));
        for (key, value) in input.as_object().unwrap() {
            let relation = match key.as_str() {
                "relatedProjectId" => Some("relatedProject"),
                "initiativeId" => Some("initiative"),
                "ownerId" => Some("owner"),
                "cycleId" => Some("cycle"),
                "parentId" => Some("parent"),
                "stateId" => Some("state"),
                "statusId" => Some("status"),
                "projectId" => Some("project"),
                "projectMilestoneId" => Some("projectMilestone"),
                "teamId" => Some("team"),
                "leadId" => Some("lead"),
                "assigneeId" => Some("assignee"),
                "issueId" => Some("issue"),
                "relatedIssueId" => Some("relatedIssue"),
                _ => None,
            };
            if let Some(relation) = relation {
                object[relation] = if value.is_null() {
                    Value::Null
                } else {
                    json!({"id":value})
                };
            } else if key == "teamIds" {
                object["teams"] = json!({"nodes":value.as_array().unwrap().iter().map(|id| json!({"id":id})).collect::<Vec<_>>(),"pageInfo":{"hasNextPage":false}});
            } else {
                object[key] = value.clone();
            }
        }
        state.objects.insert(id, object);
        if state.fail_after_at == Some(number) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        if state.timeout_after_at == Some(number) {
            delay = true;
        }
        Json(json!({"data":{"effect":{"success":true,payload:{"id":id}}}})).into_response()
    };
    if delay {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    response
}
async fn seed(mock: &Mock, p: &Plan) -> Baseline {
    let changes = prepare(p, &Baseline::new(), &authority(p)).unwrap();
    mock.client()
        .execute(p, &authority(p), &changes, &mut Journal::default())
        .await
        .unwrap();
    mock.client().retrieve_plan(p).await.unwrap()
}

#[test]
fn example_validates_and_renders_source_acceptance_and_ordered_dependencies() {
    let p = plan();
    p.validate().unwrap();
    let changes = prepare(&p, &Baseline::new(), &authority(&p)).unwrap();
    assert_eq!(
        changes,
        prepare(&p, &Baseline::new(), &authority(&p)).unwrap()
    );
    assert_eq!(
        changes.steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            EntityKind::Project,
            EntityKind::Milestone,
            EntityKind::Issue,
            EntityKind::Issue,
            EntityKind::Dependency
        ]
    );
    assert!(
        changes.steps[2].desired["description"]
            .as_str()
            .unwrap()
            .contains(&p.source.reference)
    );
    assert!(
        changes.steps[2].desired["description"]
            .as_str()
            .unwrap()
            .contains(&p.issues[0].acceptance_criteria[0])
    );
    assert_eq!(changes.steps[4].desired["type"], "blocks");
    assert_eq!(stable_id(p.id, "build").get_version_num(), 4);
}
#[test]
fn validation_rejects_cycles_duplicate_keys_wrong_milestones_and_missing_criteria() {
    let p = plan();
    let mut changed = p.clone();
    changed.dependencies.push(Dependency {
        identity: Identity {
            key: "cycle".into(),
            remote_id: None,
        },
        blocking_issue_key: "review".into(),
        blocked_issue_key: "build".into(),
    });
    assert!(changed.validate().is_err());
    let mut changed = p.clone();
    changed.issues[1].identity.key = "build".into();
    assert!(changed.validate().is_err());
    let mut changed = p.clone();
    changed.issues[0].milestone_key = Some("unknown".into());
    assert!(changed.validate().is_err());
    let mut changed = p;
    changed.projects[0].acceptance_criteria.clear();
    assert!(changed.validate().is_err());
}
#[test]
fn agreement_and_scopes_are_exact_and_clarification_is_bounded() {
    let mut p = plan();
    let agreed = authority(&p);
    p.revision += 1;
    assert!(matches!(
        prepare(&p, &Baseline::new(), &agreed),
        Err(Error::Authority(_))
    ));
    let mut agreed = authority(&p);
    if let Authority::Agreed { scope, .. } = &mut agreed {
        scope.create = false;
    }
    assert!(matches!(
        prepare(&p, &Baseline::new(), &agreed),
        Err(Error::Authority(_))
    ));
    p.unresolved_questions = vec!["Who owns launch approval?".into()];
    let mut session = ClarificationSession {
        rounds: 0,
        max_rounds: 1,
    };
    assert!(matches!(
        session.next(&p).unwrap(),
        Clarification::Ask { round: 1, .. }
    ));
    assert!(matches!(
        session.next(&p).unwrap(),
        Clarification::Handoff { .. }
    ));
    assert!(prepare(&p, &Baseline::new(), &authority(&p)).is_err());
    p.unresolved_questions.clear();
    assert!(matches!(
        session.next(&p).unwrap(),
        Clarification::Ready { .. }
    ));
}
#[tokio::test]
async fn actual_http_creates_all_types_reads_back_and_resumes_without_duplicates() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    let result = mock
        .client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(result.len(), 5);
    assert_eq!(mock.writes(), 5);
    assert!(
        journal
            .receipts
            .values()
            .all(|r| r.phase == StepPhase::Verified)
    );
    // Simulate process restart from durable, serialized receipt records.
    let durable = serde_json::to_vec(&journal.receipts).unwrap();
    let mut restarted = Journal {
        receipts: serde_json::from_slice(&durable).unwrap(),
        ..Default::default()
    };
    assert_eq!(
        result,
        mock.client()
            .execute(&p, &auth, &changes, &mut restarted)
            .await
            .unwrap()
    );
    assert_eq!(mock.writes(), 5);
    let baseline = mock.client().retrieve_plan(&p).await.unwrap();
    assert_eq!(baseline.len(), 5);
    assert!(prepare(&p, &baseline, &auth).unwrap().steps.is_empty());
    let state = mock.state.lock().unwrap();
    assert_eq!(
        state.mutations[0]["variables"]["input"]["useDefaultTemplate"],
        false
    );
    assert_eq!(
        state.mutations[2]["variables"]["input"]["useDefaultTemplate"],
        false
    );
}
#[tokio::test]
async fn timeout_after_effect_reconciles_without_second_mutation() {
    let mock = Mock::start().await;
    mock.state.lock().unwrap().timeout_after_at = Some(1);
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Provider(_))
    ));
    assert_eq!(
        journal.receipts[&changes.steps[0].id].phase,
        StepPhase::Attempted
    );
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(mock.writes(), 5);
}
#[tokio::test]
async fn partial_failure_retries_only_missing_create_with_same_id() {
    let mock = Mock::start().await;
    mock.state.lock().unwrap().fail_before_at = Some(3);
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    assert!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await
            .is_err()
    );
    assert_eq!(mock.state.lock().unwrap().objects.len(), 2);
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(mock.writes(), 6);
    let state = mock.state.lock().unwrap();
    assert_eq!(state.objects.len(), 5);
    assert_eq!(
        state.mutations[2]["variables"]["input"]["id"],
        state.mutations[3]["variables"]["input"]["id"]
    );
}
#[tokio::test]
async fn updates_use_patch_and_preserve_unmanaged_manual_fields_for_all_types() {
    let mock = Mock::start().await;
    let mut p = plan();
    let baseline = seed(&mock, &p).await;
    {
        let mut state = mock.state.lock().unwrap();
        for object in state.objects.values_mut() {
            object["manualNote"] = json!("keep me");
        }
    }
    p.revision += 1;
    p.projects[0].name = "Renamed".into();
    p.milestones[0].name = "Renamed milestone".into();
    p.issues[0].title = "Renamed issue".into();
    // Reverse the dependency (still acyclic) to exercise issueRelationUpdate.
    let dependency = &mut p.dependencies[0];
    std::mem::swap(
        &mut dependency.blocking_issue_key,
        &mut dependency.blocked_issue_key,
    );
    let auth = authority(&p);
    let changes = prepare(&p, &baseline, &auth).unwrap();
    assert_eq!(changes.steps.len(), 4);
    assert_eq!(changes.steps[0].patch.len(), 1);
    assert_eq!(changes.steps[0].patch["name"], "Renamed");
    mock.client()
        .execute(&p, &auth, &changes, &mut Journal::default())
        .await
        .unwrap();
    let state = mock.state.lock().unwrap();
    assert!(state.objects.values().all(|o| o["manualNote"] == "keep me"));
    assert!(
        state.mutations[5]["query"]
            .as_str()
            .unwrap()
            .contains("ProjectUpdateInput")
    );
}
#[tokio::test]
async fn manual_edit_since_baseline_is_preserved_and_blocks_write() {
    let mock = Mock::start().await;
    let mut p = plan();
    let baseline = seed(&mock, &p).await;
    p.revision += 1;
    p.projects[0].name = "Proposed rename".into();
    let auth = authority(&p);
    let changes = prepare(&p, &baseline, &auth).unwrap();
    {
        let mut state = mock.state.lock().unwrap();
        let object = state.objects.get_mut(&changes.steps[0].entity_id).unwrap();
        object["name"] = json!("Human rename");
        object["updatedAt"] = json!("manual-edit");
    }
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut Journal::default())
            .await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(mock.writes(), 5);
    assert_eq!(
        mock.state.lock().unwrap().objects[&changes.steps[0].entity_id]["name"],
        "Human rename"
    );
}
#[tokio::test]
async fn revision_or_prepared_payload_tampering_prevents_writes() {
    let mock = Mock::start().await;
    let mut p = plan();
    let auth = authority(&p);
    let mut changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    changes.steps[0]
        .patch
        .insert("name".into(), json!("tampered"));
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut Journal::default())
            .await,
        Err(Error::StalePreparation)
    ));
    p.revision += 1;
    assert!(
        mock.client()
            .execute(&p, &auth, &changes, &mut Journal::default())
            .await
            .is_err()
    );
    assert_eq!(mock.writes(), 0);
}
#[tokio::test]
async fn revoked_authority_and_journal_failure_both_stop_before_send() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal {
        deny_at: Some(1),
        ..Default::default()
    };
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Authority(_))
    ));
    assert_eq!(mock.writes(), 0);
    let mut journal = Journal {
        fail_phase: Some(StepPhase::Attempted),
        ..Default::default()
    };
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Journal(_))
    ));
    assert_eq!(mock.writes(), 0);
    let mut journal = Journal {
        deny_at: Some(2),
        ..Default::default()
    };
    assert!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await
            .is_err()
    );
    assert_eq!(mock.writes(), 1);
}
#[tokio::test]
async fn uncertain_update_is_not_blindly_retried_but_applied_update_is_reconciled() {
    let mock = Mock::start().await;
    let mut p = plan();
    let baseline = seed(&mock, &p).await;
    p.revision += 1;
    p.projects[0].name = "New name".into();
    let auth = authority(&p);
    let changes = prepare(&p, &baseline, &auth).unwrap();
    mock.state.lock().unwrap().fail_before_at = Some(6);
    let mut journal = Journal::default();
    assert!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await
            .is_err()
    );
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(mock.writes(), 6);
    // Explicitly isolated fresh case: server applies update then loses response.
    let mock = Mock::start().await;
    let old = plan();
    let baseline = seed(&mock, &old).await;
    let changes = prepare(&p, &baseline, &auth).unwrap();
    mock.state.lock().unwrap().fail_after_at = Some(6);
    let mut journal = Journal::default();
    assert!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await
            .is_err()
    );
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(mock.writes(), 6);
}
#[tokio::test]
async fn wrong_or_malformed_readback_and_permission_errors_fail_closed() {
    let mock = Mock::start().await;
    let p = plan();
    seed(&mock, &p).await;
    let id = stable_id(p.id, "launch");
    mock.state.lock().unwrap().wrong_id = true;
    assert!(
        mock.client()
            .retrieve(EntityKind::Project, id)
            .await
            .is_err()
    );
    {
        let mut state = mock.state.lock().unwrap();
        state.wrong_id = false;
        state.malformed_readback = true;
    }
    assert!(
        mock.client()
            .retrieve(EntityKind::Project, id)
            .await
            .is_err()
    );
    {
        let mut state = mock.state.lock().unwrap();
        state.malformed_readback = false;
        state.graphql_read_error = true;
    }
    assert!(
        mock.client()
            .retrieve(EntityKind::Project, id)
            .await
            .is_err()
    );
}

#[test]
fn standing_authority_stays_source_scoped_and_adoption_requires_baseline() {
    let mut p = plan();
    let mut auth = authority(&p);
    if let Authority::Agreed { scope, .. } = auth {
        auth = Authority::Standing {
            reference: "policy://project-planning".into(),
            source_reference: p.source.reference.clone(),
            scope,
        };
    }
    prepare(&p, &Baseline::new(), &auth).unwrap();
    p.revision += 1;
    prepare(&p, &Baseline::new(), &auth).unwrap();
    p.source.reference = "chat://another-request".into();
    assert!(matches!(
        prepare(&p, &Baseline::new(), &auth),
        Err(Error::Authority(_))
    ));
    p.projects[0].identity.remote_id = Some(Uuid::new_v4());
    assert!(matches!(
        prepare(&p, &Baseline::new(), &authority(&p)),
        Err(Error::Authority(_))
    ));
    let mut encoded = serde_json::to_value(&p).unwrap();
    encoded["unexpected_auto_publish"] = json!(true);
    assert!(serde_json::from_value::<Plan>(encoded).is_err());
}

#[tokio::test]
async fn completed_step_receipts_remain_immutable_after_unrelated_manual_edits() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    let historical_receipts = journal.receipts.clone();
    {
        let mut state = mock.state.lock().unwrap();
        for object in state.objects.values_mut() {
            object["updatedAt"] = json!("2026-10-31T13:00:00Z");
            object["humanComment"] = json!("Unmanaged field added after completion");
        }
    }
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(journal.receipts, historical_receipts);
    assert_eq!(mock.writes(), 5);
}

fn standing(plan: &Plan) -> Authority {
    let Authority::Agreed { scope, .. } = authority(plan) else {
        unreachable!()
    };
    Authority::Standing {
        reference: "policy://operator-approved".into(),
        source_reference: plan.source.reference.clone(),
        scope,
    }
}
fn scope_mut(authority: &mut Authority) -> &mut Scope {
    match authority {
        Authority::Agreed { scope, .. } | Authority::Standing { scope, .. } => scope,
    }
}

#[test]
fn standing_scope_rejects_substituted_plan_uuid_and_arbitrary_remote_adoption() {
    let original = plan();
    let auth = standing(&original);
    let mut attack = original.clone();
    attack.id = Uuid::new_v4();
    assert!(matches!(
        authorize_plan(&attack, &auth),
        Err(Error::Authority(_))
    ));
    for kind in [
        EntityKind::Project,
        EntityKind::Milestone,
        EntityKind::Issue,
        EntityKind::Dependency,
    ] {
        let mut attack = original.clone();
        let identity = match kind {
            EntityKind::Project => &mut attack.projects[0].identity,
            EntityKind::Milestone => &mut attack.milestones[0].identity,
            EntityKind::Issue => &mut attack.issues[0].identity,
            EntityKind::Dependency => &mut attack.dependencies[0].identity,
            _ => unreachable!(),
        };
        let unrelated_id = Uuid::new_v4();
        identity.remote_id = Some(unrelated_id);
        let baseline = Baseline::from([(
            unrelated_id,
            RemoteEntity {
                id: unrelated_id,
                kind,
                fields: BTreeMap::new(),
                updated_at: "reviewed-but-not-authorized".into(),
            },
        )]);
        assert!(matches!(
            authorize_plan(&attack, &auth),
            Err(Error::Authority(_))
        ));
        assert!(matches!(
            prepare(&attack, &baseline, &auth),
            Err(Error::Authority(_))
        ));
    }
    authorize_plan(&original, &auth).unwrap();
    assert_eq!(
        prepare(&original, &Baseline::new(), &auth)
            .unwrap()
            .steps
            .len(),
        5
    );
}

#[test]
fn existing_resource_binding_rejects_replacement_ids_kinds_parent_projects_and_identity_removal() {
    let mut p = plan();
    let id = Uuid::new_v4();
    p.issues[0].identity.remote_id = Some(id);
    let mut auth = standing(&p);
    scope_mut(&mut auth).existing_resources.insert(
        "build".into(),
        ExistingResource {
            id,
            kind: EntityKind::Issue,
            project_key: "launch".into(),
        },
    );
    authorize_plan(&p, &auth).unwrap();
    // Even an exact operator binding cannot bypass mandatory baseline capture.
    assert!(matches!(
        prepare(&p, &Baseline::new(), &auth),
        Err(Error::Validation(_))
    ));
    let mut attack = p.clone();
    attack.issues[0].identity.remote_id = Some(Uuid::new_v4());
    assert!(matches!(
        authorize_plan(&attack, &auth),
        Err(Error::Authority(_))
    ));
    let mut attack = p.clone();
    attack.issues[0].identity.remote_id = None;
    assert!(matches!(
        authorize_plan(&attack, &auth),
        Err(Error::Authority(_))
    ));
    let mut attack = p.clone();
    let issue_identity = attack.issues[0].identity.clone();
    let milestone_identity = attack.milestones[0].identity.clone();
    attack.milestones[0].identity = issue_identity;
    attack.issues[0].identity = milestone_identity;
    for issue in &mut attack.issues {
        issue.milestone_key = Some("build".into());
    }
    attack.dependencies[0].blocking_issue_key = "prototype".into();
    attack.validate().unwrap();
    assert!(matches!(
        authorize_plan(&attack, &auth),
        Err(Error::Authority(_))
    ));
    let mut attack = p.clone();
    let mut other_project = attack.projects[0].clone();
    other_project.identity.key = "other".into();
    attack.projects.push(other_project);
    attack.issues[0].project_key = "other".into();
    attack.issues[0].milestone_key = None;
    scope_mut(&mut auth).project_keys.insert("other".into());
    attack.validate().unwrap();
    assert!(matches!(
        authorize_plan(&attack, &auth),
        Err(Error::Authority(_))
    ));
}

#[tokio::test]
async fn explicitly_bound_existing_resources_remain_updatable() {
    let mock = Mock::start().await;
    let mut p = plan();
    let baseline = seed(&mock, &p).await;
    // An operator explicitly approves adopting the selected existing project.
    let id = stable_id(p.id, "launch");
    p.projects[0].identity.remote_id = Some(id);
    p.revision += 1;
    p.projects[0].name = "Approved existing rename".into();
    let mut auth = standing(&p);
    scope_mut(&mut auth).existing_resources.insert(
        "launch".into(),
        ExistingResource {
            id,
            kind: EntityKind::Project,
            project_key: "launch".into(),
        },
    );
    authorize_plan(&p, &auth).unwrap();
    let changes = prepare(&p, &baseline, &auth).unwrap();
    assert_eq!(changes.steps.len(), 1);
    mock.client()
        .execute(&p, &auth, &changes, &mut Journal::default())
        .await
        .unwrap();
    assert_eq!(mock.writes(), 6);
    assert_eq!(
        mock.state.lock().unwrap().objects[&id]["name"],
        "Approved existing rename"
    );
}

#[tokio::test]
async fn uncertain_missing_create_retry_rechecks_current_authorization_before_send() {
    let mock = Mock::start().await;
    mock.state.lock().unwrap().fail_before_at = Some(1);
    let p = plan();
    let auth = standing(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Provider(_))
    ));
    assert_eq!(
        journal.receipts[&changes.steps[0].id].phase,
        StepPhase::Attempted
    );
    assert_eq!(mock.writes(), 1);
    journal.deny_at = Some(journal.writes + 1);
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Authority(_))
    ));
    assert_eq!(mock.writes(), 1);
    assert!(mock.state.lock().unwrap().objects.is_empty());
}

#[test]
fn operator_authority_example_authorizes_only_its_bound_plan() {
    let auth: Authority =
        serde_json::from_str(include_str!("../examples/standing-authority.json")).unwrap();
    let p = plan();
    authorize_plan(&p, &auth).unwrap();
    assert_eq!(prepare(&p, &Baseline::new(), &auth).unwrap().steps.len(), 5);
    let mut substituted = p;
    substituted.id = Uuid::new_v4();
    assert!(matches!(
        authorize_plan(&substituted, &auth),
        Err(Error::Authority(_))
    ));
}

#[tokio::test]
async fn denied_initial_read_prevents_provider_access_and_mutation() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal {
        deny_read_at: Some(1),
        ..Default::default()
    };
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Authority(_))
    ));
    let first = &changes.steps[0];
    assert_eq!(journal.reads, vec![(first.kind, first.entity_id)]);
    assert!(mock.state.lock().unwrap().reads.is_empty());
    assert_eq!(mock.writes(), 0);
    assert_eq!(journal.receipts[&first.id].phase, StepPhase::Prepared);
}
#[tokio::test]
async fn denied_readback_leaves_attempted_receipt_and_resume_reconciles_without_rewrite() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal {
        deny_read_at: Some(2),
        ..Default::default()
    };
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut journal)
            .await,
        Err(Error::Authority(_))
    ));
    assert_eq!(mock.state.lock().unwrap().reads.len(), 1);
    assert_eq!(mock.writes(), 1);
    assert_eq!(
        journal.receipts[&changes.steps[0].id].phase,
        StepPhase::Attempted
    );
    // Once a fresh host decision permits reading, observe the first completed
    // mutation instead of submitting it again, then finish the remaining steps.
    journal.deny_read_at = None;
    let result = mock
        .client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(result.len(), changes.steps.len());
    assert_eq!(mock.writes(), changes.steps.len());
    assert!(
        journal
            .receipts
            .values()
            .all(|r| r.phase == StepPhase::Verified)
    );
    assert_eq!(
        mock.state.lock().unwrap().reads.len(),
        changes.steps.len() * 2
    );
}
#[tokio::test]
async fn completed_receipts_do_not_bypass_live_read_authorization_on_resume() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    let before = mock.state.lock().unwrap().reads.len();
    let historical = journal.receipts.clone();
    let mut resumed = Journal {
        receipts: historical.clone(),
        deny_read_at: Some(1),
        ..Default::default()
    };
    assert!(matches!(
        mock.client()
            .execute(&p, &auth, &changes, &mut resumed)
            .await,
        Err(Error::Authority(_))
    ));
    assert_eq!(mock.state.lock().unwrap().reads.len(), before);
    assert_eq!(mock.writes(), changes.steps.len());
    assert_eq!(resumed.receipts, historical);
}
#[tokio::test]
async fn each_prewrite_and_readback_has_exact_resource_read_admission() {
    let mock = Mock::start().await;
    let p = plan();
    let auth = authority(&p);
    let changes = prepare(&p, &Baseline::new(), &auth).unwrap();
    let mut journal = Journal::default();
    mock.client()
        .execute(&p, &auth, &changes, &mut journal)
        .await
        .unwrap();
    let expected: Vec<_> = changes
        .steps
        .iter()
        .flat_map(|s| [(s.kind, s.entity_id); 2])
        .collect();
    assert_eq!(journal.reads, expected);
    assert_eq!(
        mock.state.lock().unwrap().reads,
        journal.reads.iter().map(|(_, id)| *id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn roadmap_metadata_and_progress_updates_are_verified_by_readback() {
    let mock = Mock::start().await;
    let mut draft = serde_json::to_value(plan()).unwrap();
    let team = draft["projects"][0]["team_ids"][0].clone();
    let status = Uuid::new_v4();
    draft["projects"][0]["target_date"] = json!("2027-01-20");
    draft["projects"][0]["start_date"] = json!("2026-11-20");
    draft["projects"][0]["status_id"] = json!(status);
    draft["projects"].as_array_mut().unwrap().push(json!({
        "identity":{"key":"successor"},"team_ids":[team],"name":"Follow-on",
        "description":"Second project","acceptance_criteria":["Completed"]
    }));
    draft["milestones"][0]["target_date"] = json!("2026-12-01");
    draft["issues"][1]["parent_key"] = json!("build");
    draft["issues"][1]["label_ids"] = json!([Uuid::new_v4()]);
    draft["issues"][1]["cycle_id"] = json!(Uuid::new_v4());
    draft["issues"][1]["priority"] = json!(2);
    draft["issues"][1]["state_id"] = json!(Uuid::new_v4());
    draft["issues"][1]["due_date"] = json!("2026-12-02");
    draft["roadmap"] = json!([
       {"identity":{"key":"direction"},"project_key":"launch","change":{"kind":"initiative","name":"Recovery","description":"Service health","status":"Active","target_date":"2027-02-01","owner_id":Uuid::new_v4()}},
       {"identity":{"key":"link"},"project_key":"launch","change":{"kind":"initiative_project","initiative_key":"direction"}},
       {"identity":{"key":"after-launch"},"project_key":"launch","change":{"kind":"project_dependency","blocked_project_key":"successor"}},
       {"identity":{"key":"project-week"},"project_key":"launch","change":{"kind":"project_update","body":"One service remains blocked.","health":"atRisk"}},
       {"identity":{"key":"initiative-week"},"project_key":"launch","change":{"kind":"initiative_update","initiative_key":"direction","body":"New target February 1.","health":"onTrack"}}
    ]);
    let plan: Plan = serde_json::from_value(draft).unwrap();
    let authority = authority(&plan);
    let changes = prepare(&plan, &Baseline::new(), &authority).unwrap();
    let mut journal = Journal::default();
    let client = mock.client();
    let verified = client
        .execute(&plan, &authority, &changes, &mut journal)
        .await
        .unwrap();
    assert_eq!(verified.len(), 11);
    let baseline = client.retrieve_plan(&plan).await.unwrap();
    assert!(
        prepare(&plan, &baseline, &authority)
            .unwrap()
            .steps
            .is_empty()
    );
    assert_eq!(
        baseline[&stable_id(plan.id, "review")].fields["parentId"],
        json!(stable_id(plan.id, "build"))
    );
    assert_eq!(
        baseline[&stable_id(plan.id, "initiative-week")].fields["health"],
        "onTrack"
    );
}
