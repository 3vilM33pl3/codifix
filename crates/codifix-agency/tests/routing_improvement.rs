use chrono::{DateTime, Duration, Utc};
use codifix_agency::{improvement::*, routing::*, *};
use std::collections::BTreeSet;
use uuid::Uuid;
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc)
}
fn local() -> ModelDestination {
    ModelDestination::Local {
        runtime: "ollama".into(),
    }
}
fn cloud() -> ModelDestination {
    ModelDestination::Cloud {
        provider: "provider".into(),
        region: "uk".into(),
    }
}
fn mandate() -> Mandate {
    Mandate {
        id: Uuid::new_v4(),
        parent_id: None,
        issuer_instance_id: Uuid::new_v4(),
        subject_instance_id: Uuid::new_v4(),
        specialist_id: "research".into(),
        capabilities: BTreeSet::from(["model.call".into()]),
        resources: BTreeSet::new(),
        destinations: BTreeSet::from([local(), cloud()]),
        expires_at: now() + Duration::hours(1),
        status: MandateStatus::Active,
        budget: Budget {
            micro_usd: 100,
            calls: 2,
            turns: 2,
            delegation_depth: 0,
        },
    }
}
fn evaluation() -> ModelEvaluation {
    ModelEvaluation {
        evidence_id: "eval-1".into(),
        model_revision: "digest:123".into(),
        workload: "research".into(),
        evaluated_at: now() - Duration::minutes(1),
        valid_until: now() + Duration::hours(1),
        held_out_cases: 50,
        quality_ppm: 900_000,
        micro_usd_per_call: 10,
        latency_ms: 500,
    }
}
fn profile() -> ModelProfile {
    ModelProfile {
        id: "local".into(),
        model_revision: "digest:123".into(),
        destination: local(),
        accepted_locally: true,
        context_tokens: 8192,
        local_memory_mib: 4096,
        evaluations: vec![evaluation()],
    }
}
fn request() -> RoutingRequest {
    RoutingRequest {
        workload: "research".into(),
        minimum_quality_ppm: 800_000,
        minimum_held_out_cases: 20,
        context_tokens: 4096,
        max_latency_ms: 1000,
        available_local_memory_mib: 8192,
        permitted_destinations: BTreeSet::from([local(), cloud()]),
    }
}
#[test]
fn route_compares_measured_cost_including_local_compute() {
    let local = profile();
    let mut cloud = profile();
    cloud.id = "cloud".into();
    cloud.destination = crate::cloud();
    cloud.evaluations[0].micro_usd_per_call = 15;
    assert_eq!(
        select_model(
            &[cloud.clone(), local.clone()],
            &request(),
            &mandate(),
            Usage::default(),
            now()
        )
        .expect("route")
        .profile_id,
        "local"
    );
    cloud.evaluations[0].micro_usd_per_call = 5;
    assert_eq!(
        select_model(
            &[cloud, local],
            &request(),
            &mandate(),
            Usage::default(),
            now()
        )
        .expect("route")
        .profile_id,
        "cloud"
    );
}
#[test]
fn route_denies_unevaluated_unaccepted_stale_regressed_unaffordable_or_unavailable_profiles() {
    let mut variants = vec![];
    let mut p = profile();
    p.model_revision = "digest:changed".into();
    variants.push(p);
    let mut p = profile();
    p.evaluations.push(evaluation());
    variants.push(p);
    let mut p = profile();
    p.evaluations.clear();
    variants.push(p);
    let mut p = profile();
    p.accepted_locally = false;
    variants.push(p);
    let mut p = profile();
    p.evaluations[0].valid_until = now();
    variants.push(p);
    let mut p = profile();
    let mut regression = evaluation();
    regression.evaluated_at = now();
    regression.quality_ppm = 0;
    p.evaluations.push(regression);
    variants.push(p);
    let mut p = profile();
    p.evaluations[0].micro_usd_per_call = 101;
    variants.push(p);
    let mut p = profile();
    p.evaluations[0].micro_usd_per_call = 0;
    variants.push(p);
    let mut p = profile();
    p.local_memory_mib = 16384;
    variants.push(p);
    let mut p = profile();
    p.context_tokens = 2048;
    variants.push(p);
    for p in variants {
        assert_eq!(
            select_model(&[p], &request(), &mandate(), Usage::default(), now()),
            Err(PolicyError::NoEligibleModel)
        );
    }
}
#[test]
fn destination_requires_both_task_and_mandate_consent_and_budget_reservation() {
    let p = profile();
    let mut r = request();
    r.permitted_destinations.clear();
    assert!(
        select_model(
            std::slice::from_ref(&p),
            &r,
            &mandate(),
            Usage::default(),
            now()
        )
        .is_err()
    );
    let mut m = mandate();
    m.destinations.clear();
    assert!(
        select_model(
            std::slice::from_ref(&p),
            &request(),
            &m,
            Usage::default(),
            now()
        )
        .is_err()
    );
    assert!(
        select_model(
            &[p],
            &request(),
            &mandate(),
            Usage {
                calls: 2,
                ..Usage::default()
            },
            now()
        )
        .is_err()
    );
}
fn gate() -> ImprovementGate {
    ImprovementGate {
        minimum_held_out_cases: 20,
        minimum_quality_ppm: 800_000,
        maximum_rollout_ppm: 50_000,
        minimum_rollout_successes: 2,
    }
}
fn candidate() -> ImprovementCandidate {
    ImprovementCandidate {
        id: Uuid::new_v4(),
        owner_instance_id: Uuid::new_v4(),
        kind: ImprovementKind::Prompt,
        candidate_revision: "sha256:new".into(),
        baseline_revision: "sha256:old".into(),
        stage: ImprovementStage::Candidate,
        evaluation: None,
        rollout_ppm: 0,
        rollout_successes: 0,
        rollout_failures: 0,
        local_acceptance: None,
        revert_reason: None,
    }
}
fn improvement_eval() -> ImprovementEvaluation {
    ImprovementEvaluation {
        evidence_id: "e1".into(),
        candidate_revision: "sha256:new".into(),
        baseline_revision: "sha256:old".into(),
        held_out_set_id: "heldout-v1".into(),
        training_set_id: "train-v1".into(),
        held_out_cases: 30,
        candidate_quality_ppm: 900_000,
        baseline_quality_ppm: 850_000,
        safety_failures: 0,
        evaluated_at: now(),
    }
}
fn acceptance(c: &ImprovementCandidate, id: &str) -> LocalAcceptance {
    LocalAcceptance {
        owner_instance_id: c.owner_instance_id,
        decision_ref: id.into(),
        accepted_at: now(),
    }
}
#[test]
fn improvement_cannot_skip_evaluation_rollout_or_local_acceptance() {
    let mut c = candidate();
    assert!(
        c.start_rollout(1000, acceptance(&c, "start"), &gate(), now())
            .is_err()
    );
    let mut e = improvement_eval();
    e.training_set_id = e.held_out_set_id.clone();
    assert!(c.record_evaluation(e, &gate(), now()).is_err());
    c.record_evaluation(improvement_eval(), &gate(), now())
        .expect("evaluation");
    let mut foreign = acceptance(&c, "start");
    foreign.owner_instance_id = Uuid::new_v4();
    assert!(c.start_rollout(1000, foreign, &gate(), now()).is_err());
    assert!(
        c.start_rollout(100_000, acceptance(&c, "start"), &gate(), now())
            .is_err()
    );
    c.start_rollout(1000, acceptance(&c, "start"), &gate(), now())
        .expect("rollout");
    assert!(
        c.promote(acceptance(&c, "promote"), &gate(), now())
            .is_err()
    );
    c.record_rollout_result(true).expect("result");
    c.record_rollout_result(true).expect("result");
    assert!(c.promote(acceptance(&c, "start"), &gate(), now()).is_err());
    c.promote(acceptance(&c, "promote"), &gate(), now())
        .expect("promotion");
    c.revert("Observed regression".into()).expect("revert");
    assert_eq!(c.rollout_ppm, 0);
    assert!(c.promote(acceptance(&c, "again"), &gate(), now()).is_err());
}
#[test]
fn safety_regression_and_rollout_failure_block_promotion() {
    let mut c = candidate();
    let mut e = improvement_eval();
    e.safety_failures = 1;
    assert!(c.record_evaluation(e, &gate(), now()).is_err());
    let mut e = improvement_eval();
    e.candidate_quality_ppm = 810_000;
    assert!(c.record_evaluation(e, &gate(), now()).is_err());
    c.record_evaluation(improvement_eval(), &gate(), now())
        .expect("evaluation");
    c.start_rollout(1000, acceptance(&c, "start"), &gate(), now())
        .expect("rollout");
    c.record_rollout_result(true).expect("result");
    c.record_rollout_result(true).expect("result");
    c.record_rollout_result(false).expect("result");
    assert!(
        c.promote(acceptance(&c, "promote"), &gate(), now())
            .is_err()
    );
    let mut wire = serde_json::to_value(c).expect("json");
    wire["capabilities"] = serde_json::json!(["admin"]);
    assert!(serde_json::from_value::<ImprovementCandidate>(wire).is_err());
}
