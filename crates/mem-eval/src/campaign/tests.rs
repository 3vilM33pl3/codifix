// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

pub(super) fn fixture() -> Manifest {
    let mut cases = Vec::new();
    for (track, split, clusters, size) in [
        (Track::Knowledge, Split::Development, 10, 4),
        (Track::Knowledge, Split::HeldOut, 25, 4),
        (Track::Coding, Split::Development, 4, 1),
        (Track::Coding, Split::HeldOut, 12, 1),
    ] {
        for cluster in 0..clusters {
            for item in 0..size {
                let cluster = format!("{track:?}-{split:?}-{cluster}");
                cases.push(Case {
                    id: format!("{cluster}-{item}"),
                    cluster,
                    track,
                    split,
                    suite: "suite".into(),
                    sources: vec!["source.md".into()],
                });
            }
        }
    }
    let mut manifest = Manifest {
        version: 1,
        name: "r3-test".into(),
        semantic_time: "2026-01-01T00:00:00Z".parse().unwrap(),
        seed: 77,
        repeats: 3,
        label_status: "reviewed".into(),
        smoke_only: false,
        model: Model {
            provider: "fake".into(),
            resolved_id: "fake-v1".into(),
            reasoning: "none".into(),
            embedding_id: "fake-2d".into(),
            max_input_tokens: 100,
            max_output_tokens: 100,
            input_microusd_per_million: 1,
            output_microusd_per_million: 1,
        },
        limits: Limits {
            requests: 10000,
            tokens: 10000000,
            microusd: 10000000,
            task_seconds: 60,
        },
        reference_baseline: Some(Condition::Handover),
        assets: vec![
            ("source.md", AssetKind::Source),
            ("labels.json", AssetKind::Labels),
            ("fixture.rs", AssetKind::Fixture),
            ("config.json", AssetKind::Configuration),
        ]
        .into_iter()
        .map(|(p, k)| Asset {
            path: p.into(),
            sha256: digest(b"fixture"),
            kind: k,
        })
        .collect(),
        cases,
    };
    for case in &manifest.cases {
        if case.track == Track::Knowledge {
            manifest.assets.push(Asset {
                path: format!("labels/{}.json", case.id).into(),
                sha256: digest(&scoring::rubric_bytes(&rubric(&case.id)).unwrap()),
                kind: AssetKind::Labels,
            });
        }
    }
    manifest
}

pub(super) fn rubric(case_id: &str) -> scoring::Rubric {
    scoring::Rubric {
        case_id: case_id.into(),
        question: "Fixture-only question".into(),
        criteria: [
            scoring::Dimension::FactualCorrectness,
            scoring::Dimension::RequiredEvidence,
            scoring::Dimension::AppropriateRefusal,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, dimension)| scoring::Criterion {
            id: format!("criterion-{i}"),
            dimension,
            instruction: "Fixture-only decision".into(),
            critical: false,
        })
        .collect(),
        minimum_citations: 1,
        citations_critical: true,
    }
}

fn artifact(manifest: &Manifest) -> Artifact {
    let plan = manifest.plan(Split::HeldOut).unwrap();
    let manifest_hash = plan.manifest_sha256.clone();
    let approvals = [ApprovalPurpose::Labels, ApprovalPurpose::Official]
        .into_iter()
        .map(|purpose| Approval {
            manifest_sha256: plan.manifest_sha256.clone(),
            reviewer: "fixture-only".into(),
            approved_at: manifest.semantic_time,
            purpose,
            maximum_microusd: manifest.limits.microusd,
        })
        .collect();
    Artifact {
        manifest_sha256: plan.manifest_sha256,
        split: Split::HeldOut,
        offline: false,
        completed: true,
        preparation_microusd: Some(1),
        approvals,
        observations: plan
            .work
            .into_iter()
            .map(|work| {
                let result = crate::EvalItemResult {
                    item_id: work.case_id.clone(),
                    eval_type: "grounded_answer".into(),
                    condition: crate::EvalCondition::FullMemory,
                    metadata: Default::default(),
                    success: work.condition == Condition::FullMemory,
                    skipped: false,
                    scores: Default::default(),
                    duration_ms: Some(10),
                    token_usage: Some(mem_record::TokenUsage {
                        input_tokens: 5,
                        output_tokens: 5,
                        total_tokens: 10,
                        ..Default::default()
                    }),
                    answer: Some("Fixture-only answer".into()),
                    notes: vec![],
                    sub_results: vec![],
                };
                let knowledge_review = manifest
                    .cases
                    .iter()
                    .find(|c| c.id == work.case_id)
                    .filter(|c| c.track == Track::Knowledge)
                    .map(|_| {
                        let mut evidence = scoring::KnowledgeEvidence::prepare(
                            manifest_hash.clone(),
                            &work,
                            rubric(&work.case_id),
                            vec![scoring::Source {
                                id: "source.md".into(),
                                text: "fixture".into(),
                            }],
                            scoring::Answer {
                                text: "Fixture-only answer".into(),
                                refused: false,
                                citations: vec![scoring::Citation {
                                    source_id: "source.md".into(),
                                    quote: "fixture".into(),
                                }],
                            },
                        )
                        .unwrap();
                        let mut ballot = evidence
                            .packet
                            .blank_ballot("fixture-only-not-real-approval".into())
                            .unwrap();
                        for judgment in ballot.judgments.values_mut() {
                            judgment.decision = if result.success {
                                scoring::Decision::Pass
                            } else {
                                scoring::Decision::Fail
                            };
                            judgment.reason =
                                "Synthetic test decision, not benchmark evidence".into();
                        }
                        evidence.ballots.push(ballot);
                        evidence
                    });
                Observation {
                    knowledge_review,
                    model: manifest.model.clone(),
                    work,
                    result,
                    execution_microusd: Some(10),
                    provider_requests: 1,
                    uncertain_charge: false,
                    critical_failure: false,
                    unresolved_rubric: false,
                    isolation: Isolation {
                        before_sha256: digest(b"snapshot"),
                        after_sha256: digest(b"snapshot"),
                        snapshot_sha256: digest(b"snapshot"),
                        configuration_sha256: digest(b"fixture"),
                        driver_sha256: digest(b"fixture"),
                        service_build: "fixture-only".into(),
                        semantic_time: manifest.semantic_time,
                        sealed: true,
                        restricted_network: true,
                        hidden_tests_inaccessible: true,
                    },
                }
            })
            .collect(),
    }
}

#[test]
fn schedule_is_reproducible_complete_and_changes_with_seed() {
    let mut m = fixture();
    let a = m.plan(Split::HeldOut).unwrap();
    assert_eq!(a.work.len(), (100 + 12) * 5 * 3);
    assert_eq!(a.work, m.plan(Split::HeldOut).unwrap().work);
    m.seed += 1;
    assert_ne!(a.work, m.plan(Split::HeldOut).unwrap().work);
    m.cases[0].cluster = m.cases.last().unwrap().cluster.clone();
    assert!(m.validate().is_err());
}

#[test]
fn parent_results_and_repeats_do_not_inflate_independence() {
    let m = fixture();
    let mut a = artifact(&m);
    let report = report(&m, &a).unwrap();
    assert!(report.quality_passed);
    assert_eq!(report.execution_microusd, 16800);
    assert_eq!(report.tracks[0].independent_clusters, 25);
    assert_eq!(report.tracks[0].tasks, 100);
    assert_eq!(report.tracks[1].independent_clusters, 12);
    assert_eq!(report.tracks[1].total_executions, 36);
    assert_eq!(report.tracks[0].comparisons[0].simultaneous_lower, 1.0);
    for track in &report.tracks {
        assert_eq!(track.conditions.len(), 5);
        for condition in &track.conditions {
            assert_eq!(condition.tasks, track.tasks);
            assert_eq!(condition.total_executions, track.tasks * 3);
            assert_eq!(
                condition.successful_executions,
                if condition.condition == Condition::FullMemory {
                    track.tasks * 3
                } else {
                    0
                }
            );
            assert_eq!(condition.execution_microusd, (track.tasks * 3 * 10) as u64);
            assert_eq!(condition.total_tokens, (track.tasks * 3 * 10) as u64);
            assert_eq!(condition.critical_failures, 0);
            assert_eq!(condition.median_duration_ms, 10.0);
        }
    }
    a.offline = true;
    let offline = super::report(&m, &a).unwrap();
    assert!(offline.harness_valid);
    assert!(!offline.official_evidence && !offline.quality_passed);
}

#[test]
fn invalid_evidence_fails_closed() {
    let m = fixture();
    let original = artifact(&m);
    let mut a = original.clone();
    a.observations.pop();
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].result.token_usage = None;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].isolation.after_sha256 = digest(b"changed");
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].isolation.hidden_tests_inaccessible = false;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].uncertain_charge = true;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.completed = false;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].critical_failure = true;
    let critical = report(&m, &a).unwrap();
    assert!(!critical.quality_passed);
    assert_eq!(
        critical
            .tracks
            .iter()
            .flat_map(|t| &t.conditions)
            .map(|c| c.critical_failures)
            .sum::<usize>(),
        1
    );
    let mut a = original;
    a.approvals.clear();
    assert!(!report(&m, &a).unwrap().official_evidence);
}

#[test]
fn official_knowledge_scores_require_bound_independent_review() {
    let m = fixture();
    let a = artifact(&m);
    let original = a
        .observations
        .iter()
        .find(|o| o.knowledge_review.is_some())
        .unwrap();
    scoring::validate_observation(&m, original, true).unwrap();
    let mut missing = original.clone();
    missing.knowledge_review = None;
    assert!(scoring::validate_observation(&m, &missing, true).is_err());
    scoring::validate_observation(&m, &missing, false).unwrap(); // legacy smoke only
    let mut tampered = vec![original.clone(); 7];
    tampered[0].result.success = !original.result.success;
    tampered[1].result.answer = Some("changed".into());
    tampered[2]
        .knowledge_review
        .as_mut()
        .unwrap()
        .packet
        .sources[0]
        .text = "new facts".into();
    tampered[3]
        .knowledge_review
        .as_mut()
        .unwrap()
        .ballots
        .clear();
    tampered[4].work.repeat = (original.work.repeat + 1) % 3;
    tampered[5]
        .knowledge_review
        .as_mut()
        .unwrap()
        .packet
        .rubric
        .criteria[0]
        .instruction = "new rubric".into();
    tampered[6]
        .knowledge_review
        .as_mut()
        .unwrap()
        .packet
        .sources
        .push(scoring::Source {
            id: "hidden.md".into(),
            text: "hidden answer".into(),
        });
    for observation in tampered {
        assert!(scoring::validate_observation(&m, &observation, true).is_err());
        assert!(scoring::validate_observation(&m, &observation, false).is_err());
    }
}

#[test]
fn blinded_campaign_review_retains_execution_and_revision_history() {
    let m = fixture();
    let mut original = artifact(&m);
    original.offline = true;
    let index = original
        .observations
        .iter()
        .position(|o| o.knowledge_review.is_some())
        .unwrap();
    let pending = &mut original.observations[index];
    pending.knowledge_review.as_mut().unwrap().ballots.clear();
    pending.unresolved_rubric = true;
    pending.result.success = false;
    assert!(!report(&m, &original).unwrap().harness_valid);
    let packets = review::prepare(&m, &original).unwrap();
    assert_eq!(packets.len(), 1500);
    let packet = original.observations[index]
        .knowledge_review
        .as_ref()
        .unwrap()
        .packet
        .clone();
    let mut human = packet
        .blank_ballot("synthetic-first-reviewer".into())
        .unwrap();
    for j in human.judgments.values_mut() {
        j.decision = scoring::Decision::Pass;
        j.reason = "Synthetic test decision".into();
    }
    let first_batch = review::ReviewBatch {
        artifact_sha256: digest_value(&original).unwrap(),
        updates: vec![review::BallotUpdate {
            ballot: human.clone(),
            replaces_sha256: None,
        }],
    };
    let accepted = review::apply(&m, &original, &first_batch).unwrap();
    assert!(report(&m, &accepted).unwrap().harness_valid);
    assert_eq!(
        accepted.observations[index].result.duration_ms,
        original.observations[index].result.duration_ms
    );
    assert_eq!(
        accepted.observations[index].execution_microusd,
        original.observations[index].execution_microusd
    );
    assert_eq!(
        digest_value(&accepted).unwrap(),
        digest_value(&review::apply(&m, &original, &first_batch).unwrap()).unwrap()
    );
    assert!(review::apply(&m, &accepted, &first_batch).is_err());
    let mut dissent = human.clone();
    dissent.reviewer = "synthetic-second-reviewer".into();
    dissent.judgments.values_mut().next().unwrap().decision = scoring::Decision::Fail;
    let conflicting = review::apply(
        &m,
        &accepted,
        &review::ReviewBatch {
            artifact_sha256: digest_value(&accepted).unwrap(),
            updates: vec![review::BallotUpdate {
                ballot: dissent.clone(),
                replaces_sha256: None,
            }],
        },
    )
    .unwrap();
    assert!(conflicting.observations[index].unresolved_rubric);
    assert!(!report(&m, &conflicting).unwrap().harness_valid);
    let mut correction = dissent.clone();
    for j in correction.judgments.values_mut() {
        j.decision = scoring::Decision::Pass;
        j.reason = "Synthetic reviewer revisited source evidence".into();
    }
    let mut resolution = review::ReviewBatch {
        artifact_sha256: digest_value(&conflicting).unwrap(),
        updates: vec![review::BallotUpdate {
            ballot: correction,
            replaces_sha256: None,
        }],
    };
    assert!(review::apply(&m, &conflicting, &resolution).is_err());
    resolution.updates[0].replaces_sha256 = Some(digest_value(&dissent).unwrap());
    let resolved = review::apply(&m, &conflicting, &resolution).unwrap();
    let evidence = resolved.observations[index]
        .knowledge_review
        .as_ref()
        .unwrap();
    assert_eq!(evidence.history.len(), 1);
    assert_eq!(evidence.ballots.len(), 2);
    assert_eq!(
        digest_value(&evidence.history[0].ballot).unwrap(),
        digest_value(&dissent).unwrap()
    );
    assert!(report(&m, &resolved).unwrap().harness_valid);
    let mut tampered = resolved;
    tampered.observations[index]
        .knowledge_review
        .as_mut()
        .unwrap()
        .history[0]
        .replacement_sha256 = digest(b"forged");
    assert!(!report(&m, &tampered).unwrap().harness_valid);
    let mut broken = original;
    broken.observations[index].result.token_usage = None;
    assert!(review::prepare(&m, &broken).is_err());
}

#[test]
fn draft_labels_or_changed_manifest_cannot_be_authorized() {
    let mut m = fixture();
    let a = artifact(&m);
    assert!(m.authorize(Split::HeldOut, &a.approvals).is_ok());
    m.label_status = "draft".into();
    assert!(m.authorize(Split::HeldOut, &a.approvals).is_err());
    m.label_status = "reviewed".into();
    m.seed += 1;
    assert!(m.authorize(Split::HeldOut, &a.approvals).is_err());
}

#[test]
fn smoke_and_legacy_manifests_cannot_be_promoted_by_label_approval() {
    let mut m = fixture();
    m.smoke_only = true;
    let a = artifact(&m);
    assert!(m.authorize(Split::HeldOut, &a.approvals).is_err());
    assert!(!report(&m, &a).unwrap().official_evidence);
    let mut legacy = serde_json::to_value(fixture()).unwrap();
    legacy.as_object_mut().unwrap().remove("smoke_only");
    let legacy: Manifest = serde_json::from_value(legacy).unwrap();
    assert!(legacy.smoke_only);
}

#[test]
fn reservation_is_atomic_bounded_and_conservative_on_uncertain_execution() {
    let limits = Limits {
        requests: 2,
        tokens: 100,
        microusd: 200,
        task_seconds: 1,
    };
    let mut ledger = Ledger::default();
    ledger.reserve(&limits, 60, 100).unwrap();
    assert!(ledger.reserve(&limits, 60, 100).is_err());
    assert_eq!(ledger.requests, 1);
    assert_eq!(ledger.tokens, 60);
    ledger.reserve(&limits, 40, 100).unwrap();
    assert!(ledger.reserve(&limits, 0, 0).is_err());
    assert_eq!(ledger.microusd, 200);
}

#[test]
fn simultaneous_intervals_widen_for_correlated_cluster_uncertainty() {
    let clusters: Vec<_> = (0..25)
        .map(|i| if i < 13 { [1.0; 3] } else { [-1.0; 3] })
        .collect();
    let (mean, radius) = super::statistics::simultaneous_interval(&clusters, 77);
    assert!((mean[0] - 0.04).abs() < 1e-9);
    assert!(radius > 0.2 && radius < 0.7);
    let again = super::statistics::simultaneous_interval(&clusters, 77);
    assert_eq!((mean, radius), again);
}
