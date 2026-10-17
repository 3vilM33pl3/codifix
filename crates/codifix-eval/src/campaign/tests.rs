// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

fn test_tokenizer() -> provider_profile::Tokenizer {
    provider_profile::Tokenizer {
        id: "fixture-only-not-verified".into(),
        asset: provider_profile::Pin {
            path: "config.json".into(),
            sha256: digest(b"fixture"),
        },
    }
}

#[test]
fn provider_profile_binds_models_counters_rates_and_private_assets() {
    let original = fixture();
    original.validate().unwrap();
    for mutation in 0..9 {
        let mut manifest = original.clone();
        let profile = manifest.provider_profile.as_mut().unwrap();
        match mutation {
            0 => profile.embedding.resolved_id = "unapproved-embedding".into(),
            1 => profile.embedding.dimensions = 0,
            2 => profile.embedding.max_input_tokens = 8193,
            3 => profile.responses_tokenizer.asset.sha256 = digest(b"changed counter"),
            4 => profile.configuration_evidence.path = "source.md".into(),
            5 => profile.embedding.input_microusd_per_million = u64::MAX,
            6 => profile.protocol = "unknown".into(),
            7 => profile.embedding.tokenizer.id = "../counter".into(),
            _ => profile.version = 2,
        }
        assert!(manifest.validate().is_err(), "mutation {mutation}");
    }
    let mut changed_rate = original.clone();
    changed_rate
        .provider_profile
        .as_mut()
        .unwrap()
        .embedding
        .input_microusd_per_million += 1;
    changed_rate.validate().unwrap();
    assert_ne!(
        digest_value(&original).unwrap(),
        digest_value(&changed_rate).unwrap()
    );
    assert!(
        changed_rate
            .authorize(Split::HeldOut, &artifact(&original).approvals)
            .is_err()
    );
}

#[test]
fn full_context_profile_is_explicit_model_bound_and_has_sequence_headroom() {
    let mut manifest = fixture();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("input_accounting"));
    assert_eq!(
        serde_json::to_vec(&serde_json::from_slice::<Manifest>(&legacy).unwrap()).unwrap(),
        legacy
    );
    manifest.model.resolved_id = "gpt-5.4-mini-2026-03-17".into();
    manifest.model.embedding_id = "text-embedding-3-small".into();
    manifest.model.reasoning = "medium".into();
    manifest.model.max_input_tokens = 400_000;
    manifest.model.max_output_tokens = 4096;
    manifest.model.input_microusd_per_million = 750_000;
    manifest.model.output_microusd_per_million = 4_500_000;
    manifest.model.cached_input_microusd_per_million = Some(75_000);
    manifest
        .provider_budget
        .as_mut()
        .unwrap()
        .max_calls_per_task = 12;
    let profile = manifest.provider_profile.as_mut().unwrap();
    profile.version = 2;
    profile.input_accounting = Some("full_context_reservation".into());
    profile.responses_tokenizer.id = "full-context-responses-v1".into();
    profile.embedding.resolved_id = "text-embedding-3-small".into();
    profile.embedding.dimensions = 1536;
    profile.embedding.max_input_tokens = 8192;
    profile.embedding.input_microusd_per_million = 20_000;
    profile.embedding.tokenizer.id = "full-context-embeddings-v1".into();
    manifest.validate().unwrap();
    let allowance = manifest.task_allowance().unwrap();
    assert_eq!(allowance.input_tokens, 4_800_000);
    assert_eq!(allowance.output_tokens, 49_152);
    assert_eq!(allowance.microusd, 3_821_184);
    assert_eq!(manifest.task_token_limit().unwrap(), 4_849_152);
    assert_ne!(digest(&legacy), digest_value(&manifest).unwrap());
    for mutation in 0..10 {
        let mut invalid = manifest.clone();
        let profile = invalid.provider_profile.as_mut().unwrap();
        match mutation {
            0 => profile.version = 1,
            1 => profile.input_accounting = None,
            2 => invalid.model.resolved_id = "gpt-5.4-mini".into(),
            3 => invalid.model.max_input_tokens -= 1,
            4 => profile.embedding.max_input_tokens -= 1,
            5 => invalid.provider_budget = None,
            6 => invalid.model.input_microusd_per_million = 0,
            7 => invalid.model.reasoning = "max".into(),
            8 => profile.embedding.dimensions = 2,
            _ => profile.responses_tokenizer.id = "fake-counter".into(),
        }
        assert!(invalid.validate().is_err(), "mutation {mutation}");
    }
}

#[test]
fn missing_provider_profile_cannot_qualify_as_official_evidence() {
    let mut manifest = fixture();
    manifest.provider_profile = None;
    let mut evidence = artifact(&manifest);
    let rejected = report(&manifest, &evidence).unwrap();
    assert!(!rejected.harness_valid);
    assert!(
        rejected
            .errors
            .iter()
            .any(|e| e.contains("provider/tokenizer/embedding"))
    );
    evidence.offline = true;
    assert!(report(&manifest, &evidence).unwrap().harness_valid);
    assert!(
        manifest
            .authorize(Split::HeldOut, &evidence.approvals)
            .is_err()
    );
}

#[test]
fn optional_mediation_preserves_legacy_bytes_and_requires_explicit_bounded_calls() {
    let mut manifest = fixture();
    manifest.provider_budget = None;
    manifest.preparation_budget = None;
    manifest.provider_profile = None;
    let original = digest_value(&manifest).unwrap();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("provider_budget"));
    assert!(!String::from_utf8_lossy(&bytes).contains("preparation_budget"));
    assert!(!String::from_utf8_lossy(&bytes).contains("provider_profile"));
    let decoded: Manifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original, digest_value(&decoded).unwrap());
    for bound in [0, 129] {
        manifest.provider_budget = Some(ProviderBudget {
            max_calls_per_task: bound,
        });
        assert!(manifest.validate().is_err());
    }
    manifest.provider_budget = Some(ProviderBudget {
        max_calls_per_task: 6,
    });
    manifest.validate().unwrap();
    assert_ne!(original, digest_value(&manifest).unwrap());
}

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
        provider_profile: Some(provider_profile::ProviderProfile {
            version: 1,
            input_accounting: None,
            protocol: "openai-text-v1".into(),
            responses_tokenizer: test_tokenizer(),
            embedding: provider_profile::Embedding {
                resolved_id: "fake-2d".into(),
                dimensions: 2,
                max_input_tokens: 100,
                input_microusd_per_million: 1_000_000,
                tokenizer: test_tokenizer(),
            },
            configuration_evidence: test_tokenizer().asset,
        }),
        preparation_budget: Some(Limits {
            requests: 10,
            tokens: 100,
            microusd: 100,
            task_seconds: 60,
        }),
        provider_budget: Some(ProviderBudget {
            max_calls_per_task: 1,
        }),
        model: Model {
            provider: "openai".into(),
            resolved_id: "fake-v1".into(),
            reasoning: "none".into(),
            embedding_id: "fake-2d".into(),
            max_input_tokens: 100,
            max_output_tokens: 100,
            input_microusd_per_million: 1_000_000,
            output_microusd_per_million: 1_000_000,
            cached_input_microusd_per_million: None,
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
        preparation_microusd: Some(0),
        preparation_receipt: manifest.preparation_budget.as_ref().map(|limits| {
            preparation::PreparationReceipt {
                version: 1,
                snapshot_sha256: digest(b"snapshot"),
                audit: provider_results::ProviderAudit {
                    version: 1,
                    manifest_sha256: manifest_hash.clone(),
                    limits: limits.clone(),
                    task_limits: limits.clone(),
                    ledger: Ledger::default(),
                    tasks: BTreeMap::new(),
                    calls: vec![],
                },
            }
        }),
        approvals,
        observations: plan
            .work
            .into_iter()
            .enumerate()
            .map(|(index, work)| {
                let result = crate::EvalItemResult {
                    item_id: work.case_id.clone(),
                    eval_type: "grounded_answer".into(),
                    condition: crate::EvalCondition::FullMemory,
                    metadata: Default::default(),
                    success: work.condition == Condition::FullMemory,
                    skipped: false,
                    scores: Default::default(),
                    duration_ms: Some(10),
                    token_usage: Some(codifix_record::TokenUsage {
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
                    timing: Some(TaskTiming {
                        version: 1,
                        retrieval_ms: 1,
                        answer_or_agent_ms: 9,
                        grading_ms: 5,
                        executor_total_ms: 20,
                    }),
                    // Portable synthetic metadata for report tests only. Actual
                    // journal verification is covered by CLI integration tests.
                    provider_audit: manifest.provider_budget.as_ref().map(|profile| {
                        let limits = Limits {
                            requests: profile.max_calls_per_task,
                            tokens: manifest.model.max_input_tokens
                                + manifest.model.max_output_tokens,
                            microusd: budget::Allowance::for_model(
                                &manifest.model,
                                manifest.model.max_input_tokens,
                                manifest.model.max_output_tokens,
                            )
                            .unwrap()
                            .microusd,
                            task_seconds: manifest.limits.task_seconds,
                        };
                        let task_id = format!("work-{index:05}");
                        provider_results::ProviderAudit {
                            version: 1,
                            manifest_sha256: manifest_hash.clone(),
                            task_limits: limits.clone(),
                            limits,
                            ledger: Ledger {
                                requests: 1,
                                tokens: 10,
                                microusd: 10,
                            },
                            tasks: [(
                                task_id.clone(),
                                provider_results::TaskUsage {
                                    requests: 1,
                                    input_tokens: 5,
                                    output_tokens: 5,
                                    microusd: 10,
                                },
                            )]
                            .into(),
                            calls: vec![provider_results::ProviderCall {
                                task_id,
                                request_id: "synthetic-call".into(),
                                payload_sha256: digest(b"synthetic input"),
                                result_sha256: digest(b"synthetic result"),
                                allowance: budget::Allowance {
                                    input_tokens: 100,
                                    output_tokens: 100,
                                    microusd: 200,
                                },
                                usage: budget::Allowance {
                                    input_tokens: 5,
                                    output_tokens: 5,
                                    microusd: 10,
                                },
                            }],
                        }
                    }),
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
fn partial_smoke_rehearsals_keep_whole_families_and_cannot_be_promoted() {
    let mut m = fixture();
    m.smoke_only = true;
    m.cases
        .retain(|case| case.split == Split::HeldOut && case.cluster.ends_with("-0"));
    let plan = m.plan(Split::HeldOut).unwrap();
    assert_eq!(plan.work.len(), (4 + 1) * 5 * 3);
    assert!(m.plan(Split::Development).is_err());
    let mut evidence = artifact(&m);
    // Even fabricated approval-shaped fixtures cannot promote smoke evidence.
    assert!(m.authorize(Split::HeldOut, &evidence.approvals).is_err());
    evidence.offline = true;
    let result = report(&m, &evidence).unwrap();
    assert!(result.harness_valid);
    assert!(!result.official_evidence && !result.quality_passed);
    assert_eq!(result.tracks[0].tasks, 4);
    assert_eq!(result.tracks[1].tasks, 1);
    let complete_smoke = m.clone();
    m.smoke_only = false;
    assert!(m.validate().is_err());
    m = complete_smoke.clone();
    m.cases.remove(0);
    assert!(
        m.validate().is_err(),
        "knowledge families retain four questions"
    );
    m = complete_smoke;
    m.cases.retain(|case| case.track == Track::Knowledge);
    assert!(
        m.validate().is_err(),
        "both tracks remain required per split"
    );
    m.cases.clear();
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
    a.observations[0].provider_audit = None;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0].timing = None;
    assert!(!report(&m, &a).unwrap().harness_valid);
    let mut a = original.clone();
    a.observations[0]
        .timing
        .as_mut()
        .unwrap()
        .answer_or_agent_ms = 10;
    assert!(!report(&m, &a).unwrap().harness_valid);
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
fn legacy_unmediated_evidence_is_readable_but_cannot_be_official() {
    let mut m = fixture();
    m.provider_budget = None;
    m.preparation_budget = None;
    let mut a = artifact(&m);
    let rejected = report(&m, &a).unwrap();
    assert!(!rejected.harness_valid && !rejected.official_evidence);
    assert!(
        rejected
            .errors
            .iter()
            .any(|e| e.contains("per-call mediation profile"))
    );
    a.offline = true;
    let diagnostic = report(&m, &a).unwrap();
    assert!(
        diagnostic.harness_valid && !diagnostic.official_evidence && !diagnostic.quality_passed
    );
}

#[test]
fn preparation_metadata_is_required_for_official_evidence_and_binds_snapshot_and_cost() {
    let mut m = fixture();
    let mut a = artifact(&m);
    a.preparation_receipt = None;
    assert!(!report(&m, &a).unwrap().execution_valid);
    let mut a = artifact(&m);
    a.preparation_receipt.as_mut().unwrap().snapshot_sha256 = digest(b"changed snapshot");
    assert!(!report(&m, &a).unwrap().execution_valid);
    let mut a = artifact(&m);
    a.preparation_microusd = Some(1);
    assert!(!report(&m, &a).unwrap().execution_valid);
    m.preparation_budget = None;
    let mut a = artifact(&m);
    let r = report(&m, &a).unwrap();
    assert!(!r.execution_valid && r.errors.iter().any(|e| e.contains("preparation budget")));
    a.offline = true;
    assert!(report(&m, &a).unwrap().execution_valid);
}

#[test]
fn preparation_and_execution_share_request_token_and_spend_caps() {
    let with_preparation = |m: &Manifest| {
        let mut a = artifact(m);
        let mut audit = a.observations[0].provider_audit.clone().unwrap();
        let usage = audit.tasks.values().next().unwrap().clone();
        audit.tasks = [("preparation".into(), usage)].into();
        for call in &mut audit.calls {
            call.task_id = "preparation".into();
            call.allowance = call.usage;
        }
        audit.limits = m.preparation_budget.clone().unwrap();
        audit.task_limits = audit.limits.clone();
        a.preparation_microusd = Some(audit.ledger.microusd);
        a.preparation_receipt.as_mut().unwrap().audit = audit;
        a
    };
    let m = fixture();
    let a = with_preparation(&m);
    assert!(report(&m, &a).unwrap().execution_valid);
    let prep = &a.preparation_receipt.as_ref().unwrap().audit.ledger;
    let requests = a
        .observations
        .iter()
        .map(|o| o.provider_requests)
        .sum::<u64>()
        + prep.requests;
    let tokens = a
        .observations
        .iter()
        .map(|o| o.result.token_usage.as_ref().unwrap().total_tokens)
        .sum::<u64>()
        + prep.tokens;
    let cost = a
        .observations
        .iter()
        .map(|o| o.execution_microusd.unwrap())
        .sum::<u64>()
        + prep.microusd;
    for dimension in 0..3 {
        let mut m = fixture();
        match dimension {
            0 => m.limits.requests = requests - 1,
            1 => m.limits.tokens = tokens - 1,
            _ => m.limits.microusd = cost - 1,
        }
        let result = report(&m, &with_preparation(&m)).unwrap();
        assert!(!result.execution_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e == "campaign limits exceeded")
        );
    }
    let mut m = fixture();
    m.preparation_budget = None;
    m.limits.microusd = u64::MAX;
    let mut a = artifact(&m);
    a.offline = true;
    a.preparation_microusd = Some(u64::MAX);
    assert!(
        report(&m, &a)
            .unwrap()
            .errors
            .iter()
            .any(|e| e == "campaign limits exceeded")
    );
}

#[test]
fn zero_call_official_answers_cannot_impersonate_measured_model_work() {
    let m = fixture();
    let mut a = artifact(&m);
    let observation = &mut a.observations[0];
    observation.provider_requests = 0;
    observation.execution_microusd = Some(0);
    observation.result.token_usage = Some(Default::default());
    let audit = observation.provider_audit.as_mut().unwrap();
    audit.calls.clear();
    audit.tasks.clear();
    audit.ledger = Ledger::default();
    let result = report(&m, &a).unwrap();
    assert!(
        !result.harness_valid
            && result
                .errors
                .iter()
                .any(|e| e.contains("no accounted model invocation"))
    );
}

#[test]
fn task_timing_cannot_include_grading_in_execution_or_overflow() {
    let mut timing = TaskTiming {
        version: 1,
        retrieval_ms: 2,
        answer_or_agent_ms: 3,
        grading_ms: 7,
        executor_total_ms: 15,
    };
    timing.validate(Some(5)).unwrap();
    assert!(timing.validate(Some(12)).is_err());
    timing.executor_total_ms = 11;
    assert!(timing.validate(Some(5)).is_err());
    timing.retrieval_ms = u64::MAX;
    assert!(timing.validate(Some(5)).is_err());
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
    let pending_report = report(&m, &original).unwrap();
    assert!(!pending_report.harness_valid && pending_report.execution_valid);
    assert_eq!(pending_report.pending_knowledge_reviews, 1);
    assert!(!pending_report.official_evidence && !pending_report.quality_passed);
    let mut changed_snapshot = original.clone();
    changed_snapshot.observations[index].isolation.after_sha256 = digest(b"changed snapshot");
    assert!(!report(&m, &changed_snapshot).unwrap().execution_valid);
    let mut changed_answer = original.clone();
    changed_answer.observations[index]
        .knowledge_review
        .as_mut()
        .unwrap()
        .packet
        .answer
        .text
        .push_str(" changed");
    assert!(!report(&m, &changed_answer).unwrap().execution_valid);
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
    assert!(!report(&m, &broken).unwrap().execution_valid);
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
fn development_can_select_a_reference_but_cannot_authorize_held_out_execution() {
    let mut manifest = fixture();
    manifest.reference_baseline = None;
    assert!(
        manifest
            .plan(Split::Development)
            .unwrap()
            .readiness_errors
            .is_empty()
    );
    assert!(
        manifest
            .plan(Split::HeldOut)
            .unwrap()
            .readiness_errors
            .iter()
            .any(|e| e.contains("reference baseline"))
    );
    let approvals: Vec<_> = [ApprovalPurpose::Labels, ApprovalPurpose::Development]
        .into_iter()
        .map(|purpose| Approval {
            manifest_sha256: digest_value(&manifest).unwrap(),
            reviewer: "fixture-not-real-approval".into(),
            approved_at: Utc::now(),
            purpose,
            maximum_microusd: manifest.limits.microusd,
        })
        .collect();
    assert!(manifest.authorize(Split::Development, &approvals).is_ok());
    assert!(manifest.authorize(Split::HeldOut, &approvals).is_err());
    assert!(
        manifest
            .authorize(Split::Development, &approvals[1..])
            .is_err()
    );
    manifest.reference_baseline = Some(Condition::Lexical);
    // Changing the locked campaign still requires fresh matching approvals.
    assert!(manifest.authorize(Split::Development, &approvals).is_err());
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
fn small_development_calibration_requires_its_own_approvals_and_whole_families() {
    let mut manifest = fixture();
    manifest.smoke_only = true;
    manifest.reference_baseline = None;
    manifest
        .cases
        .retain(|case| case.split == Split::Development && case.cluster.ends_with("-0"));
    let plan = manifest.plan(Split::Development).unwrap();
    assert_eq!(plan.work.len(), 75);
    assert!(plan.readiness_errors.is_empty());
    assert!(manifest.authorize(Split::Development, &[]).is_err());
    let approvals: Vec<_> = [ApprovalPurpose::Labels, ApprovalPurpose::Development]
        .into_iter()
        .map(|purpose| Approval {
            manifest_sha256: plan.manifest_sha256.clone(),
            reviewer: "fixture-not-real-approval".into(),
            approved_at: Utc::now(),
            purpose,
            maximum_microusd: manifest.limits.microusd,
        })
        .collect();
    assert!(manifest.authorize(Split::Development, &approvals).is_ok());
    assert!(manifest.authorize(Split::HeldOut, &approvals).is_err());
    assert!(
        manifest
            .authorize(Split::Development, &approvals[..1])
            .is_err()
    );
    manifest.cases.remove(0);
    assert!(manifest.validate().is_err());
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
