// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

#[tokio::test]
async fn paid_dispatch_is_rejected_before_reading_files_or_starting_processes() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "held-out".into(),
        allow_shell: true,
        offline: false,
        resume: false,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("paid campaign execution remains unavailable")
    );
}

#[tokio::test]
async fn offline_driver_requires_explicit_shell_consent() {
    let error = handle(EvalCampaignCommand::Run {
        manifest: "must-not-read".into(),
        driver: "must-not-execute".into(),
        out: "must-not-create".into(),
        split: "development".into(),
        allow_shell: false,
        offline: true,
        resume: false,
        approvals: vec![],
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("--allow-shell"));
}

#[test]
fn evidence_files_cannot_be_overwritten() {
    let root = std::env::temp_dir().join(format!("memory-r3-evidence-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("receipt.json");
    exclusive_json(&path, &1).unwrap();
    assert!(exclusive_json(&path, &2).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "1");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
    assert!(split("heldout").is_err());
    assert_eq!(split("held-out").unwrap(), Split::HeldOut);
}

#[test]
fn mediated_driver_counts_must_match_actual_locked_journal_and_receipts() {
    let root = Scratch::new();
    let (mut manifest, mut observation) = fixture();
    manifest.provider_budget = Some(c::ProviderBudget {
        max_calls_per_task: 4,
    });
    let hash = c::digest_value(&manifest).unwrap();
    let contract = provider_mediation::contract(&manifest, &hash, &root.0, "work-00000", true, 0)
        .unwrap()
        .unwrap();
    provider_mediation::prepare(Some(&contract)).unwrap();
    let directory = root.0.join("provider");
    let limits = c::Limits {
        requests: 4,
        tokens: manifest.model.max_input_tokens + manifest.model.max_output_tokens,
        microusd: 0,
        task_seconds: manifest.limits.task_seconds,
    };
    let mut budget = RequestBudget::create_scoped(
        &directory.join("budget.jsonl"),
        &hash,
        limits.clone(),
        limits,
    )
    .unwrap();
    let results = ProviderResults::create(&directory.join("results")).unwrap();
    for id in ["call-1", "call-2"] {
        results
            .dispatch(
                &mut budget,
                "work-00000",
                id,
                &c::digest(b"synthetic-input"),
                Allowance {
                    input_tokens: 20,
                    output_tokens: 10,
                    microusd: 0,
                },
                || {
                    Ok((
                        serde_json::json!("synthetic output"),
                        Allowance {
                            input_tokens: 3,
                            output_tokens: 2,
                            microusd: 0,
                        },
                    ))
                },
            )
            .unwrap();
    }
    observation.provider_requests = 2;
    let usage = observation.result.token_usage.as_mut().unwrap();
    usage.input_tokens = 6;
    usage.output_tokens = 4;
    usage.total_tokens = 10;
    observation.provider_audit = Some(results.audit(&mut budget).unwrap());
    assert!(
        provider_mediation::verify(
            &manifest,
            &hash,
            "work-00000",
            true,
            Some(&contract),
            &observation
        )
        .is_err(),
        "live provider owner must retain the lock"
    );
    drop(budget);
    provider_mediation::verify(
        &manifest,
        &hash,
        "work-00000",
        true,
        Some(&contract),
        &observation,
    )
    .unwrap();
    let mut forged = observation.clone();
    let audit = forged.provider_audit.as_mut().unwrap();
    let mut call = audit.calls[0].clone();
    call.request_id = "invented-call".into();
    audit.calls.push(call);
    audit.ledger.requests = 3;
    audit.ledger.tokens = 15;
    let task = audit.tasks.get_mut("work-00000").unwrap();
    task.requests = 3;
    task.input_tokens = 9;
    task.output_tokens = 6;
    forged.provider_requests = 3;
    let usage = forged.result.token_usage.as_mut().unwrap();
    usage.input_tokens = 9;
    usage.output_tokens = 6;
    usage.total_tokens = 15;
    forged
        .validate_provider_audit(&manifest, &hash, "work-00000", true)
        .unwrap();
    assert!(
        provider_mediation::verify(
            &manifest,
            &hash,
            "work-00000",
            true,
            Some(&contract),
            &forged
        )
        .is_err(),
        "self-consistent metadata is not journal evidence"
    );
    let mut request = serde_json::json!({"version":1});
    let last = provider_mediation::contract(
        &manifest,
        &hash,
        &root.0,
        "work-00001",
        true,
        manifest.limits.requests - 1,
    )
    .unwrap();
    provider_mediation::bind(&mut request, last.as_ref()).unwrap();
    assert_eq!(request["provider_budget"]["limits"]["requests"], 1);
    assert!(
        provider_mediation::contract(
            &manifest,
            &hash,
            &root.0,
            "work-00001",
            true,
            manifest.limits.requests
        )
        .is_err()
    );
    std::fs::write(
        directory
            .join("results")
            .join(format!("{}.json", c::digest(b"call-1"))),
        b"{}",
    )
    .unwrap();
    assert!(
        provider_mediation::verify(
            &manifest,
            &hash,
            "work-00000",
            true,
            Some(&contract),
            &observation
        )
        .is_err()
    );
}

fn fixture() -> (Manifest, Observation) {
    let mut cases = Vec::new();
    for (track, split, count, size) in [
        (c::Track::Knowledge, Split::Development, 10, 4),
        (c::Track::Knowledge, Split::HeldOut, 25, 4),
        (c::Track::Coding, Split::Development, 4, 1),
        (c::Track::Coding, Split::HeldOut, 12, 1),
    ] {
        for cluster in 0..count {
            for item in 0..size {
                let cluster = format!("{track:?}-{split:?}-{cluster}");
                cases.push(c::Case {
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
    let model = c::Model {
        provider: "fake".into(),
        resolved_id: "fake-v1".into(),
        reasoning: "none".into(),
        embedding_id: "fake-vector".into(),
        max_input_tokens: 100,
        max_output_tokens: 100,
        input_microusd_per_million: 0,
        output_microusd_per_million: 0,
    };
    let manifest = Manifest {
        version: 1,
        name: "fixture".into(),
        semantic_time: "2026-09-10T00:00:00Z".parse().unwrap(),
        seed: 1,
        repeats: 3,
        label_status: "draft".into(),
        smoke_only: true,
        provider_budget: None,
        model: model.clone(),
        limits: c::Limits {
            requests: 10000,
            tokens: 10000000,
            microusd: 0,
            task_seconds: 5,
        },
        reference_baseline: None,
        assets: [
            ("source.md", c::AssetKind::Source),
            ("labels.json", c::AssetKind::Labels),
            ("fixture.rs", c::AssetKind::Fixture),
            ("driver.sh", c::AssetKind::Configuration),
        ]
        .into_iter()
        .map(|(name, kind)| c::Asset {
            path: name.into(),
            sha256: c::digest(b"fixture"),
            kind,
        })
        .collect(),
        cases,
    };
    let work = manifest.plan(Split::Development).unwrap().work[0].clone();
    let observation = Observation {
        timing: Some(c::TaskTiming {
            version: 1,
            retrieval_ms: 0,
            answer_or_agent_ms: 1,
            grading_ms: 0,
            executor_total_ms: 1,
        }),
        provider_audit: None,
        knowledge_review: None,
        work: work.clone(),
        model,
        result: mem_eval::EvalItemResult {
            item_id: work.case_id,
            eval_type: "fixture".into(),
            condition: mem_eval::EvalCondition::FullMemory,
            metadata: Default::default(),
            success: false,
            skipped: false,
            scores: Default::default(),
            duration_ms: Some(1),
            token_usage: Some(mem_record::TokenUsage {
                input_tokens: 5,
                output_tokens: 5,
                total_tokens: 10,
                ..Default::default()
            }),
            answer: None,
            notes: vec![],
            sub_results: vec![],
        },
        execution_microusd: Some(0),
        provider_requests: 1,
        uncertain_charge: false,
        critical_failure: false,
        unresolved_rubric: false,
        isolation: c::Isolation {
            before_sha256: c::digest(b"snapshot"),
            after_sha256: c::digest(b"snapshot"),
            snapshot_sha256: c::digest(b"snapshot"),
            configuration_sha256: c::digest(b"config"),
            driver_sha256: c::digest(b"fixture"),
            service_build: "fake-build".into(),
            semantic_time: manifest.semantic_time,
            sealed: true,
            restricted_network: true,
            hidden_tests_inaccessible: true,
        },
    };
    (manifest, observation)
}

#[test]
fn invalid_measurement_stops_but_negative_quality_is_valid_evidence() {
    let (manifest, observation) = fixture();
    let check = |o: &Observation| {
        validate_task_evidence(&manifest, &observation.work, o, Some(&observation))
    };
    assert!(
        check(&observation).is_ok(),
        "a wrong answer must remain measurable"
    );
    let mutations: Vec<fn(&mut Observation)> = vec![
        |o| o.model.resolved_id = "another-model".into(),
        |o| o.result.item_id = "other-task".into(),
        |o| o.result.skipped = true,
        |o| o.result.duration_ms = None,
        |o| o.result.token_usage = None,
        |o| o.result.token_usage.as_mut().unwrap().total_tokens = 1,
        |o| {
            let u = o.result.token_usage.as_mut().unwrap();
            u.input_tokens = 101;
            u.total_tokens = 106;
        },
        |o| {
            let u = o.result.token_usage.as_mut().unwrap();
            u.output_tokens = 101;
            u.total_tokens = 106;
        },
        |o| o.uncertain_charge = true,
        |o| o.unresolved_rubric = true,
        |o| o.isolation.after_sha256 = c::digest(b"changed"),
        |o| o.isolation.configuration_sha256 = c::digest(b"different-config"),
        |o| o.isolation.service_build = "different-build".into(),
    ];
    for mutate in mutations {
        let mut invalid = observation.clone();
        mutate(&mut invalid);
        assert!(check(&invalid).is_err());
    }
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("memory-r3-runner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn driver_dispatch_settles_only_valid_evidence_and_preserves_uncertainty() {
    use std::os::unix::fs::PermissionsExt;
    for (index, command) in [
        "printf changed > ../../source.md",
        "printf '{}' > ../../campaign.json",
        "sed -i 's/\"seed\": 1,/\"seed\": 2,/' ../../campaign.json",
        "exit 17",
        "true",
        "cp ../../observation.json observation.json\nprintf '0' > preparation-microusd.json",
        "cp ../../observation.json observation.json",
        "cp ../../observation.json observation.json\nprintf '0' > preparation-microusd.json",
        "cp ../../observation.json observation.json\nprintf '0' > preparation-microusd.json",
        "exec /bin/sleep 20",
        "cp ../../observation.json observation.json\nprintf '0' > preparation-microusd.json",
    ]
    .into_iter()
    .enumerate()
    {
        let root = Scratch::new();
        let (mut manifest, mut observation) = fixture();
        if index == 7 {
            observation.provider_requests = 2;
        }
        if index == 8 {
            observation.result.token_usage = None;
        }
        if index == 9 {
            manifest.limits.task_seconds = 1;
        }
        if index == 10 {
            let usage = observation.result.token_usage.as_mut().unwrap();
            usage.cache_read_tokens = 20;
            usage.total_tokens = 30;
        }
        for asset in &manifest.assets {
            std::fs::write(root.0.join(&asset.path), b"fixture").unwrap();
        }
        let driver = root.0.join("driver.sh");
        std::fs::write(
            &driver,
            format!("#!/bin/sh\nset -eu\ngrep -q reserved ../dispatch-budget.jsonl\nprintf x >> ../dispatch-count\n{command}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o700)).unwrap();
        let driver_hash = c::digest(&std::fs::read(&driver).unwrap());
        observation.isolation.driver_sha256 = driver_hash.clone();
        exclusive_json(&root.0.join("observation.json"), &observation).unwrap();
        manifest.assets.last_mut().unwrap().sha256 = driver_hash.clone();
        std::fs::create_dir(root.0.join("suite")).unwrap();
        let items = manifest.cases.iter().map(|case| {
            let item = if case.track == c::Track::Knowledge {
                serde_json::json!({"eval_type":"grounded_answer","id":case.id,"question":"fixture"})
            } else {
                serde_json::json!({"eval_type":"agent_build_sequence","id":case.id,
                    "fixture":".","agent_command":"false","steps":(1..=3).map(|step|
                        serde_json::json!({"id":format!("step-{step}"),"prompt":"fixture"})).collect::<Vec<_>>()})
            };
            serde_json::to_string(&item).unwrap()
        }).collect::<Vec<_>>().join("\n");
        for (name, bytes) in [
            (
                "suite/suite.toml",
                "name = \"fixture\"\nitems = \"items.jsonl\"\n".to_string(),
            ),
            ("suite/items.jsonl", items),
        ] {
            std::fs::write(root.0.join(name), &bytes).unwrap();
            manifest.assets.push(c::Asset {
                path: name.into(),
                sha256: c::digest(bytes.as_bytes()),
                kind: c::AssetKind::Labels,
            });
        }
        let path = root.0.join("campaign.json");
        exclusive_json(&path, &manifest).unwrap();
        let mut plan = manifest.plan(Split::Development).unwrap();
        verify_campaign_inputs(&path, &plan.manifest_sha256).unwrap();
        plan.work.truncate(1); // One final dispatch; no subsequent iteration to detect mutation.
        let out = root.0.join("out");
        std::fs::create_dir(&out).unwrap();
        let mut artifact = Artifact {
            manifest_sha256: plan.manifest_sha256.clone(),
            split: Split::Development,
            offline: true,
            completed: false,
            preparation_microusd: None,
            approvals: vec![],
            observations: vec![],
        };
        let mut ledger = Ledger::default();
        let execution = execute(
            &path,
            &manifest,
            &driver,
            &driver_hash,
            &out,
            &plan,
            true,
            false,
            &mut ledger,
            &mut artifact,
        )
        .await;
        assert_eq!(
            execution.is_ok(),
            index == 5 || index == 10,
            "scenario {index}: {execution:?}"
        );
        assert_eq!(ledger.requests, 1);
        assert_eq!(
            artifact.observations.len(),
            usize::from(index == 5 || index == 10)
        );
        assert!(out.join("work-00000/reservation.json").exists());
        let mut budget = RequestBudget::resume_scoped(
            &out.join("dispatch-budget.jsonl"),
            &plan.manifest_sha256,
            manifest.limits.clone(),
            c::Limits {
                requests: 1,
                tokens: 200,
                microusd: 0,
                task_seconds: manifest.limits.task_seconds,
            },
        )
        .unwrap();
        if index == 5 || index == 10 {
            assert!(budget.uncertain_ids().is_empty());
            assert_eq!(budget.ledger().tokens, if index == 10 { 30 } else { 10 });
            let saved = ProviderResults::open(&out.join("dispatch-results"))
                .unwrap()
                .recover(&mut budget, "work-00000")
                .unwrap();
            assert_eq!(
                saved,
                serde_json::to_value(&artifact.observations[0]).unwrap()
            );
        } else {
            assert_eq!(budget.uncertain_ids(), ["work-00000"]);
            assert_eq!(budget.ledger().tokens, 200);
        }
        // An owning process excludes a second resume, even for settled output.
        let mut resumed_artifact = artifact.clone();
        resumed_artifact.observations.clear();
        let mut resumed_ledger = Ledger::default();
        assert!(
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                true,
                true,
                &mut resumed_ledger,
                &mut resumed_artifact
            )
            .await
            .is_err()
        );
        drop(budget);
        let resumed = execute(
            &path,
            &manifest,
            &driver,
            &driver_hash,
            &out,
            &plan,
            true,
            true,
            &mut resumed_ledger,
            &mut resumed_artifact,
        )
        .await;
        assert_eq!(
            resumed.is_ok(),
            index == 5 || index == 10,
            "resume {index}: {resumed:?}"
        );
        assert_eq!(
            std::fs::read(out.join("dispatch-count")).unwrap(),
            b"x",
            "no repeated driver execution"
        );
        if index == 5 || index == 10 {
            assert_eq!(
                c::digest_value(&resumed_artifact.observations).unwrap(),
                c::digest_value(&artifact.observations).unwrap()
            );
            // Simulate a crash after persisted output but before acknowledgement.
            let journal = out.join("dispatch-budget.jsonl");
            let original = std::fs::read_to_string(&journal).unwrap();
            let reserved = original.lines().take(2).collect::<Vec<_>>().join("\n") + "\n";
            std::fs::write(&journal, reserved).unwrap();
            resumed_artifact.observations.clear();
            resumed_ledger = Ledger::default();
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                true,
                true,
                &mut resumed_ledger,
                &mut resumed_artifact,
            )
            .await
            .unwrap();
            assert_eq!(std::fs::read_to_string(&journal).unwrap(), original);
            assert_eq!(std::fs::read(out.join("dispatch-count")).unwrap(), b"x");
            // Raw observation tampering cannot replace the bound receipt.
            std::fs::write(out.join("work-00000/observation.json"), b"{}").unwrap();
            resumed_artifact.observations.clear();
            resumed_ledger = Ledger::default();
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &plan,
                true,
                true,
                &mut resumed_ledger,
                &mut resumed_artifact,
            )
            .await
            .unwrap();
            assert_eq!(resumed_artifact.observations.len(), 1);
            let mut extended = manifest.plan(Split::Development).unwrap();
            extended.work.truncate(2);
            let mut next = observation.clone();
            next.work = extended.work[1].clone();
            next.result.item_id = next.work.case_id.clone();
            std::fs::write(
                root.0.join("observation.json"),
                serde_json::to_vec(&next).unwrap(),
            )
            .unwrap();
            resumed_artifact.observations.clear();
            resumed_ledger = Ledger::default();
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &extended,
                true,
                true,
                &mut resumed_ledger,
                &mut resumed_artifact,
            )
            .await
            .unwrap();
            assert_eq!(resumed_artifact.observations.len(), 2);
            assert_eq!(
                std::fs::read(out.join("dispatch-count")).unwrap(),
                b"xx",
                "only unstarted work dispatched"
            );
            resumed_artifact.observations.clear();
            resumed_ledger = Ledger::default();
            execute(
                &path,
                &manifest,
                &driver,
                &driver_hash,
                &out,
                &extended,
                true,
                true,
                &mut resumed_ledger,
                &mut resumed_artifact,
            )
            .await
            .unwrap();
            assert_eq!(std::fs::read(out.join("dispatch-count")).unwrap(), b"xx");
            std::fs::write(out.join("work-00000/request.json"), b"{}").unwrap();
            assert!(
                execute(
                    &path,
                    &manifest,
                    &driver,
                    &driver_hash,
                    &out,
                    &extended,
                    true,
                    true,
                    &mut resumed_ledger,
                    &mut resumed_artifact
                )
                .await
                .is_err()
            );
            assert_eq!(std::fs::read(out.join("dispatch-count")).unwrap(), b"xx");
        }
        assert_eq!(
            verify_campaign_inputs(&path, &plan.manifest_sha256).is_err(),
            index < 3
        );
    }
}
