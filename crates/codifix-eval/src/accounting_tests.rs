// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

fn run(condition: EvalCondition, repeat_index: usize, success: bool) -> EvalRun {
    let mut result = tests::sequence_result(success, "deductive");
    result.condition = condition;
    result.metadata.reasoning_mode = Some("deductive".into());
    result.token_usage = Some(TokenUsage {
        total_tokens: 30,
        ..Default::default()
    });
    result.duration_ms = Some(100);
    let mut first = result.sub_results[0].clone();
    first.token_usage = Some(TokenUsage {
        total_tokens: 10,
        ..Default::default()
    });
    first.duration_ms = Some(20);
    let mut second = first.clone();
    second.id = "second".into();
    second.token_usage = Some(TokenUsage {
        total_tokens: 20,
        ..Default::default()
    });
    second.duration_ms = Some(40);
    result.sub_results = vec![first, second];
    EvalRun {
        suite: "accounting".into(),
        project: "isolated".into(),
        condition,
        profile: EvalProfile::Offline,
        run_group_id: Uuid::new_v4(),
        repeat_index,
        suite_checksum: None,
        fixture_checksum: None,
        config_fingerprint: None,
        dry_run: false,
        created_at: Utc::now(),
        git_head: None,
        service_version: None,
        results: vec![result],
    }
}

#[test]
fn aggregate_parent_cost_is_counted_once_and_steps_cannot_inflate_success() {
    let baseline = run(EvalCondition::NoMemory, 0, false);
    let mut candidate = run(EvalCondition::FullMemory, 0, true);
    candidate.results[0]
        .token_usage
        .as_mut()
        .unwrap()
        .total_tokens = 45;
    // A failing step diagnostic must not be a second whole-task observation.
    candidate.results[0].sub_results[1].success = false;
    let comparison = compare_run_sets(&[baseline], &[candidate]);
    assert_eq!(comparison.basis, EvalComparisonBasis::TaskLevel);
    assert_eq!(comparison.paired_items, 1);
    assert_eq!(comparison.baseline_total_tokens, 30); // not parent 30 + children 30
    assert_eq!(comparison.candidate_total_tokens, 45);
    assert_eq!(comparison.token_delta, 15);
    assert_eq!(comparison.baseline_mean_duration_ms, 100.0);
    assert_eq!(comparison.success_rate_delta, 1.0);
    assert_eq!(comparison.mcnemar_b, 1);
    let tasks = &comparison.groups["reasoning_mode:deductive"];
    let steps = &comparison.groups["step:reasoning_mode:deductive"];
    assert_eq!(tasks.paired_items, 1);
    assert_eq!(steps.paired_items, 2);
    assert_eq!(steps.candidate_success_rate, 0.5);
    assert_eq!(steps.baseline_total_tokens, 30);
    assert_eq!(steps.baseline_mean_duration_ms, 30.0);
    assert!(comparison_text(&comparison).contains("paired whole tasks only"));
}

#[test]
fn unmatched_steps_do_not_change_task_comparison_and_repeats_remain_distinct() {
    let baselines = vec![
        run(EvalCondition::NoMemory, 0, false),
        run(EvalCondition::NoMemory, 1, true),
    ];
    let mut candidates = vec![
        run(EvalCondition::FullMemory, 0, true),
        run(EvalCondition::FullMemory, 1, true),
    ];
    candidates[0].results[0].sub_results.pop();
    let comparison = compare_run_sets(&baselines, &candidates);
    assert_eq!(comparison.paired_items, 2);
    assert_eq!(comparison.baseline_total_tokens, 60);
    assert_eq!(comparison.candidate_total_tokens, 60);
    assert_eq!(
        comparison.groups["step:reasoning_mode:deductive"].paired_items,
        3
    );
    assert_eq!(comparison.success_rate_delta, 0.5);
}

#[test]
fn skipped_parents_cannot_contribute_or_leak_step_diagnostics() {
    let baseline = run(EvalCondition::NoMemory, 0, true);
    let mut candidate = run(EvalCondition::FullMemory, 0, true);
    candidate.results[0].skipped = true;
    let comparison = compare_run_sets(&[baseline], &[candidate]);
    assert_eq!(comparison.paired_items, 0);
    assert_eq!(comparison.candidate_total_tokens, 0);
    assert!(comparison.groups.is_empty());
}

#[test]
fn missing_parent_usage_is_not_invented_from_partial_step_usage() {
    let mut baseline = run(EvalCondition::NoMemory, 0, true);
    let mut candidate = run(EvalCondition::FullMemory, 0, true);
    baseline.results[0].token_usage = None;
    candidate.results[0].token_usage = None;
    let comparison = compare_run_sets(&[baseline], &[candidate]);
    assert_eq!(comparison.baseline_total_tokens, 0);
    assert_eq!(
        comparison.groups["step:reasoning_mode:deductive"].baseline_total_tokens,
        30
    );
    assert!(comparison_markdown(&comparison).contains("not evidence of zero cost"));
}

#[test]
fn legacy_comparisons_remain_readable_but_require_recomparison_before_gating() {
    let comparison = compare_run_sets(
        &[run(EvalCondition::NoMemory, 0, true)],
        &[run(EvalCondition::FullMemory, 0, true)],
    );
    let policy: EvalGatePolicy = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(evaluate_gate(&comparison, &policy).passed);
    let mut json = serde_json::to_value(&comparison).unwrap();
    json.as_object_mut().unwrap().remove("basis");
    let legacy: EvalComparison = serde_json::from_value(json).unwrap();
    assert_eq!(legacy.basis, EvalComparisonBasis::LegacyMixed);
    assert!(comparison_text(&legacy).contains("WARNING"));
    let gate = evaluate_gate(&legacy, &policy);
    assert!(!gate.passed);
    assert!(gate.reasons[0].contains("regenerate"));
}
