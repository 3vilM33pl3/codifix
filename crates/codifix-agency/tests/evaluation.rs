use codifix_agency::{PolicyError, evaluation::*};
use uuid::Uuid;
fn plan() -> EvaluationPlan {
    serde_json::from_str(include_str!("fixtures/synthetic-evaluation-plan.json"))
        .expect("synthetic plan")
}
fn outcomes() -> Vec<OutcomeRecord> {
    serde_json::from_str(include_str!("fixtures/synthetic-evaluation-outcomes.json"))
        .expect("synthetic outcomes")
}
#[test]
fn synthetic_matched_report_exposes_tradeoffs_without_claiming_benefit() {
    let report = compare_outcomes(&plan(), &outcomes()).expect("matched synthetic report");
    assert!(report.plan.synthetic);
    assert!(report.interpretation.contains("Synthetic"));
    assert!(report.interpretation.contains("no real model benefit"));
    let deterministic = &report.conditions[&EvaluationCondition::Deterministic];
    assert_eq!(deterministic.verified_successes, 1);
    assert_eq!(deterministic.total_micro_usd, 4);
    let network = &report.conditions[&EvaluationCondition::SpecialistNetwork];
    assert_eq!(network.verified_successes, 2);
    assert_eq!(network.total_micro_usd, 70);
    assert_eq!(network.local_compute_micro_usd, 20);
    assert_eq!(network.human_interventions, 1);
    let pair = report
        .comparisons
        .iter()
        .find(|p| {
            p.baseline == EvaluationCondition::SingleAgent
                && p.comparison == EvaluationCondition::SpecialistNetwork
        })
        .expect("paired conditions");
    assert_eq!(pair.matched_cases, 2);
    assert_eq!(pair.success_delta, "1");
    assert_eq!(pair.total_micro_usd_delta, "30");
    assert_eq!(pair.total_latency_ms_delta, "100");
    assert_eq!(pair.human_interventions_delta, "1");
    assert_eq!(pair.comparison_only_successes, 1);
    let wire = serde_json::to_value(&report).expect("portable JSON report");
    assert!(wire.get("winner").is_none());
}
#[test]
fn missing_extra_or_duplicate_conditions_and_evidence_are_rejected() {
    let mut missing = outcomes();
    missing.pop();
    assert!(compare_outcomes(&plan(), &missing).is_err());
    let mut extra = outcomes();
    extra.push(extra[0].clone());
    assert!(compare_outcomes(&plan(), &extra).is_err());
    let mut duplicate = outcomes();
    duplicate[1] = duplicate[0].clone();
    assert!(compare_outcomes(&plan(), &duplicate).is_err());
    let mut duplicate_receipt = outcomes();
    duplicate_receipt[1].evidence.receipt_ref = duplicate_receipt[0].evidence.receipt_ref.clone();
    assert!(compare_outcomes(&plan(), &duplicate_receipt).is_err());
    let mut duplicate_digest = outcomes();
    duplicate_digest[1].evidence.receipt_sha256 =
        duplicate_digest[0].evidence.receipt_sha256.clone();
    assert!(compare_outcomes(&plan(), &duplicate_digest).is_err());
    let mut missing_condition = plan();
    missing_condition
        .conditions
        .remove(&EvaluationCondition::Deterministic);
    assert!(compare_outcomes(&missing_condition, &outcomes()).is_err());
}
#[test]
fn mismatched_case_dataset_task_model_procedure_verifier_or_synthetic_binding_is_rejected() {
    let mut variants = vec![];
    let mut x = outcomes();
    x[0].dataset_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].task_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].case_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].case_id = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].model_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].procedure_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].evidence.verifier_revision = "other".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].synthetic = false;
    variants.push(x);
    let mut x = outcomes();
    x[0].held_out_set_id = "training-synthetic-v1".into();
    variants.push(x);
    for x in variants {
        assert!(compare_outcomes(&plan(), &x).is_err());
    }
}
#[test]
fn evidence_free_or_unverified_success_and_invalid_costs_fail_closed() {
    let mut variants = vec![];
    let mut x = outcomes();
    x[0].evidence.receipt_ref.clear();
    variants.push(x);
    let mut x = outcomes();
    x[0].evidence.receipt_sha256 = "fake".into();
    variants.push(x);
    let mut x = outcomes();
    x[0].evidence.verified_success = false;
    variants.push(x);
    let mut x = outcomes();
    x[0].local_compute_micro_usd = x[0].total_micro_usd + 1;
    variants.push(x);
    let mut x = outcomes();
    x[0].latency_ms = 0;
    variants.push(x);
    let mut x = outcomes();
    x[0].task_id = Uuid::nil();
    variants.push(x);
    for x in variants {
        assert!(compare_outcomes(&plan(), &x).is_err());
    }
    let mut leaked = plan();
    leaked.training_set_id = leaked.held_out_set_id.clone();
    assert!(compare_outcomes(&leaked, &outcomes()).is_err());
}
#[test]
fn accounting_overflow_is_rejected_and_negative_deltas_are_exact_json_strings() {
    let mut x = outcomes();
    x[0].total_micro_usd = u64::MAX;
    assert_eq!(
        compare_outcomes(&plan(), &x),
        Err(PolicyError::Invalid("evaluation metric overflow"))
    );
    x[1].total_micro_usd = 0;
    x[1].local_compute_micro_usd = 0;
    let report = compare_outcomes(&plan(), &x).expect("u64 total");
    let wire = serde_json::to_value(report).expect("portable wide negative difference");
    assert_eq!(
        wire["comparisons"][0]["total_micro_usd_delta"],
        "-18446744073709551575"
    );
}
