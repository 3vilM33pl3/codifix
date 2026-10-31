#![allow(clippy::unwrap_used)]
pub use codifix_agency_runtime::{config, store};
#[path = "../src/learning.rs"]
mod learning;
use chrono::{Duration, Utc};
use codifix_agency::{improvement::*, routing::*, *};
use config::{Config, Peer};
use learning::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use store::{Store, Submit};
use uuid::Uuid;
struct Fixture {
    db: codifix_test_support::IsolatedDatabase,
    store: Store,
    cfg: Config,
    grant: Mandate,
    peer: Peer,
}
async fn fixture() -> Option<Fixture> {
    let db = codifix_test_support::IsolatedDatabase::new().await?;
    let store = Store {
        pool: db.pool.clone(),
    };
    let instance = Uuid::new_v4();
    let project = Uuid::new_v4();
    sqlx::query("INSERT INTO projects(id,slug,name,root_path) VALUES($1,$2,'learning fixture','/synthetic')").bind(project).bind(project.to_string()).execute(&db.pool).await.unwrap();
    let grant = Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: instance,
        subject_instance_id: instance,
        specialist_id: "drafter".into(),
        capabilities: BTreeSet::from(["model_draft".into()]),
        resources: BTreeSet::from([ResourceScope {
            kind: "draft".into(),
            id: "fixture".into(),
        }]),
        destinations: BTreeSet::new(),
        expires_at: Utc::now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 10000,
            calls: 1000,
            turns: 1000,
            delegation_depth: 2,
        },
    };
    let peer = Peer {
        id: "operator".into(),
        instance_id: instance,
        token_env: "UNUSED_LEARNING_TOKEN".into(),
        operator: true,
        mandates: BTreeSet::from([grant.id]),
        endpoint: None,
        outbound_token_env: None,
    };
    store
        .install_mandate(project, &grant, &peer, instance)
        .await
        .unwrap();
    let cfg = Config {
        instance_id: instance,
        listen: "127.0.0.1:0".parse().unwrap(),
        public_url: "https://agency.example.test".into(),
        database_env: "UNUSED_LEARNING_DB".into(),
        specialists: vec![SpecialistManifest {
            schema_version: 1,
            instance_id: instance,
            specialist_id: "drafter".into(),
            name: "Drafter".into(),
            capabilities: grant.capabilities.clone(),
        }],
        peers: vec![peer.clone()],
        models: vec![],
        model_profiles: vec![],
        public_profile: None,
        private_repository_dids: vec![],
        linear: None,
        openbao_controllers: BTreeMap::new(),
    };
    Some(Fixture {
        db,
        store,
        cfg,
        grant,
        peer,
    })
}
fn routing() -> RoutingRequest {
    RoutingRequest {
        workload: "draft-json".into(),
        minimum_quality_ppm: 900000,
        minimum_held_out_cases: 20,
        context_tokens: 100,
        max_latency_ms: 5000,
        available_local_memory_mib: 10000,
        permitted_destinations: BTreeSet::new(),
    }
}
async fn candidate(
    f: &Fixture,
    kind: DraftArtifact,
) -> (ImprovementCandidate, ArtifactDocument, String) {
    let base = improve(
        &f.store,
        &f.cfg,
        json!({"action":"baseline","specialist_id":"drafter","workload":"draft-json"}),
    )
    .await
    .unwrap();
    let artifact = ArtifactDocument::new("draft-v2".into(), kind).unwrap();
    let candidate = ImprovementCandidate {
        id: Uuid::new_v4(),
        owner_instance_id: f.cfg.instance_id,
        kind: match &artifact.content {
            DraftArtifact::Prompt { .. } => ImprovementKind::Prompt,
            DraftArtifact::Procedure { .. } => ImprovementKind::Procedure,
            DraftArtifact::ModelProfile { .. } => ImprovementKind::ModelProfile,
        },
        candidate_revision: artifact.revision.clone(),
        baseline_revision: base["revision"].as_str().unwrap().into(),
        stage: ImprovementStage::Candidate,
        evaluation: None,
        rollout_ppm: 0,
        rollout_successes: 0,
        rollout_failures: 0,
        local_acceptance: None,
        revert_reason: None,
    };
    improve(&f.store,&f.cfg,json!({"action":"propose","specialist_id":"drafter","workload":"draft-json","candidate":candidate,"artifact":artifact,"baseline_digest":base["digest"]})).await.unwrap();
    (candidate, artifact, base["digest"].as_str().unwrap().into())
}
async fn rollout(f: &Fixture, c: &ImprovementCandidate, a: &ArtifactDocument, base: &str) {
    let evaluation = ImprovementEvaluation {
        evidence_id: format!("heldout-{}", c.id),
        candidate_revision: c.candidate_revision.clone(),
        baseline_revision: c.baseline_revision.clone(),
        held_out_set_id: "heldout-v1".into(),
        training_set_id: "training-v1".into(),
        held_out_cases: 20,
        candidate_quality_ppm: 950000,
        baseline_quality_ppm: 900000,
        safety_failures: 0,
        evaluated_at: Utc::now(),
    };
    improve(&f.store,&f.cfg,json!({"action":"evaluate","id":c.id,"evaluation":evaluation,"artifact_digest":a.digest,"baseline_digest":base,"verifier_id":"external-review-v1"})).await.unwrap();
    improve(&f.store,&f.cfg,json!({"action":"rollout","id":c.id,"fraction_ppm":100000,"decision_ref":"local-rollout-approval"})).await.unwrap();
}
async fn task(f: &Fixture, selected_candidate: Uuid) -> store::StoredTask {
    // Runtime allocates IDs; find a naturally assigned deterministic canary task.
    for _ in 0..200 {
        let req = Submit {
            request_id: Uuid::new_v4().to_string(),
            mandate_id: f.grant.id,
            specialist_id: "drafter".into(),
            objective: "Synthetic drafting".into(),
            acceptance_criteria: vec!["model_output_valid_json".into()],
            capability: "model_draft".into(),
            resource: ResourceScope {
                kind: "draft".into(),
                id: "fixture".into(),
            },
            deadline: Utc::now() + Duration::minutes(10),
            budget: Budget {
                micro_usd: 1,
                calls: 1,
                turns: 2,
                delegation_depth: 1,
            },
            payload: json!({"kind":"model_draft","prompt":"Task input"}),
            parent_id: None,
        };
        let mut t = f.store.submit(&req, &f.peer).await.unwrap();
        if canary_selected(selected_candidate, t.contract.id, 100000).unwrap() {
            t.contract
                .transition(TaskStatus::Working, Utc::now())
                .unwrap();
            f.store.update_contract(&t).await.unwrap();
            return f.store.get_internal(t.contract.id).await.unwrap();
        }
        // Keep the real runtime's active-task admission bound intact while
        // sampling server-assigned IDs for deterministic canary membership.
        f.store.cancel(t.contract.id, &f.peer).await.unwrap();
    }
    panic!("could not allocate canary fixture task");
}
async fn complete(f: &Fixture, t: &store::StoredTask, b: &DraftBinding) -> Value {
    let mut t = t.clone();
    let reference = format!("agency:task:{}:evidence", t.contract.id);
    t.contract
        .finish(
            TaskOutcome {
                summary: "Verified JSON artifact".into(),
                artifact_refs: vec![reference.clone()],
                evidence_refs: vec![reference.clone()],
                acceptance: vec![AcceptanceResult {
                    criterion: "model_output_valid_json".into(),
                    passed: true,
                    evidence_ref: reference,
                }],
            },
            Utc::now(),
        )
        .unwrap();
    f.store.update_contract(&t).await.unwrap();
    let evidence = json!({"artifact":{"synthetic":true},"learning":b,"provider_usage":{"prompt_tokens":20,"completion_tokens":10}});
    sqlx::query("INSERT INTO agency_outcomes(task_id,evidence) VALUES($1,$2)")
        .bind(t.contract.id)
        .bind(&evidence)
        .execute(&f.db.pool)
        .await
        .unwrap();
    evidence
}
fn assessment(t: &store::StoredTask, b: &DraftBinding, index: usize) -> TaskAssessment {
    TaskAssessment {
        task_id: t.contract.id,
        candidate_id: b.candidate_id,
        artifact_digest: b.artifact_digest.clone(),
        baseline_digest: b.baseline_digest.clone(),
        verifier_id: "external-review-v1".into(),
        dataset_id: "canary-independent-v1".into(),
        case_id: format!("case-{index}"),
        evidence_id: format!("assessment-{}", t.contract.id),
        quality_ppm: 950000,
        safety_failures: 0,
        assessed_at: Utc::now(),
    }
}
#[tokio::test]
async fn immutable_artifact_applies_deterministically_and_json_completion_does_not_promote() {
    let Some(f) = fixture().await else { return };
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Prompt {
            prefix: "Use short sentences.".into(),
            suffix: "Include an explanation.".into(),
        },
    )
    .await;
    rollout(&f, &c, &a, &base).await;
    let t = task(&f, c.id).await;
    let applied = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    assert!(applied.prompt.contains("Use short sentences."));
    assert!(applied.prompt.contains("Task input"));
    let b = applied.binding.unwrap();
    assert_eq!(b.arm, Arm::Candidate);
    let again = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    assert_eq!(again.binding, Some(b.clone()));
    complete(&f, &t, &b).await;
    let observed = observe_task(&f.store, &f.cfg, t.contract.id).await.unwrap();
    assert_eq!(observed["observed"], true);
    assert_eq!(
        observe_task(&f.store, &f.cfg, t.contract.id).await.unwrap()["observed"],
        false
    );
    let state = improve(&f.store, &f.cfg, json!({"action":"get","id":c.id}))
        .await
        .unwrap();
    assert_eq!(state["candidate"]["rollout_successes"], 0);
    assert!(
        improve(
            &f.store,
            &f.cfg,
            json!({"action":"promote","id":c.id,"decision_ref":"local-promote-approval"})
        )
        .await
        .is_err()
    );
    assert!(
        improve(
            &f.store,
            &f.cfg,
            json!({"action":"observe","task_id":t.contract.id,"passed":true})
        )
        .await
        .is_err()
    );
    let assess = assessment(&t, &b, 0);
    improve(
        &f.store,
        &f.cfg,
        json!({"action":"assess","assessment":assess}),
    )
    .await
    .unwrap();
    let state = improve(&f.store, &f.cfg, json!({"action":"get","id":c.id}))
        .await
        .unwrap();
    assert_eq!(state["candidate"]["rollout_successes"], 1);
    assert_eq!(
        improve(
            &f.store,
            &f.cfg,
            json!({"action":"assess","assessment":assess})
        )
        .await
        .unwrap()["assessed"],
        false
    );
    let mut forged = assess;
    forged.quality_ppm = 1_000_000;
    assert!(
        improve(
            &f.store,
            &f.cfg,
            json!({"action":"assess","assessment":forged})
        )
        .await
        .is_err()
    );
    improve(
        &f.store,
        &f.cfg,
        json!({"action":"revert","id":c.id,"reason":"operator rollback"}),
    )
    .await
    .unwrap();
    let fresh = task(&f, c.id).await;
    let plain = select_draft(&f.store, &f.cfg, &fresh, "Task input", &routing())
        .await
        .unwrap();
    assert_eq!(plain.prompt, "Task input");
    assert!(plain.binding.is_none());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn independent_quality_cases_gate_promotion_and_procedure_reversion() {
    let Some(f) = fixture().await else { return };
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Procedure {
            steps: vec![
                "Extract the requested fields.".into(),
                "Check facts against supplied data.".into(),
            ],
        },
    )
    .await;
    rollout(&f, &c, &a, &base).await;
    let mut first_assessment = None;
    for index in 0..20 {
        let t = task(&f, c.id).await;
        let applied = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
            .await
            .unwrap();
        assert!(applied.prompt.starts_with("Drafting procedure:"));
        let b = applied.binding.unwrap();
        complete(&f, &t, &b).await;
        let mut assess = assessment(&t, &b, index);
        if index == 1 {
            let first: TaskAssessment = first_assessment.clone().unwrap();
            assess.case_id = first.case_id.clone();
            assert!(
                improve(
                    &f.store,
                    &f.cfg,
                    json!({"action":"assess","assessment":assess})
                )
                .await
                .is_err()
            );
            assess.case_id = format!("case-{index}");
            let mut forged = assess.clone();
            forged.artifact_digest = "0".repeat(64);
            assert!(
                improve(
                    &f.store,
                    &f.cfg,
                    json!({"action":"assess","assessment":forged})
                )
                .await
                .is_err()
            );
            let mut forged = assess.clone();
            forged.verifier_id = "candidate-self-report".into();
            assert!(
                improve(
                    &f.store,
                    &f.cfg,
                    json!({"action":"assess","assessment":forged})
                )
                .await
                .is_err()
            );
            let mut forged = assess.clone();
            forged.dataset_id = "training-v1".into();
            assert!(
                improve(
                    &f.store,
                    &f.cfg,
                    json!({"action":"assess","assessment":forged})
                )
                .await
                .is_err()
            );
        }
        improve(
            &f.store,
            &f.cfg,
            json!({"action":"assess","assessment":assess}),
        )
        .await
        .unwrap();
        if index == 0 {
            first_assessment = Some(assess);
        }
    }
    let promoted = improve(
        &f.store,
        &f.cfg,
        json!({"action":"promote","id":c.id,"decision_ref":"independent-local-promotion"}),
    )
    .await
    .unwrap();
    assert_eq!(promoted["candidate"]["stage"], "promoted");
    assert_eq!(promoted["candidate"]["rollout_successes"], 20);
    let t = task(&f, c.id).await;
    let applied = select_draft(&f.store, &f.cfg, &t, "Fresh input", &routing())
        .await
        .unwrap();
    assert!(
        applied
            .prompt
            .contains("Check facts against supplied data.")
    );
    improve(
        &f.store,
        &f.cfg,
        json!({"action":"revert","id":c.id,"reason":"restore built-in baseline"}),
    )
    .await
    .unwrap();
    let restored = select_draft(&f.store, &f.cfg, &t, "Fresh input", &routing())
        .await
        .unwrap();
    assert_eq!(restored.prompt, "Fresh input");
    assert!(restored.binding.is_none());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn artifact_digest_evaluation_binding_and_operational_failure_fail_closed() {
    let Some(f) = fixture().await else { return };
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Prompt {
            prefix: "Be precise.".into(),
            suffix: String::new(),
        },
    )
    .await;
    let mut corrupt = a.clone();
    corrupt.content = DraftArtifact::Prompt {
        prefix: "Unreviewed replacement".into(),
        suffix: String::new(),
    };
    assert_ne!(
        ArtifactDocument::new(corrupt.revision.clone(), corrupt.content.clone())
            .unwrap()
            .digest,
        corrupt.digest
    );
    let evaluation = ImprovementEvaluation {
        evidence_id: "bound-heldout".into(),
        candidate_revision: c.candidate_revision.clone(),
        baseline_revision: c.baseline_revision.clone(),
        held_out_set_id: "heldout-v1".into(),
        training_set_id: "training-v1".into(),
        held_out_cases: 20,
        candidate_quality_ppm: 950000,
        baseline_quality_ppm: 900000,
        safety_failures: 0,
        evaluated_at: Utc::now(),
    };
    assert!(improve(&f.store,&f.cfg,json!({"action":"evaluate","id":c.id,"evaluation":evaluation,"artifact_digest":"0".repeat(64),"baseline_digest":base,"verifier_id":"external-review-v1"})).await.is_err());
    rollout(&f, &c, &a, &base).await;
    let mut t = task(&f, c.id).await;
    select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    t.contract
        .clarify("Provider result uncertain".into(), Utc::now())
        .unwrap();
    f.store.update_contract(&t).await.unwrap();
    observe_task(&f.store, &f.cfg, t.contract.id).await.unwrap();
    observe_task(&f.store, &f.cfg, t.contract.id).await.unwrap();
    let state = improve(&f.store, &f.cfg, json!({"action":"get","id":c.id}))
        .await
        .unwrap();
    assert_eq!(state["candidate"]["rollout_failures"], 1);
    assert_eq!(state["candidate"]["rollout_successes"], 0);
    let mut unsupported = serde_json::to_value(&a).unwrap();
    unsupported["content"] = json!({"kind":"retrieval","path":"/private"});
    assert!(serde_json::from_value::<ArtifactDocument>(unsupported).is_err());
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn model_profile_artifact_filters_routes_without_expanding_authority_and_detects_drift() {
    let Some(mut f) = fixture().await else { return };
    let destination = ModelDestination::Local {
        runtime: "synthetic-local".into(),
    };
    for id in ["baseline-model", "candidate-model"] {
        f.cfg.model_profiles.push(ModelProfile {
            id: id.into(),
            model_revision: format!("{id}-v1"),
            destination: destination.clone(),
            accepted_locally: true,
            context_tokens: 4096,
            local_memory_mib: 1024,
            evaluations: vec![ModelEvaluation {
                evidence_id: format!("eval-{id}"),
                model_revision: format!("{id}-v1"),
                workload: "draft-json".into(),
                evaluated_at: Utc::now() - Duration::minutes(1),
                valid_until: Utc::now() + Duration::hours(1),
                held_out_cases: 20,
                quality_ppm: 950000,
                micro_usd_per_call: 1,
                latency_ms: 100,
            }],
        });
        f.cfg.models.push(config::ModelEndpoint {
            profile_id: id.into(),
            url: "http://127.0.0.1:1/never-called".into(),
            token_env: None,
            model: format!("{id}-v1"),
            max_input_bytes: 1024,
            max_output_tokens: 64,
            reserve_micro_usd: 1,
            input_micro_usd_per_million: 0,
            output_micro_usd_per_million: 0,
        });
    }
    let digest = configured_profile_digest(&f.cfg, "candidate-model").unwrap();
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::ModelProfile {
            profile_id: "candidate-model".into(),
            profile_digest: digest,
        },
    )
    .await;
    rollout(&f, &c, &a, &base).await;
    let t = task(&f, c.id).await;
    let applied = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    assert_eq!(applied.profiles.len(), 1);
    assert_eq!(applied.profiles[0].id, "candidate-model");
    // Neither task nor mandate permits the destination: deployment cannot grant it.
    assert!(
        select_model(
            &applied.profiles,
            &routing(),
            &f.grant,
            Usage::default(),
            Utc::now()
        )
        .is_err()
    );
    f.cfg.models[1].model = "unreviewed-provider-revision".into();
    assert!(
        select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
            .await
            .is_err()
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn canary_quality_cannot_regress_a_stronger_independent_baseline() {
    let Some(f) = fixture().await else { return };
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Prompt {
            prefix: "Use the reviewed template.".into(),
            suffix: String::new(),
        },
    )
    .await;
    let evaluation = ImprovementEvaluation {
        evidence_id: "strong-baseline-heldout".into(),
        candidate_revision: c.candidate_revision.clone(),
        baseline_revision: c.baseline_revision.clone(),
        held_out_set_id: "heldout-v1".into(),
        training_set_id: "training-v1".into(),
        held_out_cases: 20,
        candidate_quality_ppm: 995000,
        baseline_quality_ppm: 990000,
        safety_failures: 0,
        evaluated_at: Utc::now(),
    };
    improve(&f.store,&f.cfg,json!({"action":"evaluate","id":c.id,"evaluation":evaluation,"artifact_digest":a.digest,"baseline_digest":base,"verifier_id":"external-review-v1"})).await.unwrap();
    improve(&f.store,&f.cfg,json!({"action":"rollout","id":c.id,"fraction_ppm":100000,"decision_ref":"high-baseline-rollout"})).await.unwrap();
    let t = task(&f, c.id).await;
    let applied = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    let binding = applied.binding.unwrap();
    complete(&f, &t, &binding).await;
    let mut assessment = assessment(&t, &binding, 0);
    assessment.quality_ppm = 940000;
    let result = improve(
        &f.store,
        &f.cfg,
        json!({"action":"assess","assessment":assessment}),
    )
    .await
    .unwrap();
    assert_eq!(result["quality_passed"], false);
    let state = improve(&f.store, &f.cfg, json!({"action":"get","id":c.id}))
        .await
        .unwrap();
    assert_eq!(state["candidate"]["rollout_successes"], 0);
    assert_eq!(state["candidate"]["rollout_failures"], 1);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn deployed_prompt_reaches_mock_model_and_evidence_persists_exact_binding() {
    use axum::{Json, Router, extract::State, routing::post};
    use std::sync::{Arc, Mutex};
    async fn provider(
        State(captured): State<Arc<Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        captured.lock().unwrap().push(body);
        Json(
            json!({"choices":[{"message":{"content":"{\"reviewed\":true}"}}],"usage":{"prompt_tokens":20,"completion_tokens":10}}),
        )
    }
    let Some(mut f) = fixture().await else { return };
    let captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/chat", post(provider))
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let destination = ModelDestination::Local {
        runtime: "learning-mock".into(),
    };
    f.grant.destinations.insert(destination.clone());
    sqlx::query("UPDATE agency_mandates SET document=$2 WHERE id=$1")
        .bind(f.grant.id)
        .bind(serde_json::to_value(&f.grant).unwrap())
        .execute(&f.db.pool)
        .await
        .unwrap();
    f.cfg.model_profiles.push(ModelProfile {
        id: "learning-model".into(),
        model_revision: "learning-model-v1".into(),
        destination: destination.clone(),
        accepted_locally: true,
        context_tokens: 4096,
        local_memory_mib: 1024,
        evaluations: vec![ModelEvaluation {
            evidence_id: "learning-model-heldout".into(),
            model_revision: "learning-model-v1".into(),
            workload: "draft-json".into(),
            evaluated_at: Utc::now() - Duration::minutes(1),
            valid_until: Utc::now() + Duration::hours(1),
            held_out_cases: 20,
            quality_ppm: 950000,
            micro_usd_per_call: 1,
            latency_ms: 100,
        }],
    });
    f.cfg.models.push(config::ModelEndpoint {
        profile_id: "learning-model".into(),
        url: endpoint,
        token_env: None,
        model: "learning-model-v1".into(),
        max_input_bytes: 1024,
        max_output_tokens: 64,
        reserve_micro_usd: 1,
        input_micro_usd_per_million: 0,
        output_micro_usd_per_million: 0,
    });
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Prompt {
            prefix: "Reviewed drafting template: include source fields.".into(),
            suffix: "End with the requested JSON.".into(),
        },
    )
    .await;
    rollout(&f, &c, &a, &base).await;
    let t = task(&f, c.id).await;
    let mut route = routing();
    route.permitted_destinations.insert(destination);
    let result =
        codifix_agency_runtime::model::draft(&f.store, &f.cfg, &t, "Original task input", &route)
            .await
            .unwrap();
    assert_eq!(result["artifact"]["reviewed"], true);
    assert_eq!(result["learning"]["candidate_id"], c.id.to_string());
    assert_eq!(result["learning"]["artifact_digest"], a.digest);
    {
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        let prompt = captured[0]["messages"][1]["content"].as_str().unwrap();
        assert!(prompt.contains("Reviewed drafting template: include source fields."));
        assert!(prompt.contains("Original task input"));
        assert_eq!(captured[0]["messages"][0]["content"], BASE_SYSTEM_PROMPT);
    }
    let binding: DraftBinding = serde_json::from_value(result["learning"].clone()).unwrap();
    complete(&f, &t, &binding).await;
    sqlx::query("UPDATE agency_outcomes SET evidence=$2 WHERE task_id=$1")
        .bind(t.contract.id)
        .bind(&result)
        .execute(&f.db.pool)
        .await
        .unwrap();
    let observed = observe_task(&f.store, &f.cfg, t.contract.id).await.unwrap();
    assert_eq!(observed["observed"], true);
    let persisted: Value = sqlx::query_scalar(
        "SELECT binding FROM agency_learning_assignments WHERE task_id=$1 AND task_revision=$2",
    )
    .bind(t.contract.id)
    .bind(t.revision)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(persisted, result["learning"]);
    let state = improve(&f.store, &f.cfg, json!({"action":"get","id":c.id}))
        .await
        .unwrap();
    assert_eq!(
        state["candidate"]["rollout_successes"], 0,
        "provider JSON success must not count as independently verified quality"
    );
    server.abort();
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
async fn reverting_after_assignment_fences_physical_model_admission_without_debit() {
    let Some(f) = fixture().await else { return };
    let (c, a, base) = candidate(
        &f,
        DraftArtifact::Prompt {
            prefix: "Reviewed template".into(),
            suffix: String::new(),
        },
    )
    .await;
    rollout(&f, &c, &a, &base).await;
    let t = task(&f, c.id).await;
    let applied = select_draft(&f.store, &f.cfg, &t, "Task input", &routing())
        .await
        .unwrap();
    assert_eq!(applied.binding.unwrap().candidate_id, c.id);
    let before = f.store.get_internal(t.contract.id).await.unwrap();
    let mandate_before: (i64, i64, i64) = sqlx::query_as(
        "SELECT used_microusd,used_calls,used_turns FROM agency_mandates WHERE id=$1",
    )
    .bind(f.grant.id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    improve(&f.store,&f.cfg,json!({"action":"revert","id":c.id,"reason":"revoke deployment before physical model admission"})).await.unwrap();
    let intent = codifix_store::effects::EffectIntent {
        id: Uuid::new_v5(&t.contract.id, format!("model:{}", t.revision).as_bytes()),
        project_id: Some(t.project_id),
        kind: codifix_store::effects::EffectKind::ProviderRequest,
        host_id: "agency".into(),
        resource: "model:synthetic-fixture".into(),
        input_fingerprint: "a".repeat(64),
        actor_id: t.contract.specialist_id.clone(),
        source: "agency-model".into(),
    };
    assert!(
        f.store
            .admit_effect(
                t.contract.id,
                t.revision,
                Usage {
                    micro_usd: 1,
                    calls: 1,
                    turns: 1
                },
                &intent
            )
            .await
            .is_err()
    );
    let after = f.store.get_internal(t.contract.id).await.unwrap();
    assert_eq!(
        (after.used_micro_usd, after.used_calls, after.used_turns),
        (before.used_micro_usd, before.used_calls, before.used_turns)
    );
    let mandate_after: (i64, i64, i64) = sqlx::query_as(
        "SELECT used_microusd,used_calls,used_turns FROM agency_mandates WHERE id=$1",
    )
    .bind(f.grant.id)
    .fetch_one(&f.db.pool)
    .await
    .unwrap();
    assert_eq!(mandate_after, mandate_before);
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM agency_attempts WHERE id=$1")
        .bind(intent.id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    let effects: i64 = sqlx::query_scalar("SELECT count(*) FROM external_effects WHERE id=$1")
        .bind(intent.id)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(effects, 0);
    f.db.cleanup().await.unwrap();
}
